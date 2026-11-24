//! `DvizMap2D`: the navigation scene drawn from above, RViz style.
//!
//! A dark canvas (the same in both themes), then, bottom to top
//! ([`draw_plan`]): the occupancy grid and the costmap as textures in RViz's
//! palettes, each built only when its grid changes and drawn as one quad; a
//! 1 m grid; then, drawn again every frame from the latest [`NavScene`] as
//! vectors, the plan and the trail, the scan as square dots, the footprint
//! (filled, its outline red while localization is not trusted), a red pose
//! arrow and the goal as a ring with an arrow when it has a yaw. The wheel
//! zooms about the pointer and a drag pans; the view can also be fitted to
//! the scene or follow the robot ([`MapCamera`]).

use crate::map_view::{
    costmap_rgba, grid_lines, grid_rgba, layer_color, scene_bounds, MapView, NavLayers,
};
use crate::nav_scene::{GridLayer, NavScene, Pose2D};
use dorobot_studio_plugin::*;
use dorobot_ux::tokens::pal;
use dviz_protocol::{nav_entities, nav_status_parts, Grid2DHeader, NavStatus};
use std::sync::Arc;

/// The canvas under everything, in both themes.
const CANVAS: [u8; 4] = [0x30, 0x30, 0x30, 255];
/// The grid: a line every metre, this colour, this wide in pixels.
const GRID_M: f64 = 1.0;
const GRID_COLOR: [u8; 4] = [160, 160, 164, 128];
const GRID_LINE_PX: f64 = 1.0;
/// Line widths, in pixels: the plan, the trail, the footprint's outline, and
/// the pose arrow and the goal (ring and arrow).
const PLAN_PX: f32 = 4.0;
const TRAIL_PX: f32 = 3.0;
const FOOTPRINT_PX: f32 = 2.0;
const MARK_PX: f32 = 2.0;
/// Side of a scan dot, in pixels.
const DOT_PX: f64 = 3.0;
/// Inside the footprint, and its outline while localization is not trusted.
const FOOTPRINT_FILL: [u8; 4] = [40, 40, 40, 120];
const UNTRUSTED: [u8; 4] = [255, 0, 0, 255];
/// The most corners the footprint's fill takes (four `vec4`s in its shader);
/// a footprint with more is outlined only.
const FILL_CORNERS: usize = 8;
/// The pose arrow: its colour and its length on the map, in metres.
const POSE_ARROW_COLOR: [u8; 4] = [255, 25, 0, 255];
const POSE_ARROW_M: f64 = 0.4;
/// Arrows: the least the shaft is drawn at, and the head, in pixels.
const ARROW_MIN_PX: f64 = 16.0;
const ARROW_HEAD_PX: f64 = 8.0;
/// The goal's ring: its radius on the map, the least it is drawn at in
/// pixels, and its segments.
const GOAL_RING_M: f64 = 0.15;
const GOAL_RING_MIN_PX: f64 = 10.0;
const GOAL_RING_SEGMENTS: usize = 24;
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

    // A grid (the occupancy map or the costmap): one texel a cell, read
    // whole (no blending between cells), whatever the zoom.
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

    // A polygon of up to 8 corners, filled inside its bounding quad. The
    // corners are in pixels from the quad's top-left, two to a vec4
    // (corners_01 holds corners 0 and 1); unused ones repeat corner 0, so
    // the edge from the last corner back to the first closes it. A point is
    // inside when a ray from it towards +x crosses the edges an odd number
    // of times. No anti-aliasing: the outline is drawn over the edge.
    let DrawMapFill = set_type_default() do #(DrawMapFill::script_shader(vm)){
        ..mod.draw.DrawQuad
        // 1 when the edge from a to b crosses the ray from p towards +x.
        crossing: fn(p: vec2, a: vec2, b: vec2) -> f32 {
            if abs(step(p.y, a.y) - step(p.y, b.y)) < 0.5 { return 0.0 }
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y)
            return step(p.x, x)
        }
        pixel: fn() {
            let p = self.pos * self.rect_size
            var n = 0.0
            n = n + self.crossing(p, self.corners_01.xy, self.corners_01.zw)
            n = n + self.crossing(p, self.corners_01.zw, self.corners_23.xy)
            n = n + self.crossing(p, self.corners_23.xy, self.corners_23.zw)
            n = n + self.crossing(p, self.corners_23.zw, self.corners_45.xy)
            n = n + self.crossing(p, self.corners_45.xy, self.corners_45.zw)
            n = n + self.crossing(p, self.corners_45.zw, self.corners_67.xy)
            n = n + self.crossing(p, self.corners_67.xy, self.corners_67.zw)
            n = n + self.crossing(p, self.corners_67.zw, self.corners_01.xy)
            let inside = n - 2.0 * floor(n * 0.5)
            return vec4(self.color.rgb * self.color.a * inside, self.color.a * inside)
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
        draw_map: DrawMapGrid{}
        draw_costmap: DrawMapGrid{}
        draw_rule: mod.draw.DrawColor{}
        draw_dot: mod.draw.DrawColor{}
        draw_fill: DrawMapFill{}
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
pub struct DrawMapFill {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    color: Vec4f,
    #[live]
    corners_01: Vec4f,
    #[live]
    corners_23: Vec4f,
    #[live]
    corners_45: Vec4f,
    #[live]
    corners_67: Vec4f,
}

impl DrawMapFill {
    /// Fills the polygon through `corners` (absolute; 3 to [`FILL_CORNERS`])
    /// in its bounding box, unless that box has no area or is outside `rect`.
    fn polygon(&mut self, cx: &mut Cx2d, rect: Rect, corners: &[Vec2d]) {
        let Some(&first) = corners.first() else {
            return;
        };
        if corners.len() > FILL_CORNERS {
            return;
        }
        let (min, max) = corners.iter().fold((first, first), |(min, max), p| {
            (
                dvec2(min.x.min(p.x), min.y.min(p.y)),
                dvec2(max.x.max(p.x), max.y.max(p.y)),
            )
        });
        let size = max - min;
        if !(size.x > 0.0 && size.y > 0.0) || !meets(rect, min, max, 0.0) {
            return;
        }
        let local = |i: usize| *corners.get(i).unwrap_or(&first) - min;
        let pair = |i: usize| {
            let (a, b) = (local(i), local(i + 1));
            vec4(a.x as f32, a.y as f32, b.x as f32, b.y as f32)
        };
        self.corners_01 = pair(0);
        self.corners_23 = pair(2);
        self.corners_45 = pair(4);
        self.corners_67 = pair(6);
        self.draw_abs(cx, Rect { pos: min, size });
    }
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

/// How a line is drawn: its width in pixels and its colour.
#[derive(Debug, Clone, Copy)]
struct Pen {
    width: f32,
    color: [u8; 4],
}

impl DrawMapLine {
    /// Draws the following segments with `pen`.
    fn take(&mut self, pen: Pen) {
        self.color = rgba(pen.color);
        self.thickness = pen.width;
    }

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

/// One step of a frame of the map. A frame draws them in this order, each
/// over the ones before it ([`draw_plan`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapDraw {
    /// The dark canvas, the same in both themes.
    Canvas,
    /// The occupancy grid's texture (RViz map palette).
    Map,
    /// The costmap's texture (RViz costmap palette), over the map.
    Costmap,
    /// The 1 m grid lines.
    Grid,
    Plan,
    Trail,
    /// The scan's square dots.
    Scan,
    /// The footprint's translucent fill, then its outline (red while
    /// localization is not trusted, [`footprint_color`]).
    FootprintFill,
    FootprintEdge,
    /// The red arrow from the robot's pose ([`pose_arrow`]).
    PoseArrow,
    /// The goal's ring, with an arrow when it has a yaw ([`goal_ring`]).
    Goal,
    /// "Waiting for map…", while there is nothing to show.
    Waiting,
}

/// What a frame draws for `scene` with the legend's `layers`, bottom to top:
/// the canvas and the grid always; every other step when its layer is on
/// and there is something to draw. The legend's Robot switch covers the
/// footprint and the pose arrow.
pub fn draw_plan(scene: &NavScene, layers: &NavLayers) -> Vec<MapDraw> {
    let steps = [
        (MapDraw::Canvas, true),
        (MapDraw::Map, layers.map && scene.grid.is_some()),
        (MapDraw::Costmap, layers.costmap && scene.costmap.is_some()),
        (MapDraw::Grid, true),
        (MapDraw::Plan, layers.plan && scene.plan.len() >= 2),
        (MapDraw::Trail, layers.trail && scene.trail.len() >= 2),
        (MapDraw::Scan, layers.scan && !scene.scan.is_empty()),
        (
            MapDraw::FootprintFill,
            layers.footprint && fill_corners(&scene.footprint).is_some(),
        ),
        (
            MapDraw::FootprintEdge,
            layers.footprint && scene.footprint.len() >= 2,
        ),
        (MapDraw::PoseArrow, layers.footprint && scene.pose.is_some()),
        (MapDraw::Goal, layers.goal && scene.goal.is_some()),
        (MapDraw::Waiting, scene_bounds(scene).is_none()),
    ];
    steps
        .into_iter()
        .filter_map(|(step, on)| on.then_some(step))
        .collect()
}

/// The footprint outline's colour: `normal` (the bridge's, or the default),
/// or red while localization is not trusted (`loc_ok` false) or its input
/// has stopped (`stale` names it). Without a status yet, `normal`.
pub fn footprint_color(status: Option<&NavStatus>, normal: [u8; 4]) -> [u8; 4] {
    let untrusted = status.is_some_and(|status| {
        !status.loc_ok
            || status
                .stale
                .iter()
                .any(|part| part == nav_status_parts::LOCALIZATION)
    });
    if untrusted {
        UNTRUSTED
    } else {
        normal
    }
}

/// The pose arrow, in map metres: from the robot's position
/// [`POSE_ARROW_M`] ahead along its yaw.
pub fn pose_arrow(pose: &Pose2D) -> ([f64; 2], [f64; 2]) {
    let (sin, cos) = pose.yaw.sin_cos();
    (
        [pose.x, pose.y],
        [pose.x + POSE_ARROW_M * cos, pose.y + POSE_ARROW_M * sin],
    )
}

/// The goal's ring around the screen point `center`: [`GOAL_RING_M`] at
/// `px_per_m`, at least [`GOAL_RING_MIN_PX`], as [`GOAL_RING_SEGMENTS`]
/// segments through screen points, closed (the first point again at the end).
pub fn goal_ring(center: [f64; 2], px_per_m: f64) -> Vec<[f64; 2]> {
    let radius = (GOAL_RING_M * px_per_m).max(GOAL_RING_MIN_PX);
    let step = std::f64::consts::TAU / GOAL_RING_SEGMENTS as f64;
    let mut ring: Vec<[f64; 2]> = (0..GOAL_RING_SEGMENTS)
        .map(|i| {
            let (sin, cos) = (step * i as f64).sin_cos();
            [center[0] + radius * cos, center[1] + radius * sin]
        })
        .collect();
    ring.push(ring[0]);
    ring
}

/// The corners of the footprint's fill: its points without a closing one,
/// when there are 3 to [`FILL_CORNERS`].
fn fill_corners(points: &[[f32; 2]]) -> Option<&[[f32; 2]]> {
    let corners = match points {
        [first, .., last] if first == last => &points[..points.len() - 1],
        _ => points,
    };
    (3..=FILL_CORNERS)
        .contains(&corners.len())
        .then_some(corners)
}

/// An arrow on screen from `from` along `along` (pixels): the shaft at
/// least [`ARROW_MIN_PX`] long and a head [`ARROW_HEAD_PX`] long at its tip.
/// Nothing without a direction or when a value is not finite.
fn arrow_strokes(from: Vec2d, along: Vec2d) -> Vec<(Vec2d, Vec2d)> {
    let length = along.length();
    let finite = length.is_finite() && from.x.is_finite() && from.y.is_finite();
    if !(finite && length > f64::EPSILON) {
        return Vec::new();
    }
    let direction = along / length;
    let to = from + direction * length.max(ARROW_MIN_PX);
    let across = dvec2(-direction.y, direction.x) * (ARROW_HEAD_PX / 2.0);
    let back = to - direction * ARROW_HEAD_PX;
    vec![(from, to), (to, back + across), (to, back - across)]
}

/// The goal's strokes on screen, from its origin `from` (screen) and its
/// vector (map metres, x and y): its ring ([`goal_ring`]), then an arrow
/// along the vector ([`arrow_strokes`]). A goal sent without a yaw has a
/// vertical vector, no length on the map: the ring only. Nothing when a
/// value is not finite.
fn goal_strokes(from: Vec2d, vector: [f32; 2], px_per_m: f64) -> Vec<(Vec2d, Vec2d)> {
    // Screen y runs down.
    let along = dvec2(f64::from(vector[0]), -f64::from(vector[1])) * px_per_m;
    if !(along.x.is_finite() && along.y.is_finite() && from.x.is_finite() && from.y.is_finite()) {
        return Vec::new();
    }
    let ring = goal_ring([from.x, from.y], px_per_m);
    let mut strokes: Vec<(Vec2d, Vec2d)> = ring
        .windows(2)
        .map(|w| (dvec2(w[0][0], w[0][1]), dvec2(w[1][0], w[1][1])))
        .collect();
    strokes.extend(arrow_strokes(from, along));
    strokes
}

/// A grid as it is on the GPU, and what it was built from.
struct GridPicture {
    header: Grid2DHeader,
    cells: Arc<Vec<u8>>,
    generation: u64,
    texture: Texture,
}

/// Keeps `picture` in step with `layer`: dropped without a layer, built again
/// (coloured by `palette`) only when the grid changed. A new generation of the
/// same grid (the bridge sends the map and the costmap again every 5 s)
/// keeps the texture; the generation alone is not trusted, as it starts over
/// after a reconnect. Returns whether a texture was built.
fn sync_picture(
    cx: &mut Cx,
    picture: &mut Option<GridPicture>,
    layer: Option<&GridLayer>,
    palette: fn(&Grid2DHeader, &[u8]) -> Vec<u8>,
) -> bool {
    let Some(layer) = layer else {
        *picture = None;
        return false;
    };
    if let Some(picture) = picture {
        if picture.generation == layer.generation && Arc::ptr_eq(&picture.cells, &layer.cells) {
            return false;
        }
        if picture.header == layer.header && picture.cells == layer.cells {
            picture.generation = layer.generation;
            picture.cells = layer.cells.clone();
            return false;
        }
    }
    let pixels: Vec<u32> = palette(&layer.header, &layer.cells)
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
    *picture = Some(GridPicture {
        header: layer.header.clone(),
        cells: layer.cells.clone(),
        generation: layer.generation,
        texture,
    });
    true
}

/// `picture` over its grid's own rectangle of the map, when that is in `rect`.
fn draw_picture(
    draw: &mut DrawMapGrid,
    cx: &mut Cx2d,
    view: &MapView,
    rect: Rect,
    picture: &GridPicture,
) {
    let header = &picture.header;
    let resolution = f64::from(header.resolution);
    let (x0, y0) = (f64::from(header.origin[0]), f64::from(header.origin[1]));
    let (x1, y1) = (
        x0 + f64::from(header.width) * resolution,
        y0 + f64::from(header.height) * resolution,
    );
    // Texture row 0 is the largest y, at the top.
    let pos = on_screen(view, rect, [x0, y1]);
    let size = on_screen(view, rect, [x1, y0]) - pos;
    if size.x <= 0.0 || size.y <= 0.0 || !meets(rect, pos, pos + size, 0.0) {
        return;
    }
    draw.draw_vars.set_texture(0, &picture.texture);
    draw.draw_abs(cx, Rect { pos, size });
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

/// Where the map point `p` (metres) is on screen, in absolute pixels.
fn on_screen(view: &MapView, rect: Rect, p: [f64; 2]) -> Vec2d {
    let s = view.world_to_screen(p);
    rect.pos + dvec2(s[0], s[1])
}

/// Whether the box from `a` to `b`, grown by `pad`, meets `rect`.
fn meets(rect: Rect, a: Vec2d, b: Vec2d, pad: f64) -> bool {
    a.x.max(b.x) + pad >= rect.pos.x
        && a.x.min(b.x) - pad <= rect.pos.x + rect.size.x
        && a.y.max(b.y) + pad >= rect.pos.y
        && a.y.min(b.y) - pad <= rect.pos.y + rect.size.y
}

/// A polyline through `points` (map metres) with `pen`, closed back to its
/// first point if asked and not closed already. Segments outside `rect` are
/// left out.
fn polyline(
    line: &mut DrawMapLine,
    cx: &mut Cx2d,
    view: &MapView,
    rect: Rect,
    points: &[[f32; 2]],
    closed: bool,
    pen: Pen,
) {
    if points.len() < 2 {
        return;
    }
    let at = |p: &[f32; 2]| on_screen(view, rect, [f64::from(p[0]), f64::from(p[1])]);
    line.take(pen);
    let pad = f64::from(pen.width) + 1.0;
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

/// `segments` (absolute pixels) with `pen`; those outside `rect` are left out.
fn strokes(
    line: &mut DrawMapLine,
    cx: &mut Cx2d,
    rect: Rect,
    segments: &[(Vec2d, Vec2d)],
    pen: Pen,
) {
    line.take(pen);
    let pad = f64::from(pen.width) + 1.0;
    for &(a, b) in segments {
        if meets(rect, a, b, pad) {
            line.segment(cx, a, b);
        }
    }
}

/// The navigation scene from above; see the module documentation.
#[derive(Script, ScriptHook, Widget)]
pub struct DvizMap2D {
    #[deref]
    view: View,
    #[live]
    draw_ground: DrawColor,
    #[live]
    draw_map: DrawMapGrid,
    #[live]
    draw_costmap: DrawMapGrid,
    #[live]
    draw_rule: DrawColor,
    #[live]
    draw_dot: DrawColor,
    #[live]
    draw_fill: DrawMapFill,
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
    #[rust]
    grid: Option<GridPicture>,
    #[rust]
    costmap: Option<GridPicture>,
    /// Grid and costmap textures built so far.
    #[rust]
    texture_builds: u64,
    #[rust]
    costmap_builds: u64,
    #[rust]
    drag: Option<Drag>,
}

impl DvizMap2D {
    /// Keeps the map and costmap textures in step with `scene`
    /// ([`sync_picture`]).
    fn update_pictures(&mut self, cx: &mut Cx, scene: &NavScene) {
        if sync_picture(cx, &mut self.grid, scene.grid.as_ref(), grid_rgba) {
            self.texture_builds += 1;
        }
        if sync_picture(cx, &mut self.costmap, scene.costmap.as_ref(), costmap_rgba) {
            self.costmap_builds += 1;
        }
    }

    /// One step of the frame ([`draw_plan`]) in `rect`.
    fn draw_step(&mut self, cx: &mut Cx2d, rect: Rect, step: MapDraw) {
        let view = self.camera.view;
        let scene = &self.scene;
        match step {
            MapDraw::Canvas => {
                self.draw_ground.color = rgba(CANVAS);
                self.draw_ground.draw_abs(cx, rect);
            }
            MapDraw::Map => {
                if let Some(picture) = &self.grid {
                    draw_picture(&mut self.draw_map, cx, &view, rect, picture);
                }
            }
            MapDraw::Costmap => {
                if let Some(picture) = &self.costmap {
                    draw_picture(&mut self.draw_costmap, cx, &view, rect, picture);
                }
            }
            MapDraw::Grid => {
                self.draw_rule.color = rgba(GRID_COLOR);
                let half = GRID_LINE_PX / 2.0;
                for (a, b) in grid_lines(&view, GRID_M) {
                    // Vertical lines first (same x at both ends), then horizontal.
                    let (pos, size) = if a[0] == b[0] {
                        (dvec2(a[0] - half, a[1]), dvec2(GRID_LINE_PX, b[1] - a[1]))
                    } else {
                        (dvec2(a[0], a[1] - half), dvec2(b[0] - a[0], GRID_LINE_PX))
                    };
                    let pos = rect.pos + pos;
                    self.draw_rule.draw_abs(cx, Rect { pos, size });
                }
            }
            MapDraw::Plan => {
                let pen = Pen {
                    width: PLAN_PX,
                    color: layer_color(scene, nav_entities::NAV_PLAN),
                };
                let line = &mut self.draw_line;
                polyline(line, cx, &view, rect, &scene.plan, false, pen);
            }
            MapDraw::Trail => {
                let pen = Pen {
                    width: TRAIL_PX,
                    color: layer_color(scene, nav_entities::NAV_TRAIL),
                };
                let line = &mut self.draw_line;
                polyline(line, cx, &view, rect, &scene.trail, false, pen);
            }
            MapDraw::Scan => {
                self.draw_dot.color = rgba(layer_color(scene, nav_entities::SENSORS_SCAN));
                let half = dvec2(DOT_PX / 2.0, DOT_PX / 2.0);
                for p in &scene.scan {
                    let at = on_screen(&view, rect, [f64::from(p[0]), f64::from(p[1])]);
                    if meets(rect, at, at, DOT_PX) {
                        let pos = at - half;
                        let size = dvec2(DOT_PX, DOT_PX);
                        self.draw_dot.draw_abs(cx, Rect { pos, size });
                    }
                }
            }
            MapDraw::FootprintFill => {
                if let Some(points) = fill_corners(&scene.footprint) {
                    let corners: Vec<Vec2d> = points
                        .iter()
                        .map(|p| on_screen(&view, rect, [f64::from(p[0]), f64::from(p[1])]))
                        .collect();
                    self.draw_fill.color = rgba(FOOTPRINT_FILL);
                    self.draw_fill.polygon(cx, rect, &corners);
                }
            }
            MapDraw::FootprintEdge => {
                let normal = layer_color(scene, nav_entities::ROBOT_FOOTPRINT);
                let pen = Pen {
                    width: FOOTPRINT_PX,
                    color: footprint_color(scene.status.as_ref(), normal),
                };
                let points = &scene.footprint;
                polyline(&mut self.draw_line, cx, &view, rect, points, true, pen);
            }
            MapDraw::PoseArrow => {
                if let Some(pose) = &scene.pose {
                    let (from, to) = pose_arrow(pose);
                    let from = on_screen(&view, rect, from);
                    let arrow = arrow_strokes(from, on_screen(&view, rect, to) - from);
                    let pen = Pen {
                        width: MARK_PX,
                        color: POSE_ARROW_COLOR,
                    };
                    strokes(&mut self.draw_line, cx, rect, &arrow, pen);
                }
            }
            MapDraw::Goal => {
                if let Some((origin, vector)) = scene.goal {
                    let from = on_screen(&view, rect, origin.map(f64::from));
                    let goal = goal_strokes(from, vector, view.px_per_m);
                    let pen = Pen {
                        width: MARK_PX,
                        color: layer_color(scene, nav_entities::NAV_GOAL),
                    };
                    strokes(&mut self.draw_line, cx, rect, &goal, pen);
                }
            }
            MapDraw::Waiting => self.waiting(cx, rect),
        }
    }

    /// [`WAITING`] in the middle of `rect`, in the dark theme's ink: the
    /// canvas is dark in both themes.
    fn waiting(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_text.color = pal::INK_D;
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

    /// Draws the steps of [`draw_plan`] in order. Each step is one or more
    /// instances of one draw type; Makepad appends a draw only to the draw
    /// call just before it when that has the same shader and textures, and
    /// starts a new one, painted over the earlier ones, otherwise. So the
    /// steps stay in this order on screen: the costmap's texture is a new
    /// call over the map's, the dots one over the lines, and lines drawn
    /// later in the same call are over the earlier ones.
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
        for step in draw_plan(&self.scene, &self.layers) {
            self.draw_step(cx, rect, step);
        }
        cx.pop_clip_rect();
        DrawStep::done()
    }
}

impl DvizMap2DRef {
    /// Shows `scene`. The map and costmap textures are rebuilt only when
    /// their grid changed; a scene already shown (same update count, same
    /// grids) is not copied again.
    pub fn set_scene(&self, cx: &mut Cx, scene: &NavScene) {
        let Some(mut map) = self.borrow_mut() else {
            return;
        };
        let same = |a: &Option<GridLayer>, b: &Option<GridLayer>| match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a.generation == b.generation && Arc::ptr_eq(&a.cells, &b.cells),
            _ => false,
        };
        if same(&map.scene.grid, &scene.grid)
            && same(&map.scene.costmap, &scene.costmap)
            && map.scene.updated == scene.updated
        {
            return;
        }
        map.update_pictures(cx, scene);
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

    /// What the next frame draws, bottom to top ([`draw_plan`]).
    pub fn draw_plan(&self) -> Vec<MapDraw> {
        self.borrow()
            .map_or_else(Vec::new, |map| draw_plan(&map.scene, &map.layers))
    }

    /// Whether a grid texture is held.
    pub fn has_texture(&self) -> bool {
        self.borrow().is_some_and(|map| map.grid.is_some())
    }

    /// How many grid textures have been built.
    pub fn texture_builds(&self) -> u64 {
        self.borrow().map_or(0, |map| map.texture_builds)
    }

    /// Whether a costmap texture is held.
    pub fn has_costmap_texture(&self) -> bool {
        self.borrow().is_some_and(|map| map.costmap.is_some())
    }

    /// How many costmap textures have been built.
    pub fn costmap_texture_builds(&self) -> u64 {
        self.borrow().map_or(0, |map| map.costmap_builds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav_scene::Pose2D;
    use dviz_protocol::GridKind;

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

    #[test]
    fn goal_ring_radius_min_10px() {
        let center = [100.0, 50.0];
        // 0.15 m is 3 px at 20 px/m, so the ring is drawn 10 px; 30 px at 200.
        for (px_per_m, radius) in [(20.0, 10.0), (200.0, 30.0)] {
            let ring = goal_ring(center, px_per_m);
            assert_eq!(ring.len(), 25, "{px_per_m}");
            assert_eq!(ring.first(), ring.last(), "closed");
            for p in &ring {
                let r = (p[0] - center[0]).hypot(p[1] - center[1]);
                assert!((r - radius).abs() < 1e-9, "{px_per_m}: {p:?} at {r}");
            }
        }
    }

    /// A goal sent without a yaw has a vertical vector, flat on the map: only
    /// its ring; with a yaw the ring and an arrow along the vector.
    #[test]
    fn a_goal_is_a_ring_and_with_a_yaw_an_arrow() {
        let at = dvec2(100.0, 50.0);
        let near = |a: Vec2d, b: Vec2d| (a - b).length() < 1e-9;
        let on_ring = |p: Vec2d| ((p - at).length() - GOAL_RING_MIN_PX).abs() < 1e-9;
        // 0.15 m at 50 px/m is under 10 px: the ring is 10 px.
        let ring = goal_strokes(at, [0.0, 0.0], 50.0);
        assert_eq!(ring.len(), GOAL_RING_SEGMENTS, "{ring:?}");
        assert!(ring.iter().all(|(a, b)| on_ring(*a) && on_ring(*b)));
        // 0.5 m east at 50 px/m: the ring, then a 25 px shaft and a head.
        let east = goal_strokes(at, [0.5, 0.0], 50.0);
        assert_eq!(east.len(), GOAL_RING_SEGMENTS + 3);
        let arrow = &east[GOAL_RING_SEGMENTS..];
        assert!(
            near(arrow[0].0, at) && near(arrow[0].1, at + dvec2(25.0, 0.0)),
            "{arrow:?}"
        );
        assert!(arrow[1..]
            .iter()
            .all(|(a, _)| near(*a, at + dvec2(25.0, 0.0))));
        // North is up on screen; a short one is drawn ARROW_MIN_PX long.
        let north = goal_strokes(at, [0.0, 0.1], 50.0);
        assert!(
            near(north[GOAL_RING_SEGMENTS].1, at + dvec2(0.0, -ARROW_MIN_PX)),
            "{north:?}"
        );
        assert!(goal_strokes(at, [f32::NAN, 0.0], 50.0).is_empty());
        assert!(goal_strokes(at, [0.5, 0.0], f64::NAN).is_empty());
    }

    #[test]
    fn pose_arrow_points_forward() {
        let (from, to) = pose_arrow(&Pose2D {
            x: 1.5,
            y: -2.0,
            yaw: std::f64::consts::FRAC_PI_2,
        });
        assert_eq!(from, [1.5, -2.0]);
        assert!(
            (to[0] - 1.5).abs() < 1e-12 && (to[1] - (-2.0 + 0.4)).abs() < 1e-12,
            "{to:?}"
        );
        let (_, ahead) = pose_arrow(&Pose2D {
            x: 0.0,
            y: 0.0,
            yaw: 0.0,
        });
        assert_eq!(ahead, [0.4, 0.0]);
    }

    #[test]
    fn footprint_red_when_untrusted_or_stale() {
        let normal = [220, 220, 220, 255];
        let red = [255, 0, 0, 255];
        let trusted = NavStatus {
            loc_ok: true,
            ..NavStatus::default()
        };
        assert_eq!(footprint_color(Some(&trusted), normal), normal);
        let untrusted = NavStatus {
            loc_ok: false,
            ..trusted.clone()
        };
        assert_eq!(footprint_color(Some(&untrusted), normal), red);
        let stale = NavStatus {
            stale: vec!["localization".into()],
            ..trusted.clone()
        };
        assert_eq!(footprint_color(Some(&stale), normal), red);
        // Another part gone stale says nothing about localization.
        let gate = NavStatus {
            stale: vec!["gate".into(), "speed".into()],
            ..trusted.clone()
        };
        assert_eq!(footprint_color(Some(&gate), normal), normal);
        // No status yet: nothing is known either way.
        assert_eq!(footprint_color(None, normal), normal);
        // A colour the bridge sent is kept while localization is trusted.
        assert_eq!(footprint_color(Some(&trusted), [1, 2, 3, 4]), [1, 2, 3, 4]);
    }

    /// Every layer present, as the car sends them.
    fn full_scene() -> NavScene {
        let layer = |kind| GridLayer {
            header: Grid2DHeader {
                width: 4,
                height: 2,
                resolution: 0.5,
                origin: [0.0; 3],
                kind,
            },
            cells: Arc::new(vec![0; 8]),
            generation: 1,
        };
        NavScene {
            grid: Some(layer(GridKind::Occupancy)),
            costmap: Some(layer(GridKind::Costmap)),
            pose: Some(Pose2D {
                x: 1.0,
                y: 0.5,
                yaw: 0.0,
            }),
            footprint: vec![[0.8, 0.3], [1.2, 0.3], [1.2, 0.7], [0.8, 0.7], [0.8, 0.3]],
            plan: vec![[1.0, 0.5], [2.0, 0.5]],
            trail: vec![[0.0, 0.5], [1.0, 0.5]],
            goal: Some(([2.0, 0.5], [0.5, 0.0])),
            scan: vec![[1.5, 1.0]],
            ..NavScene::default()
        }
    }

    #[test]
    fn costmap_layer_off_skips_it() {
        let scene = full_scene();
        let on = draw_plan(&scene, &NavLayers::default());
        assert!(on.contains(&MapDraw::Costmap), "{on:?}");
        let off = draw_plan(
            &scene,
            &NavLayers {
                costmap: false,
                ..NavLayers::default()
            },
        );
        assert!(!off.contains(&MapDraw::Costmap), "{off:?}");
        // Everything else is drawn as before.
        let rest: Vec<MapDraw> = on.into_iter().filter(|d| *d != MapDraw::Costmap).collect();
        assert_eq!(off, rest);
    }

    #[test]
    fn layers_are_drawn_bottom_to_top() {
        use MapDraw::*;
        let scene = full_scene();
        assert_eq!(
            draw_plan(&scene, &NavLayers::default()),
            [
                Canvas,
                Map,
                Costmap,
                Grid,
                Plan,
                Trail,
                Scan,
                FootprintFill,
                FootprintEdge,
                PoseArrow,
                Goal
            ]
        );
        // No costmap (a planner without one): the rest as before.
        let older = NavScene {
            costmap: None,
            ..scene.clone()
        };
        assert_eq!(
            draw_plan(&older, &NavLayers::default()),
            [
                Canvas,
                Map,
                Grid,
                Plan,
                Trail,
                Scan,
                FootprintFill,
                FootprintEdge,
                PoseArrow,
                Goal
            ]
        );
        // Every legend switch off: the canvas and its grid.
        let none = NavLayers {
            map: false,
            footprint: false,
            plan: false,
            trail: false,
            goal: false,
            scan: false,
            costmap: false,
        };
        assert_eq!(draw_plan(&scene, &none), [Canvas, Grid]);
        // Nothing received yet: the grid and the waiting line.
        assert_eq!(
            draw_plan(&NavScene::default(), &NavLayers::default()),
            [Canvas, Grid, Waiting]
        );
        // A footprint too short to fill is still outlined.
        let short = NavScene {
            footprint: vec![[0.0, 0.0], [1.0, 0.0]],
            ..scene
        };
        let plan = draw_plan(&short, &NavLayers::default());
        assert!(!plan.contains(&FootprintFill) && plan.contains(&FootprintEdge));
    }

    /// The fill takes the footprint's corners: a closing point is not a
    /// corner; under three is no area, over FILL_CORNERS is left unfilled.
    #[test]
    fn footprint_fill_takes_three_to_eight_corners() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let closed = [square.as_slice(), &square[..1]].concat();
        assert_eq!(fill_corners(&closed), Some(&square[..]));
        assert_eq!(fill_corners(&square), Some(&square[..]));
        assert_eq!(fill_corners(&square[..2]), None);
        let octagon: Vec<[f32; 2]> = (0..8)
            .map(|i| {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                [a.cos(), a.sin()]
            })
            .collect();
        assert_eq!(fill_corners(&octagon).map(<[_]>::len), Some(8));
        let nonagon: Vec<[f32; 2]> = (0..9).map(|i| [i as f32, (i % 2) as f32]).collect();
        assert_eq!(fill_corners(&nonagon), None);
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
                ("map glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_map, "glsl", false)})),
                ("map metal", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_map, "metal", false)})),
                ("costmap glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_costmap, "glsl", false)})),
                ("fill glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_fill, "glsl", false)})),
                ("fill metal", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_fill, "metal", false)})),
                ("rule glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizMap2D.draw_rule, "glsl", false)})),
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
