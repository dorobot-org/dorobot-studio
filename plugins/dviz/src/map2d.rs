//! `DvizMap2D`: the navigation scene drawn from above, RViz style.
//!
//! The occupancy grid is one texture, built from [`grid_rgba`] only when the
//! grid changes and drawn as one quad; everything else is drawn again every
//! frame from the latest [`NavScene`] as vectors: the trail and the plan as
//! 2 px lines, the footprint as a closed outline with a short heading line,
//! the goal as an arrow (a cross when it has no yaw), and the scan as 2 px
//! dots. The wheel zooms about the pointer and a drag pans; the view can also
//! be fitted to the scene or follow the robot ([`MapCamera`]).

use crate::map_view::{grid_rgba, layer_color, scene_bounds, MapView, NavLayers};
use crate::nav_scene::{GridLayer, NavScene};
use dorobot_studio_plugin::*;
use dorobot_ux::tokens::pal;
use dviz_protocol::{nav_entities, Grid2DHeader};
use std::sync::Arc;

/// Width of every line, in pixels.
const LINE_PX: f32 = 2.0;
/// Side of a scan dot, in pixels.
const DOT_PX: f64 = 2.0;
/// Length of the heading line from the robot's pose, in metres, and the
/// least it is drawn at, in pixels.
const HEADING_M: f64 = 0.3;
const HEADING_MIN_PX: f64 = 10.0;
/// The goal arrow: the least its shaft is drawn at, and its head, in pixels.
const ARROW_MIN_PX: f64 = 16.0;
const ARROW_HEAD_PX: f64 = 8.0;
/// A goal without a direction (sent without a yaw): a cross this wide, in pixels.
const GOAL_MARK_PX: f64 = 12.0;
/// Scroll distance of one mouse wheel notch, and the zoom it gives.
const WHEEL_NOTCH: f64 = 32.0;
const ZOOM_PER_NOTCH: f64 = 1.2;
/// How far a press must move before it pans, in pixels; a click stays a click.
const DRAG_SLOP_PX: f64 = 3.0;
/// Shown when there is nothing to draw yet.
const WAITING: &str = "Waiting for map…";

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The occupancy grid: one texel a cell, read whole (no blending between
    // cells), whatever the zoom.
    let DrawMapGrid = set_type_default() do #(DrawMapGrid::script_shader(vm)){
        ..mod.draw.DrawQuad
        grid_tex: texture_2d(float)
        pixel: fn() {
            let size = self.grid_tex.size()
            // Commas: an argument that starts with "(" would otherwise call
            // the one before it.
            let cell = vec2(
                (floor(self.pos.x * size.x) + 0.5) / max(size.x, 1.0),
                (floor(self.pos.y * size.y) + 0.5) / max(size.y, 1.0)
            )
            let color = self.grid_tex.sample(cell)
            return vec4(color.rgb * color.a, color.a)
        }
    }

    // An anti-aliased segment from seg_a to seg_b inside its bounding quad,
    // as the graph's links (after the segment shader of Makepad's charts).
    let DrawMapLine = set_type_default() do #(DrawMapLine::script_shader(vm)){
        ..mod.draw.DrawQuad
        pixel: fn() {
            let p = self.pos * self.rect_size
            let pa = p - self.seg_a
            let ba = self.seg_b - self.seg_a
            let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0)
            let d = length(pa - ba * h)
            let aa = 1.0 - smoothstep(self.thickness * 0.5 - 0.75, self.thickness * 0.5 + 0.75, d)
            return vec4(self.color.rgb * self.color.a * aa, self.color.a * aa)
        }
    }

    mod.widgets.DvizMap2DBase = #(DvizMap2D::register_widget(vm))
    mod.widgets.DvizMap2D = set_type_default() do mod.widgets.DvizMap2DBase{
        width: Fill
        height: Fill
        draw_ground: mod.draw.DrawColor{}
        draw_grid: DrawMapGrid{}
        draw_dot: mod.draw.DrawColor{}
        draw_line: DrawMapLine{}
        draw_text +: {text_style: mod.widgets.ux.TEXT_BODY{}}
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMapGrid {
    #[deref]
    draw_super: DrawQuad,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMapLine {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    color: Vec4f,
    #[live]
    seg_a: Vec2f,
    #[live]
    seg_b: Vec2f,
    #[live(2.0)]
    thickness: f32,
}

impl DrawMapLine {
    /// A segment from `a` to `b` (absolute), drawn in its padded bounding box.
    fn segment(&mut self, cx: &mut Cx2d, a: Vec2d, b: Vec2d) {
        let pad = self.thickness as f64 + 1.0;
        let pos = dvec2(a.x.min(b.x) - pad, a.y.min(b.y) - pad);
        let size = dvec2((a.x - b.x).abs() + 2.0 * pad, (a.y - b.y).abs() + 2.0 * pad);
        self.seg_a = Vec2f {
            x: (a.x - pos.x) as f32,
            y: (a.y - pos.y) as f32,
        };
        self.seg_b = Vec2f {
            x: (b.x - pos.x) as f32,
            y: (b.y - pos.y) as f32,
        };
        self.draw_abs(cx, Rect { pos, size });
    }
}

/// How the view moves: by hand, kept fitted to the scene, or kept on the
/// robot. A new view is fitted, so the first scene shows whole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MapCamera {
    pub view: MapView,
    /// Keep showing the whole scene, until zoomed or dragged.
    pub fitted: bool,
    /// Keep the robot in the middle, until dragged; zooming keeps it.
    pub follow: bool,
}

impl Default for MapCamera {
    fn default() -> Self {
        Self {
            view: MapView::default(),
            fitted: true,
            follow: false,
        }
    }
}

impl MapCamera {
    /// Puts the view where fitting or following wants it for `scene`.
    pub fn track(&mut self, scene: &NavScene) {
        if self.fitted {
            if let Some((min, max)) = scene_bounds(scene) {
                self.view.fit(min, max);
            }
        }
        if self.follow {
            if let Some(pose) = &scene.pose {
                self.view.center = [pose.x, pose.y];
            }
        }
    }

    /// A drag by `ds` pixels: the view is the user's from now on.
    pub fn drag(&mut self, ds: [f64; 2]) {
        self.view.pan_by(ds);
        self.fitted = false;
        self.follow = false;
    }

    /// A zoom by `factor` about the screen point `s`; following goes on.
    pub fn zoom(&mut self, s: [f64; 2], factor: f64) {
        self.view.zoom_at(s, factor);
        self.fitted = false;
    }

    /// Fits the view to the scene from now on, instead of following.
    pub fn fit(&mut self) {
        self.fitted = true;
        self.follow = false;
    }

    pub fn set_follow(&mut self, on: bool) {
        self.follow = on;
        if on {
            self.fitted = false;
        }
    }
}

/// The zoom factor for a wheel scroll of `scroll` (Makepad's sign: positive
/// is a turn towards the user). A notch away from the user zooms in by
/// [`ZOOM_PER_NOTCH`], one towards the user out; a touchpad's small steps
/// zoom by a share of that.
pub(crate) fn wheel_factor(scroll: f64) -> f64 {
    if !scroll.is_finite() {
        return 1.0;
    }
    ZOOM_PER_NOTCH.powf(-(scroll / WHEEL_NOTCH).clamp(-1.0, 1.0))
}

/// The grid as it is on the GPU, and what it was built from.
struct GridPicture {
    header: Grid2DHeader,
    cells: Arc<Vec<u8>>,
    generation: u64,
    texture: Texture,
}

/// A press on the map, and whether it has moved far enough to pan.
struct Drag {
    start: Vec2d,
    last: Vec2d,
    panning: bool,
}

fn rgba(color: [u8; 4]) -> Vec4f {
    Vec4f {
        x: f32::from(color[0]) / 255.0,
        y: f32::from(color[1]) / 255.0,
        z: f32::from(color[2]) / 255.0,
        w: f32::from(color[3]) / 255.0,
    }
}

/// The ground around the map: a mid grey, darker in the dark theme. Neither
/// is the grid's unknown grey nor its free white, so the map's edge shows.
fn ground(light: f32) -> Vec4f {
    pal::mixv(
        pal::rgb(0x30, 0x30, 0x32),
        pal::rgb(0xAE, 0xAE, 0xB2),
        light,
    )
}

/// Whether the box from `a` to `b`, grown by `pad`, meets `rect`.
fn meets(rect: Rect, a: Vec2d, b: Vec2d, pad: f64) -> bool {
    a.x.max(b.x) + pad >= rect.pos.x
        && a.x.min(b.x) - pad <= rect.pos.x + rect.size.x
        && a.y.max(b.y) + pad >= rect.pos.y
        && a.y.min(b.y) - pad <= rect.pos.y + rect.size.y
}

/// A polyline through `points` (map metres), in `color`, closed back to its
/// first point if asked and not closed already. Segments outside `rect` are
/// left out.
fn polyline(
    line: &mut DrawMapLine,
    cx: &mut Cx2d,
    view: &MapView,
    rect: Rect,
    points: &[[f32; 2]],
    closed: bool,
    color: [u8; 4],
) {
    if points.len() < 2 {
        return;
    }
    let at = |p: &[f32; 2]| {
        let s = view.world_to_screen([f64::from(p[0]), f64::from(p[1])]);
        rect.pos + dvec2(s[0], s[1])
    };
    line.color = rgba(color);
    line.thickness = LINE_PX;
    let pad = f64::from(LINE_PX) + 1.0;
    let closing = (closed && points.first() != points.last()).then(|| &points[0]);
    let mut previous = at(&points[0]);
    for p in points[1..].iter().chain(closing) {
        let next = at(p);
        if meets(rect, previous, next, pad) {
            line.segment(cx, previous, next);
        }
        previous = next;
    }
}

/// The goal's strokes on screen, from its origin `from` (screen) and its
/// vector (map metres, x and y): an arrow along the vector, at least
/// [`ARROW_MIN_PX`] long, or, when the vector has no length on the map (a
/// goal sent without a yaw is a vertical vector), a cross [`GOAL_MARK_PX`]
/// wide. Nothing when a value is not finite.
fn goal_strokes(from: Vec2d, vector: [f32; 2], px_per_m: f64) -> Vec<(Vec2d, Vec2d)> {
    // Screen y runs down.
    let along = dvec2(f64::from(vector[0]), -f64::from(vector[1])) * px_per_m;
    let length = along.length();
    if !length.is_finite() || !from.x.is_finite() || !from.y.is_finite() {
        return Vec::new();
    }
    if length <= f64::EPSILON {
        let half = GOAL_MARK_PX / 2.0;
        return vec![
            (from + dvec2(-half, -half), from + dvec2(half, half)),
            (from + dvec2(-half, half), from + dvec2(half, -half)),
        ];
    }
    let direction = along / length;
    let to = from + direction * length.max(ARROW_MIN_PX);
    let across = dvec2(-direction.y, direction.x) * (ARROW_HEAD_PX / 2.0);
    let back = to - direction * ARROW_HEAD_PX;
    vec![(from, to), (to, back + across), (to, back - across)]
}

/// The navigation scene from above; see the module documentation.
#[derive(Script, ScriptHook, Widget)]
pub struct DvizMap2D {
    #[deref]
    view: View,
    #[live]
    draw_ground: DrawColor,
    #[live]
    draw_grid: DrawMapGrid,
    #[live]
    draw_dot: DrawColor,
    #[live]
    draw_line: DrawMapLine,
    #[live]
    draw_text: DrawText,
    #[rust]
    camera: MapCamera,
    #[rust]
    scene: NavScene,
    #[rust]
    layers: NavLayers,
    /// 0 dark, 1 light.
    #[rust]
    light: f32,
    #[rust]
    grid: Option<GridPicture>,
    /// Grid textures built so far.
    #[rust]
    texture_builds: u64,
    #[rust]
    drag: Option<Drag>,
}

impl DvizMap2D {
    /// Keeps the grid texture in step with `grid`: dropped without a grid,
    /// built again only when the grid changed. A new generation of the same
    /// grid (the bridge sends the static map again every few seconds) keeps
    /// the texture; the generation alone is not trusted, as it starts over
    /// after a reconnect.
    fn update_grid(&mut self, cx: &mut Cx, grid: Option<&GridLayer>) {
        let Some(layer) = grid else {
            self.grid = None;
            return;
        };
        if let Some(picture) = &mut self.grid {
            if picture.generation == layer.generation && Arc::ptr_eq(&picture.cells, &layer.cells) {
                return;
            }
            if picture.header == layer.header && picture.cells == layer.cells {
                picture.generation = layer.generation;
                picture.cells = layer.cells.clone();
                return;
            }
        }
        let pixels: Vec<u32> = grid_rgba(&layer.header, &layer.cells)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[r, g, b, a]| {
                (u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
            })
            .collect();
        let texture = Texture::new_with_format(
            cx,
            TextureFormat::VecBGRAu8_32 {
                data: Some(pixels),
                width: layer.header.width as usize,
                height: layer.header.height as usize,
                updated: TextureUpdated::Full,
            },
        );
        self.grid = Some(GridPicture {
            header: layer.header.clone(),
            cells: layer.cells.clone(),
            generation: layer.generation,
            texture,
        });
        self.texture_builds += 1;
    }

    /// The scan as dots of [`DOT_PX`].
    fn dots(&mut self, cx: &mut Cx2d, rect: Rect) {
        let view = self.camera.view;
        self.draw_dot.color = rgba(layer_color(&self.scene, nav_entities::SENSORS_SCAN));
        for p in &self.scene.scan {
            let s = view.world_to_screen([f64::from(p[0]), f64::from(p[1])]);
            let at = rect.pos + dvec2(s[0], s[1]);
            if meets(rect, at, at, DOT_PX) {
                let corner = at - dvec2(DOT_PX / 2.0, DOT_PX / 2.0);
                self.draw_dot.draw_abs(
                    cx,
                    Rect {
                        pos: corner,
                        size: dvec2(DOT_PX, DOT_PX),
                    },
                );
            }
        }
    }

    /// The footprint's outline and, from the pose, a short heading line.
    fn robot(&mut self, cx: &mut Cx2d, rect: Rect) {
        let color = layer_color(&self.scene, nav_entities::ROBOT_FOOTPRINT);
        let view = self.camera.view;
        let footprint = &self.scene.footprint;
        polyline(&mut self.draw_line, cx, &view, rect, footprint, true, color);
        let Some(pose) = &self.scene.pose else {
            return;
        };
        let s = view.world_to_screen([pose.x, pose.y]);
        let from = rect.pos + dvec2(s[0], s[1]);
        let length = (HEADING_M * view.px_per_m).max(HEADING_MIN_PX);
        // Screen y runs down.
        let to = from + dvec2(pose.yaw.cos(), -pose.yaw.sin()) * length;
        if meets(rect, from, to, f64::from(LINE_PX)) {
            self.draw_line.color = rgba(color);
            self.draw_line.thickness = LINE_PX;
            self.draw_line.segment(cx, from, to);
        }
    }

    /// The goal: an arrow from its origin along its vector, or a cross when
    /// it has no direction ([`goal_strokes`]).
    fn goal(&mut self, cx: &mut Cx2d, rect: Rect) {
        let Some((origin, vector)) = self.scene.goal else {
            return;
        };
        let view = self.camera.view;
        let s = view.world_to_screen([f64::from(origin[0]), f64::from(origin[1])]);
        let from = rect.pos + dvec2(s[0], s[1]);
        let strokes = goal_strokes(from, vector, view.px_per_m);
        if !strokes
            .iter()
            .any(|(a, b)| meets(rect, *a, *b, f64::from(LINE_PX)))
        {
            return;
        }
        self.draw_line.color = rgba(layer_color(&self.scene, nav_entities::NAV_GOAL));
        self.draw_line.thickness = LINE_PX;
        for (a, b) in strokes {
            self.draw_line.segment(cx, a, b);
        }
    }

    /// The grid texture over the map's own rectangle.
    fn grid(&mut self, cx: &mut Cx2d, rect: Rect) {
        let Some(picture) = &self.grid else {
            return;
        };
        let header = &picture.header;
        let resolution = f64::from(header.resolution);
        let (x0, y0) = (f64::from(header.origin[0]), f64::from(header.origin[1]));
        let (x1, y1) = (
            x0 + f64::from(header.width) * resolution,
            y0 + f64::from(header.height) * resolution,
        );
        let view = self.camera.view;
        // Texture row 0 is the largest y, at the top.
        let top_left = view.world_to_screen([x0, y1]);
        let bottom_right = view.world_to_screen([x1, y0]);
        let pos = rect.pos + dvec2(top_left[0], top_left[1]);
        let size = dvec2(bottom_right[0] - top_left[0], bottom_right[1] - top_left[1]);
        if size.x <= 0.0 || size.y <= 0.0 || !meets(rect, pos, pos + size, 0.0) {
            return;
        }
        self.draw_grid.draw_vars.set_texture(0, &picture.texture);
        self.draw_grid.draw_abs(cx, Rect { pos, size });
    }

    /// [`WAITING`] in the middle of `rect`.
    fn waiting(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_text.color = pal::Th { l: self.light }.ink();
        let laid = self
            .draw_text
            .layout(cx, 0.0, 0.0, None, false, Align::default(), WAITING);
        let scale = f64::from(self.draw_text.font_scale);
        let size = dvec2(
            f64::from(laid.size_in_lpxs.width) * scale,
            f64::from(laid.size_in_lpxs.height) * scale,
        );
        self.draw_text
            .draw_abs(cx, rect.pos + (rect.size - size) / 2.0, WAITING);
    }

    /// Puts the view where the camera wants it and redraws.
    fn moved(&mut self, cx: &mut Cx) {
        self.camera.track(&self.scene);
        self.view.redraw(cx);
    }
}

impl Widget for DvizMap2D {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let area = self.view.area();
        match event.hits(cx, area) {
            Hit::FingerHoverIn(_) => cx.set_cursor(MouseCursor::Grab),
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                self.drag = Some(Drag {
                    start: fe.abs,
                    last: fe.abs,
                    panning: false,
                });
                cx.set_cursor(MouseCursor::Grabbing);
            }
            Hit::FingerMove(fe) => {
                let Some(drag) = &mut self.drag else {
                    return;
                };
                if !drag.panning && (fe.abs - drag.start).length() < DRAG_SLOP_PX {
                    return;
                }
                drag.panning = true;
                let delta = fe.abs - drag.last;
                drag.last = fe.abs;
                self.camera.drag([delta.x, delta.y]);
                self.moved(cx);
            }
            Hit::FingerUp(_) => {
                self.drag = None;
                cx.set_cursor(MouseCursor::Grab);
            }
            Hit::FingerScroll(fs) => {
                // The zoom takes the whole gesture, so nothing around scrolls with it.
                let owner = match event {
                    Event::Scroll(se) => {
                        cx.is_scroll_owner(se, area, Vec2Index::Y, true)
                            || cx.is_scroll_owner(se, area, Vec2Index::X, true)
                    }
                    _ => false,
                };
                if !owner {
                    return;
                }
                let scroll = if fs.scroll.y.abs() > f64::EPSILON {
                    fs.scroll.y
                } else {
                    fs.scroll.x
                };
                let origin = area.rect(cx).pos;
                let at = fs.abs - origin;
                self.camera.zoom([at.x, at.y], wheel_factor(scroll));
                self.moved(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.view.visible() {
            return DrawStep::done();
        }
        while self.view.draw_walk(cx, scope, walk).step().is_some() {}
        let rect = self.view.area().rect(cx);
        let size = [rect.size.x, rect.size.y];
        if size[0] > 0.0 && size[1] > 0.0 && self.camera.view.size != size {
            self.camera.view.size = size;
            self.camera.track(&self.scene);
        }
        cx.push_clip_rect(rect);
        self.draw_ground.color = ground(self.light);
        self.draw_ground.draw_abs(cx, rect);
        if self.layers.map {
            self.grid(cx, rect);
        }
        // Back to front: grid, scan, then the lines (trail, plan, robot,
        // goal). Each draw struct is one draw call, in the order first used;
        // within the lines' call, a later line is on top.
        if self.layers.scan {
            self.dots(cx, rect);
        }
        let view = self.camera.view;
        for (on, points, entity) in [
            (
                self.layers.trail,
                &self.scene.trail,
                nav_entities::NAV_TRAIL,
            ),
            (self.layers.plan, &self.scene.plan, nav_entities::NAV_PLAN),
        ] {
            if on {
                let color = layer_color(&self.scene, entity);
                polyline(&mut self.draw_line, cx, &view, rect, points, false, color);
            }
        }
        if self.layers.footprint {
            self.robot(cx, rect);
        }
        if self.layers.goal {
            self.goal(cx, rect);
        }
        if scene_bounds(&self.scene).is_none() {
            self.waiting(cx, rect);
        }
        cx.pop_clip_rect();
        DrawStep::done()
    }
}

impl DvizMap2DRef {
    /// Shows `scene`. The grid texture is rebuilt only when the grid
    /// changed; a scene already shown (same update count, same grid) is not
    /// copied again.
    pub fn set_scene(&self, cx: &mut Cx, scene: &NavScene) {
        let Some(mut map) = self.borrow_mut() else {
            return;
        };
        let same_grid = match (&map.scene.grid, &scene.grid) {
            (None, None) => true,
            (Some(a), Some(b)) => a.generation == b.generation && Arc::ptr_eq(&a.cells, &b.cells),
            _ => false,
        };
        if same_grid && map.scene.updated == scene.updated {
            return;
        }
        map.update_grid(cx, scene.grid.as_ref());
        map.scene = scene.clone();
        map.moved(cx);
    }

    pub fn set_layers(&self, cx: &mut Cx, layers: NavLayers) {
        if let Some(mut map) = self.borrow_mut() {
            if map.layers != layers {
                map.layers = layers;
                map.view.redraw(cx);
            }
        }
    }

    /// Fits the view to the scene (map bounds, or the robot and its paths),
    /// and keeps it fitted until it is zoomed or dragged. Stops following.
    pub fn fit(&self, cx: &mut Cx) {
        if let Some(mut map) = self.borrow_mut() {
            map.camera.fit();
            map.moved(cx);
        }
    }

    /// Keeps the robot in the middle of the view, or stops doing so. A drag
    /// on the map also stops it.
    pub fn set_follow(&self, cx: &mut Cx, on: bool) {
        if let Some(mut map) = self.borrow_mut() {
            map.camera.set_follow(on);
            map.moved(cx);
        }
    }

    /// The theme: 0 dark, 1 light.
    pub fn set_light(&self, cx: &mut Cx, light: f32) {
        if let Some(mut map) = self.borrow_mut() {
            if map.light != light {
                map.light = light;
                map.view.redraw(cx);
            }
        }
    }

    pub fn view(&self) -> MapView {
        self.borrow()
            .map_or_else(MapView::default, |map| map.camera.view)
    }

    pub fn follow(&self) -> bool {
        self.borrow().is_some_and(|map| map.camera.follow)
    }

    pub fn layers(&self) -> NavLayers {
        self.borrow()
            .map_or_else(NavLayers::default, |map| map.layers)
    }

    pub fn light(&self) -> f32 {
        self.borrow().map_or(0.0, |map| map.light)
    }

    /// Whether a grid texture is held.
    pub fn has_texture(&self) -> bool {
        self.borrow().is_some_and(|map| map.grid.is_some())
    }

    /// How many grid textures have been built.
    pub fn texture_builds(&self) -> u64 {
        self.borrow().map_or(0, |map| map.texture_builds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav_scene::Pose2D;

    fn robot_at(x: f64, y: f64) -> NavScene {
        NavScene {
            pose: Some(Pose2D { x, y, yaw: 0.0 }),
            trail: vec![[x as f32, y as f32], [x as f32 + 6.0, y as f32]],
            ..NavScene::default()
        }
    }

    #[test]
    fn a_wheel_notch_zooms_by_1_2() {
        assert!((wheel_factor(-WHEEL_NOTCH) - 1.2).abs() < 1e-12);
        assert!((wheel_factor(WHEEL_NOTCH) - 1.0 / 1.2).abs() < 1e-12);
        // A larger step is still one notch; a touchpad's small one a share.
        assert!((wheel_factor(-120.0) - 1.2).abs() < 1e-12);
        let small = wheel_factor(-8.0);
        assert!(small > 1.0 && small < 1.2, "{small}");
        assert_eq!(wheel_factor(0.0), 1.0);
        assert_eq!(wheel_factor(f64::NAN), 1.0);
    }

    #[test]
    fn a_new_camera_fits_the_scene_until_zoomed() {
        let mut camera = MapCamera::default();
        let scene = robot_at(2.0, 3.0);
        camera.track(&scene);
        let fitted = camera.view;
        let (min, max) = scene_bounds(&scene).unwrap();
        assert_eq!(
            fitted.center,
            [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0]
        );
        camera.zoom([10.0, 10.0], 2.0);
        assert!(!camera.fitted);
        let zoomed = camera.view;
        camera.track(&robot_at(30.0, 30.0));
        assert_eq!(camera.view, zoomed, "a zoomed view stays where it is");
        camera.fit();
        camera.track(&scene);
        assert_eq!(camera.view, fitted);
    }

    #[test]
    fn a_drag_turns_following_off_and_a_zoom_does_not() {
        let mut camera = MapCamera::default();
        camera.set_follow(true);
        assert!(camera.follow && !camera.fitted);
        camera.track(&robot_at(5.0, -1.0));
        assert_eq!(camera.view.center, [5.0, -1.0]);
        camera.zoom([0.0, 0.0], 1.5);
        camera.track(&robot_at(5.0, -1.0));
        assert!(camera.follow);
        assert_eq!(camera.view.center, [5.0, -1.0]);
        camera.drag([40.0, 0.0]);
        assert!(!camera.follow && !camera.fitted);
        let dragged = camera.view;
        camera.track(&robot_at(9.0, 9.0));
        assert_eq!(camera.view, dragged);
        // Fitting stops following too.
        camera.set_follow(true);
        camera.fit();
        assert!(!camera.follow && camera.fitted);
    }

    /// A goal sent without a yaw has a vertical vector, flat on the map: it is
    /// a cross of GOAL_MARK_PX at the goal; with a yaw an arrow along it.
    #[test]
    fn a_goal_without_a_yaw_is_a_cross_and_with_one_an_arrow() {
        let at = dvec2(100.0, 50.0);
        let near = |a: Vec2d, b: Vec2d| (a - b).length() < 1e-9;
        let cross = goal_strokes(at, [0.0, 0.0], 50.0);
        assert_eq!(cross.len(), 2, "{cross:?}");
        let half = GOAL_MARK_PX / 2.0;
        assert!(
            near(cross[0].0, at + dvec2(-half, -half)) && near(cross[0].1, at + dvec2(half, half)),
            "{cross:?}"
        );
        assert!(
            near(cross[1].0, at + dvec2(-half, half)) && near(cross[1].1, at + dvec2(half, -half)),
            "{cross:?}"
        );
        assert!((10.0..=12.0).contains(&GOAL_MARK_PX));
        // 0.5 m east at 50 px/m: a 25 px shaft and a head at its end.
        let east = goal_strokes(at, [0.5, 0.0], 50.0);
        assert_eq!(east.len(), 3);
        assert!(
            near(east[0].0, at) && near(east[0].1, at + dvec2(25.0, 0.0)),
            "{east:?}"
        );
        assert!(east[1..]
            .iter()
            .all(|(a, _)| near(*a, at + dvec2(25.0, 0.0))));
        // North is up on screen; a short one is drawn ARROW_MIN_PX long.
        let north = goal_strokes(at, [0.0, 0.1], 50.0);
        assert!(
            near(north[0].1, at + dvec2(0.0, -ARROW_MIN_PX)),
            "{north:?}"
        );
        assert!(goal_strokes(at, [f32::NAN, 0.0], 50.0).is_empty());
        assert!(goal_strokes(at, [0.5, 0.0], f64::NAN).is_empty());
    }

    /// The grid and line shaders compile for the desktop backends (no GPU is
    /// needed: this is the fork's own shader compiler).
    #[test]
    fn map_shaders_compile() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(|vm| {
            dorobot_studio_core::register_ui(vm);
            script_mod(vm);
            assert!(vm.take_errors().is_empty());
            let sources = [
                ("grid glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_grid, "glsl", false)})),
                ("grid metal", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_grid, "metal", false)})),
                ("line glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_line, "glsl", false)})),
                ("line metal", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_line, "metal", false)})),
                ("dot glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_dot, "glsl", false)})),
            ];
            for (name, value) in sources {
                let source = vm
                    .heap()
                    .string_with(value, |_, text| text.to_string())
                    .unwrap_or_else(|| panic!("{name}: no source"));
                assert!(
                    !source.is_empty() && !source.starts_with("ERRORS"),
                    "{name}: {source}"
                );
            }
            assert!(vm.take_errors().is_empty());
        });
    }
}
