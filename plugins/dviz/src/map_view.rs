//! The 2D map view's pure parts: map metres to screen pixels and back, zoom
//! and pan, what to show (`scene_bounds`), which layers are on, each layer's
//! colour, and the occupancy grid as a picture. `grid_rgba` is shared with
//! the Rerun sink. Compiled in every build.

use crate::nav_scene::NavScene;
use dviz_protocol::{nav_entities, Grid2DHeader};

/// Zoom limits, in screen pixels per map metre.
pub const MIN_PX_PER_M: f64 = 2.0;
pub const MAX_PX_PER_M: f64 = 2000.0;
/// Room left on each side of what `fit` shows, as a share of its size.
const FIT_MARGIN: f64 = 0.05;
/// Without a map, the smallest width and height `scene_bounds` gives, in metres.
const MIN_SPAN_M: f64 = 4.0;

/// What part of the map the view shows: the map point at the middle of the
/// view, the scale, and the view's size in pixels. Screen y runs down, map
/// y up; screen (0, 0) is the view's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapView {
    pub center: [f64; 2],
    pub px_per_m: f64,
    pub size: [f64; 2],
}

/// The origin at 50 px/m, in a 640 x 480 view: the size the view counts with
/// until it is first drawn.
impl Default for MapView {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            px_per_m: 50.0,
            size: [640.0, 480.0],
        }
    }
}

impl MapView {
    /// Where the map point `p` (metres) is on screen (pixels).
    pub fn world_to_screen(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.size[0] / 2.0 + (p[0] - self.center[0]) * self.px_per_m,
            self.size[1] / 2.0 - (p[1] - self.center[1]) * self.px_per_m,
        ]
    }

    /// The map point (metres) under the screen point `s` (pixels).
    pub fn screen_to_world(&self, s: [f64; 2]) -> [f64; 2] {
        [
            self.center[0] + (s[0] - self.size[0] / 2.0) / self.px_per_m,
            self.center[1] - (s[1] - self.size[1] / 2.0) / self.px_per_m,
        ]
    }

    /// Scales by `factor` about the screen point `s`, which keeps the map
    /// point under it. The scale stays within [`MIN_PX_PER_M`,
    /// `MAX_PX_PER_M`]; a factor that is not a positive number does nothing.
    pub fn zoom_at(&mut self, s: [f64; 2], factor: f64) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let anchor = self.screen_to_world(s);
        self.px_per_m = (self.px_per_m * factor).clamp(MIN_PX_PER_M, MAX_PX_PER_M);
        self.center = [
            anchor[0] - (s[0] - self.size[0] / 2.0) / self.px_per_m,
            anchor[1] + (s[1] - self.size[1] / 2.0) / self.px_per_m,
        ];
    }

    /// Moves the map with a pointer that moved by `ds` pixels.
    pub fn pan_by(&mut self, ds: [f64; 2]) {
        self.center[0] -= ds[0] / self.px_per_m;
        self.center[1] += ds[1] / self.px_per_m;
    }

    /// Shows the box from `min` to `max` (metres) whole and centred, with 5 %
    /// of its size to spare on every side, within the zoom limits.
    pub fn fit(&mut self, min: [f64; 2], max: [f64; 2]) {
        if !min.iter().chain(&max).all(|v| v.is_finite()) {
            return;
        }
        let margin = 1.0 + 2.0 * FIT_MARGIN;
        let scale = |room: f64, span: f64| {
            if span > 0.0 {
                room / (span * margin)
            } else {
                f64::INFINITY
            }
        };
        let px_per_m = scale(self.size[0], (max[0] - min[0]).abs())
            .min(scale(self.size[1], (max[1] - min[1]).abs()));
        if px_per_m.is_finite() && px_per_m > 0.0 {
            self.px_per_m = px_per_m.clamp(MIN_PX_PER_M, MAX_PX_PER_M);
        }
        self.center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    }
}

