//! The 2D map view's pure parts: map metres to screen pixels and back, zoom
//! and pan, what to show (`scene_bounds`), which layers are on, each layer's
//! colour, and the occupancy grid as a picture. `grid_rgba` is shared with
//! the Rerun sink. Compiled in every build.

use crate::nav_scene::NavScene;
use dviz_protocol::{nav_entities, Grid2DHeader, GridKind, GRID_FREE, GRID_OCCUPIED};

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
        }
    }
}

/// The colour of a navigation entity (`nav_entities` name): the one the
/// bridge last sent with it, else the default. The map's default stands for
/// its unknown cells, which is what its legend swatch shows.
pub fn layer_color(scene: &NavScene, entity: &str) -> [u8; 4] {
    if let Some(color) = scene.colors.get(entity) {
        return *color;
    }
    match entity {
        nav_entities::MAP_GRID => UNKNOWN_RGBA,
        nav_entities::MAP_CLOUD => [180, 180, 180, 255],
        nav_entities::ROBOT_FOOTPRINT => [0, 200, 0, 255],
        nav_entities::NAV_PLAN => [0, 255, 0, 255],
        nav_entities::NAV_TRAIL => [255, 140, 0, 255],
        nav_entities::NAV_GOAL => [255, 0, 255, 255],
        nav_entities::SENSORS_SCAN => [255, 0, 0, 255],
        _ => [255, 255, 255, 255],
    }
}

const FREE_RGBA: [u8; 4] = [254, 254, 254, 255];
const OCCUPIED_RGBA: [u8; 4] = [0, 0, 0, 255];
const UNKNOWN_RGBA: [u8; 4] = [205, 205, 205, 255];

/// 占用栅格转 RGBA：空闲 [254,254,254,255]、占用 [0,0,0,255]、未知 [205,205,205,255]；
/// costmap 先按未知灰处理（交付 ③ 再配色）。输出第 0 行是地图 y 最大的一行（贴图从上往下）。
pub fn grid_rgba(header: &Grid2DHeader, cells: &[u8]) -> Vec<u8> {
    let width = header.width as usize;
    let height = header.height as usize;
    let mut out = Vec::with_capacity(cells.len().saturating_mul(4));
    if width == 0 {
        return out;
    }
    // Input row 0 has the smallest y; the picture starts at the largest y.
    for row in cells.chunks(width).take(height).rev() {
        for &cell in row {
            let color = match header.kind {
                GridKind::Occupancy => match cell {
                    GRID_FREE => FREE_RGBA,
                    GRID_OCCUPIED => OCCUPIED_RGBA,
                    _ => UNKNOWN_RGBA,
                },
                GridKind::Costmap => UNKNOWN_RGBA,
            };
            out.extend_from_slice(&color);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav_scene::{GridLayer, Pose2D};
    use dviz_protocol::nav_entities::{
        NAV_GOAL, NAV_PLAN, NAV_TRAIL, ROBOT_FOOTPRINT, SENSORS_SCAN,
    };

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
        assert_eq!(rgba[0..4], [205, 205, 205, 255]);
        assert_eq!(rgba[4..8], [254, 254, 254, 255]); // row 1, column 1 = free
        assert_eq!(rgba[8..12], [254, 254, 254, 255]); // row 0, column 0 = free
        assert_eq!(rgba[12..16], [0, 0, 0, 255]); // row 0, column 1 = occupied
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
    fn every_layer_is_shown_by_default() {
        let layers = NavLayers::default();
        assert!(
            layers.map
                && layers.footprint
                && layers.plan
                && layers.trail
                && layers.goal
                && layers.scan
        );
    }

    #[test]
    fn layer_colors_default_until_the_bridge_sends_one() {
        let mut scene = NavScene::default();
        assert_eq!(layer_color(&scene, NAV_PLAN), [0, 255, 0, 255]);
        assert_eq!(layer_color(&scene, NAV_TRAIL), [255, 140, 0, 255]);
        assert_eq!(layer_color(&scene, ROBOT_FOOTPRINT), [0, 200, 0, 255]);
        assert_eq!(layer_color(&scene, NAV_GOAL), [255, 0, 255, 255]);
        assert_eq!(layer_color(&scene, SENSORS_SCAN), [255, 0, 0, 255]);
        scene.colors.insert(NAV_PLAN, [1, 2, 3, 4]);
        assert_eq!(layer_color(&scene, NAV_PLAN), [1, 2, 3, 4]);
    }
}