/// 取景范围：有地图用地图边界；没有地图用车、轨迹、规划的包围盒，至少 4 m × 4 m。
/// 什么都没有时返回 None。
///
/// The robot is its pose and its footprint. The map's own rotation is not
/// drawn (the bridge does not send rotated maps), so neither is it counted.
pub fn scene_bounds(scene: &NavScene) -> Option<([f64; 2], [f64; 2])> {
    if let Some(grid) = &scene.grid {
        let header = &grid.header;
        let resolution = f64::from(header.resolution);
        let min = [f64::from(header.origin[0]), f64::from(header.origin[1])];
        let max = [
            min[0] + f64::from(header.width) * resolution,
            min[1] + f64::from(header.height) * resolution,
        ];
        return Some((min, max));
    }
    let lines = scene
        .footprint
        .iter()
        .chain(&scene.trail)
        .chain(&scene.plan)
        .map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let (mut min, mut max) = scene
        .pose
        .iter()
        .map(|pose| [pose.x, pose.y])
        .chain(lines)
        .fold(None, |bounds: Option<([f64; 2], [f64; 2])>, p| {
            Some(match bounds {
                None => (p, p),
                Some((min, max)) => (
                    [min[0].min(p[0]), min[1].min(p[1])],
                    [max[0].max(p[0]), max[1].max(p[1])],
                ),
            })
        })?;
    for axis in 0..2 {
        if max[axis] - min[axis] < MIN_SPAN_M {
            let middle = (min[axis] + max[axis]) / 2.0;
            min[axis] = middle - MIN_SPAN_M / 2.0;
            max[axis] = middle + MIN_SPAN_M / 2.0;
        }
    }
    Some((min, max))
}

/// Which layers the 2D view draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavLayers {
    pub map: bool,
    pub footprint: bool,
    pub plan: bool,
    pub trail: bool,
    pub goal: bool,
    pub scan: bool,
    pub costmap: bool,
}

impl Default for NavLayers {
    fn default() -> Self {
        Self {
            map: true,
            footprint: true,
            plan: true,
            trail: true,
            goal: true,
            scan: true,
            costmap: true,
        }
    }
}

/// The colour of a navigation entity (`nav_entities` name): the one the
/// bridge last sent with it, else the bridge's own default (RViz look). The
/// map's default stands for its unknown cells, which is what its legend
/// swatch shows.
pub fn layer_color(scene: &NavScene, entity: &str) -> [u8; 4] {
    if let Some(color) = scene.colors.get(entity) {
        return *color;
    }
    match entity {
        nav_entities::MAP_GRID => [0x70, 0x89, 0x86, 255],
        nav_entities::MAP_CLOUD => [180, 180, 180, 255],
        nav_entities::ROBOT_FOOTPRINT => [220, 220, 220, 255],
        nav_entities::NAV_PLAN => [0, 180, 255, 255],
        nav_entities::NAV_TRAIL => [0, 220, 0, 255],
        nav_entities::NAV_GOAL => [255, 170, 0, 255],
        nav_entities::SENSORS_SCAN => [255, 0, 0, 255],
        _ => [255, 255, 255, 255],
    }
}

/// RViz's colour for a cell with no data.
const RVIZ_UNKNOWN: [u8; 3] = [0x70, 0x89, 0x86];
/// RViz map layer opacity 0.7 and costmap layer opacity 0.5, as alpha bytes.
const MAP_ALPHA: u8 = 179;
const COSTMAP_ALPHA: u8 = 128;

/// A Nav2 cost value (0 free, 1..=252 inflated, 253 inscribed, 254 lethal,
/// 255 unknown) as RViz's 0..=100 scale; `None` is unknown.
pub fn nav2_to_rviz(cost: u8) -> Option<u8> {
    match cost {
        0 => Some(0),
        253 => Some(99),
        254 => Some(100),
        255 => None,
        c => Some((1 + (u32::from(c) - 1) * 97 / 251) as u8),
    }
}

/// RViz "map" scheme for a `grid2d` occupancy value (0..=100; anything else
/// is unknown): white to black, at map opacity 0.7.
pub fn rviz_map_color(occ: u8) -> [u8; 4] {
    if occ > 100 {
        let [r, g, b] = RVIZ_UNKNOWN;
        return [r, g, b, MAP_ALPHA];
    }
    let v = (255 - 255 * u32::from(occ) / 100) as u8;
    [v, v, v, MAP_ALPHA]
}

/// RViz "costmap" scheme for an RViz value: 0 transparent, 1..=98 blue to
/// red, 99 cyan, 100 magenta, `None` unknown; opacity 0.5.
pub fn rviz_costmap_color(v: Option<u8>) -> [u8; 4] {
    match v {
        None => {
            let [r, g, b] = RVIZ_UNKNOWN;
            [r, g, b, COSTMAP_ALPHA]
        }
        Some(0) => [0, 0, 0, 0],
        Some(99) => [0, 255, 255, COSTMAP_ALPHA],
        Some(i) if i >= 100 => [255, 0, 255, COSTMAP_ALPHA],
        Some(i) => {
            let v = (255 * u32::from(i) / 100) as u8;
            [v, 0, 255 - v, COSTMAP_ALPHA]
        }
    }
}

/// The grid as RGBA, one colour per cell. Output row 0 is the largest map y
/// (a picture runs top to bottom; input row 0 is the smallest y).
fn grid_pixels(header: &Grid2DHeader, cells: &[u8], color: impl Fn(u8) -> [u8; 4]) -> Vec<u8> {
    let width = header.width as usize;
    let height = header.height as usize;
    let mut out = Vec::with_capacity(cells.len().saturating_mul(4));
    if width == 0 {
        return out;
    }
    for row in cells.chunks(width).take(height).rev() {
        for &cell in row {
            out.extend_from_slice(&color(cell));
        }
    }
    out
}

/// Occupancy grid in RViz's map palette (see `rviz_map_color`).
pub fn grid_rgba(header: &Grid2DHeader, cells: &[u8]) -> Vec<u8> {
    grid_pixels(header, cells, rviz_map_color)
}

/// Nav2 costmap in RViz's costmap palette (see `nav2_to_rviz`).
pub fn costmap_rgba(header: &Grid2DHeader, cells: &[u8]) -> Vec<u8> {
    grid_pixels(header, cells, |c| rviz_costmap_color(nav2_to_rviz(c)))
}

/// Grid lines in screen coordinates: the multiples of `spacing_m` (map
/// metres) inside the view, as vertical then horizontal segments across it.
/// Empty for a non-positive spacing or one under a pixel.
pub fn grid_lines(view: &MapView, spacing_m: f64) -> Vec<([f64; 2], [f64; 2])> {
    if !(spacing_m.is_finite() && spacing_m > 0.0) || spacing_m * view.px_per_m < 1.0 {
        return Vec::new();
    }
    let [w, h] = view.size;
    let top_left = view.screen_to_world([0.0, 0.0]);
    let bottom_right = view.screen_to_world([w, h]);
    // A hair of slack so a line exactly on the edge stays in.
    let eps = 1e-9;
    let first = |lo: f64| ((lo - eps) / spacing_m).ceil() as i64;
    let last = |hi: f64| ((hi + eps) / spacing_m).floor() as i64;
    let mut lines = Vec::new();
    for k in first(top_left[0])..=last(bottom_right[0]) {
        let x = view.world_to_screen([k as f64 * spacing_m, 0.0])[0];
        lines.push(([x, 0.0], [x, h]));
    }
    for k in first(bottom_right[1])..=last(top_left[1]) {
        let y = view.world_to_screen([0.0, k as f64 * spacing_m])[1];
        lines.push(([0.0, y], [w, y]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav_scene::{GridLayer, Pose2D};
    use dviz_protocol::nav_entities::{
        NAV_GOAL, NAV_PLAN, NAV_TRAIL, ROBOT_FOOTPRINT, SENSORS_SCAN,
    };
    use dviz_protocol::GridKind;

    #[test]
    fn grid_rgba_colors_and_flip() {
        let header = Grid2DHeader {
            width: 2,
            height: 2,
            resolution: 0.05,
            origin: [0.0; 3],
            kind: GridKind::Occupancy,
        };
        let rgba = grid_rgba(&header, &[0, 100, 255, 0]);
        assert_eq!(rgba.len(), 16);
        // First pixel: row 1 (largest y), column 0 = unknown.
        assert_eq!(rgba[0..4], [0x70, 0x89, 0x86, 179]);
        assert_eq!(rgba[4..8], [255, 255, 255, 179]); // row 1, column 1 = free
        assert_eq!(rgba[8..12], [255, 255, 255, 179]); // row 0, column 0 = free
        assert_eq!(rgba[12..16], [0, 0, 0, 179]); // row 0, column 1 = occupied
    }

    #[test]
    fn nav2_to_rviz_table() {
        assert_eq!(nav2_to_rviz(0), Some(0));
        assert_eq!(nav2_to_rviz(1), Some(1));
        assert_eq!(nav2_to_rviz(252), Some(98));
        assert_eq!(nav2_to_rviz(253), Some(99));
        assert_eq!(nav2_to_rviz(254), Some(100));
        assert_eq!(nav2_to_rviz(255), None);
    }

    #[test]
    fn rviz_map_palette() {
        assert_eq!(rviz_map_color(0), [255, 255, 255, 179]);
        assert_eq!(rviz_map_color(100), [0, 0, 0, 179]);
        assert_eq!(rviz_map_color(50), [128, 128, 128, 179]);
        assert_eq!(rviz_map_color(255), [0x70, 0x89, 0x86, 179]);
    }

    #[test]
    fn rviz_costmap_palette() {
        assert_eq!(rviz_costmap_color(Some(0))[3], 0);
        assert_eq!(rviz_costmap_color(Some(1)), [2, 0, 253, 128]);
        assert_eq!(rviz_costmap_color(Some(98)), [249, 0, 6, 128]);
        assert_eq!(rviz_costmap_color(Some(99)), [0, 255, 255, 128]);
        assert_eq!(rviz_costmap_color(Some(100)), [255, 0, 255, 128]);
        assert_eq!(rviz_costmap_color(None), [0x70, 0x89, 0x86, 128]);
    }

    #[test]
    fn costmap_rgba_converts_and_flips() {
        let header = Grid2DHeader {
            width: 2,
            height: 2,
            resolution: 0.05,
            origin: [0.0; 3],
            kind: GridKind::Costmap,
        };
        let rgba = costmap_rgba(&header, &[0, 254, 255, 253]);
        assert_eq!(rgba.len(), 16);
        assert_eq!(rgba[0..4], [0x70, 0x89, 0x86, 128]); // row 1, column 0
        assert_eq!(rgba[4..8], [0, 255, 255, 128]);
        assert_eq!(rgba[8..12][3], 0); // row 0, column 0 = free, transparent
        assert_eq!(rgba[12..16], [255, 0, 255, 128]);
    }

    #[test]
    fn grid_lines_follow_zoom() {
        let mut view = MapView {
            center: [0.0, 0.0],
            px_per_m: 100.0,
            size: [800.0, 600.0],
        };
        let vertical =
            |lines: &[([f64; 2], [f64; 2])]| lines.iter().filter(|l| l.0[0] == l.1[0]).count();
        let lines = grid_lines(&view, 1.0);
        assert_eq!(vertical(&lines), 9);
        let first = lines.iter().find(|l| l.0[0] == l.1[0]).unwrap();
        assert!((first.0[0]).abs() < 1e-9);
        view.zoom_at([400.0, 300.0], 2.0);
        assert_eq!(vertical(&grid_lines(&view, 1.0)), 5);
        assert!(grid_lines(&view, 0.0).is_empty());
    }

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn world_screen_round_trip() {
        let view = MapView {
            center: [0.0, 0.0],
            px_per_m: 100.0,
            size: [800.0, 600.0],
        };
        let s = view.world_to_screen([1.0, 1.0]);
        assert!(close(s, [500.0, 200.0]), "{s:?}");
        let back = view.screen_to_world(s);
        assert!(close(back, [1.0, 1.0]), "{back:?}");
        // Off-centre and fractional, both ways.
        let view = MapView {
            center: [-3.25, 7.5],
            px_per_m: 37.0,
            size: [640.0, 480.0],
        };
        for p in [[0.0, 0.0], [-10.5, 3.3], [12.0, -8.0]] {
            assert!(
                close(view.screen_to_world(view.world_to_screen(p)), p),
                "{p:?}"
            );
        }
    }

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut view = MapView {
            center: [1.0, -2.0],
            px_per_m: 50.0,
            size: [800.0, 600.0],
        };
        let cursor = [600.0, 100.0];
        let before = view.screen_to_world(cursor);
        view.zoom_at(cursor, 2.0);
        assert_eq!(view.px_per_m, 100.0);
        let after = view.screen_to_world(cursor);
        assert!(close(before, after), "{before:?} {after:?}");
        assert!(close(view.world_to_screen(before), cursor));
    }

    #[test]
    fn zoom_is_clamped() {
        let mut view = MapView {
            center: [0.0, 0.0],
            px_per_m: 100.0,
            size: [800.0, 600.0],
        };
        view.zoom_at([400.0, 300.0], 1e6);
        assert_eq!(view.px_per_m, 2000.0);
        view.zoom_at([400.0, 300.0], 1e-9);
        assert_eq!(view.px_per_m, 2.0);
        // A factor that is not a positive number changes nothing.
        for factor in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            let before = view;
            view.zoom_at([10.0, 10.0], factor);
            assert_eq!(view, before, "{factor}");
        }
    }

    #[test]
    fn pan_moves_the_world_with_the_pointer() {
        let mut view = MapView {
            center: [0.0, 0.0],
            px_per_m: 100.0,
            size: [800.0, 600.0],
        };
        let grabbed = view.screen_to_world([300.0, 300.0]);
        view.pan_by([50.0, -20.0]);
        assert!(close(view.world_to_screen(grabbed), [350.0, 280.0]));
    }

    #[test]
    fn fit_with_margin() {
        let mut view = MapView {
            center: [-40.0, 9.0],
            px_per_m: 3.0,
            size: [800.0, 600.0],
        };
        view.fit([0.0, 0.0], [10.0, 5.0]);
        assert!(
            (view.px_per_m - 800.0 / (10.0 * 1.1)).abs() < 1e-9,
            "{view:?}"
        );
        assert!(close(view.center, [5.0, 2.5]), "{view:?}");
        // Tall bounds: the height decides.
        view.fit([0.0, 0.0], [1.0, 20.0]);
        assert!(
            (view.px_per_m - 600.0 / (20.0 * 1.1)).abs() < 1e-9,
            "{view:?}"
        );
    }

    fn scene_with_pose(x: f64, y: f64) -> NavScene {
        NavScene {
            pose: Some(Pose2D { x, y, yaw: 0.0 }),
            ..NavScene::default()
        }
    }

    fn contains(bounds: ([f64; 2], [f64; 2]), p: [f64; 2]) -> bool {
        let (min, max) = bounds;
        (min[0]..=max[0]).contains(&p[0]) && (min[1]..=max[1]).contains(&p[1])
    }

    #[test]
    fn fit_without_grid_uses_robot_and_trail() {
        assert_eq!(scene_bounds(&NavScene::default()), None);
        let mut scene = scene_with_pose(2.0, 3.0);
        scene.trail = vec![[2.0, 3.0], [3.0, 3.0], [4.0, 3.0]];
        let bounds = scene_bounds(&scene).expect("robot and trail give bounds");
        let (min, max) = bounds;
        assert!(
            max[0] - min[0] >= 4.0 && max[1] - min[1] >= 4.0,
            "{bounds:?}"
        );
        assert!(
            contains(bounds, [2.0, 3.0]) && contains(bounds, [4.0, 3.0]),
            "{bounds:?}"
        );
        // The plan counts too; a box larger than 4 m keeps its own size.
        scene.plan = vec![[4.0, 3.0], [12.0, -5.0]];
        let bounds = scene_bounds(&scene).unwrap();
        assert!(
            contains(bounds, [12.0, -5.0]) && contains(bounds, [2.0, 3.0]),
            "{bounds:?}"
        );
        assert!(
            (bounds.1[0] - bounds.0[0] - 10.0).abs() < 1e-6,
            "{bounds:?}"
        );
        // Then the view fitted to them shows both points.
        let mut view = MapView {
            center: [0.0, 0.0],
            px_per_m: 100.0,
            size: [800.0, 600.0],
        };
        view.fit(bounds.0, bounds.1);
        for p in [[2.0, 3.0], [12.0, -5.0]] {
            let s = view.world_to_screen(p);
            assert!(
                (0.0..=800.0).contains(&s[0]) && (0.0..=600.0).contains(&s[1]),
                "{p:?} at {s:?}"
            );
        }
    }

    #[test]
    fn bounds_of_a_grid_are_the_map() {
        let header = Grid2DHeader {
            width: 200,
            height: 100,
            resolution: 0.05,
            origin: [-5.0, -2.0, 0.0],
            kind: GridKind::Occupancy,
        };
        let mut scene = scene_with_pose(40.0, 40.0);
        scene.grid = Some(GridLayer {
            header,
            cells: std::sync::Arc::new(vec![0; 200 * 100]),
            generation: 1,
        });
        let (min, max) = scene_bounds(&scene).unwrap();
        assert!(close(min, [-5.0, -2.0]), "{min:?}");
        assert!(
            (max[0] - 5.0).abs() < 1e-5 && (max[1] - 3.0).abs() < 1e-5,
            "{max:?}"
        );
    }

    #[test]
    fn scene_without_costmap_draws_nothing_for_it() {
        let mut scene = scene_with_pose(1.0, 1.0);
        assert!(scene.costmap.is_none());
        let before = scene_bounds(&scene);
        scene.costmap = Some(GridLayer {
            header: Grid2DHeader {
                width: 400,
                height: 400,
                resolution: 0.05,
                origin: [-50.0, -50.0, 0.0],
                kind: GridKind::Costmap,
            },
            cells: std::sync::Arc::new(vec![0; 400 * 400]),
            generation: 1,
        });
        // The costmap never changes what the view fits to.
        assert_eq!(scene_bounds(&scene), before);
    }

    #[test]
    fn every_layer_is_shown_by_default() {
        let layers = NavLayers::default();
        assert!(
            layers.map
                && layers.footprint
                && layers.plan
                && layers.trail
                && layers.goal
                && layers.scan
                && layers.costmap
        );
    }

    #[test]
    fn layer_colors_default_until_the_bridge_sends_one() {
        // The bridge's own defaults (RViz look), so a scene without colours
        // looks the same as one with them.
        let mut scene = NavScene::default();
        assert_eq!(layer_color(&scene, NAV_PLAN), [0, 180, 255, 255]);
        assert_eq!(layer_color(&scene, NAV_TRAIL), [0, 220, 0, 255]);
        assert_eq!(layer_color(&scene, ROBOT_FOOTPRINT), [220, 220, 220, 255]);
        assert_eq!(layer_color(&scene, NAV_GOAL), [255, 170, 0, 255]);
        assert_eq!(layer_color(&scene, SENSORS_SCAN), [255, 0, 0, 255]);
        scene.colors.insert(NAV_PLAN, [1, 2, 3, 4]);
        assert_eq!(layer_color(&scene, NAV_PLAN), [1, 2, 3, 4]);
    }
}
