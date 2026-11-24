//! The latest navigation layers of a session, kept apart from the sink so the
//! panels can draw the map without the 3D viewer.

use dviz_protocol::{
    binary_formats, nav_entities, parse_grid2d, parse_points_xyz_f32, Arrows3DData, Grid2DHeader,
    LineStrips3DData, MvizMessage, NavStatus, Points3DData, Transform3DData,
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavLayer {
    Grid,
    Costmap,
    MapCloud,
    Pose,
    Footprint,
    Plan,
    Trail,
    Goal,
    Scan,
    Cloud,
    Status,
}

impl NavLayer {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            nav_entities::MAP_GRID => Self::Grid,
            nav_entities::COSTMAP_GLOBAL => Self::Costmap,
            nav_entities::MAP_CLOUD => Self::MapCloud,
            nav_entities::ROBOT_POSE => Self::Pose,
            nav_entities::ROBOT_FOOTPRINT => Self::Footprint,
            nav_entities::NAV_PLAN => Self::Plan,
            nav_entities::NAV_TRAIL => Self::Trail,
            nav_entities::NAV_GOAL => Self::Goal,
            nav_entities::SENSORS_SCAN => Self::Scan,
            nav_entities::SENSORS_CLOUD => Self::Cloud,
            nav_entities::NAV_STATUS => Self::Status,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Grid => nav_entities::MAP_GRID,
            Self::Costmap => nav_entities::COSTMAP_GLOBAL,
            Self::MapCloud => nav_entities::MAP_CLOUD,
            Self::Pose => nav_entities::ROBOT_POSE,
            Self::Footprint => nav_entities::ROBOT_FOOTPRINT,
            Self::Plan => nav_entities::NAV_PLAN,
            Self::Trail => nav_entities::NAV_TRAIL,
            Self::Goal => nav_entities::NAV_GOAL,
            Self::Scan => nav_entities::SENSORS_SCAN,
            Self::Cloud => nav_entities::SENSORS_CLOUD,
            Self::Status => nav_entities::NAV_STATUS,
        }
    }

    /// The message type this layer carries.
    fn msg_type(self) -> &'static str {
        match self {
            Self::Grid | Self::Costmap => "grid2d",
            Self::MapCloud | Self::Scan | Self::Cloud => "points3d",
            Self::Pose => "transform3d",
            Self::Footprint | Self::Plan | Self::Trail => "linestrips3d",
            Self::Goal => "arrows3d",
            Self::Status => "status",
        }
    }
}

/// The layer `entity` is: exactly a navigation entity name, or one after
/// dropping the first segment (the robot id).
pub fn nav_layer(entity: &str) -> Option<NavLayer> {
    NavLayer::from_name(entity).or_else(|| NavLayer::from_name(entity.split_once('/')?.1))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pose2D {
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
}

#[derive(Debug, Clone)]
pub struct GridLayer {
    pub header: Grid2DHeader,
    pub cells: Arc<Vec<u8>>,
    pub generation: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NavScene {
    pub grid: Option<GridLayer>,
    pub costmap: Option<GridLayer>,
    pub pose: Option<Pose2D>,
    pub footprint: Vec<[f32; 2]>,
    pub plan: Vec<[f32; 2]>,
    pub trail: Vec<[f32; 2]>,
    pub goal: Option<([f32; 2], [f32; 2])>,
    pub scan: Vec<[f32; 2]>,
    pub status: Option<NavStatus>,
    pub status_at: Option<Instant>,
    pub colors: BTreeMap<&'static str, [u8; 4]>,
    pub updated: u64,
}

impl NavScene {
    /// Updates the scene and returns whether the message should still go to
    /// the sink: everything but `status`. A message that cannot be used
    /// leaves the scene as it was and is passed on.
    pub fn apply(&mut self, layer: NavLayer, msg: &MvizMessage, binary: Option<&[u8]>) -> bool {
        self.try_apply(layer, msg, binary)
            .unwrap_or(layer != NavLayer::Status)
    }

    /// `apply` that reports an unusable message (wrong type, non-finite or
    /// wrongly sized data) as `Err`, with the scene unchanged. Work is
    /// linear in the payload, which the session already bounds.
    pub(crate) fn try_apply(
        &mut self,
        layer: NavLayer,
        msg: &MvizMessage,
        binary: Option<&[u8]>,
    ) -> Result<bool, ()> {
        if msg.msg_type != layer.msg_type() {
            return Err(());
        }
        let color = msg
            .data
            .get("color")
            .and_then(|c| serde_json::from_value::<[u8; 4]>(c.clone()).ok());
        match layer {
            // Drawn from the viewer only; nothing to keep but the colour.
            NavLayer::MapCloud | NavLayer::Cloud => {}
            NavLayer::Grid | NavLayer::Costmap => self.set_grid(layer, &msg.data, binary)?,
            NavLayer::Pose => self.pose = Some(pose_of(&msg.data)?),
            NavLayer::Footprint => self.footprint = first_strip(&msg.data)?,
            NavLayer::Plan => self.plan = first_strip(&msg.data)?,
            NavLayer::Trail => self.trail = first_strip(&msg.data)?,
            NavLayer::Goal => self.goal = first_arrow(&msg.data)?,
            NavLayer::Scan => self.scan = points_xy(msg, binary)?,
            NavLayer::Status => {
                let status: NavStatus = serde_json::from_value(msg.data.clone()).map_err(|_| ())?;
                self.status = Some(status);
                self.status_at = Some(Instant::now());
            }
        }
        if let Some(color) = color {
            self.colors.insert(layer.name(), color);
        }
        self.updated += 1;
        Ok(layer != NavLayer::Status)
    }

    fn set_grid(&mut self, layer: NavLayer, data: &Value, binary: Option<&[u8]>) -> Result<(), ()> {
        let header: Grid2DHeader = serde_json::from_value(data.clone()).map_err(|_| ())?;
        if !header.origin.iter().all(|v| v.is_finite()) {
            return Err(());
        }
        let cells = parse_grid2d(&header, binary.ok_or(())?).ok_or(())?;
        let slot = if layer == NavLayer::Costmap {
            &mut self.costmap
        } else {
            &mut self.grid
        };
        let generation = slot.as_ref().map_or(0, |g| g.generation) + 1;
        *slot = Some(GridLayer {
            header,
            cells: Arc::new(cells.to_vec()),
            generation,
        });
        Ok(())
    }
}

fn finite(values: &[f32]) -> Result<(), ()> {
    values.iter().all(|v| v.is_finite()).then_some(()).ok_or(())
}

/// Position and yaw from a `transform3d`; a missing part counts as identity.
fn pose_of(data: &Value) -> Result<Pose2D, ()> {
    let t: Transform3DData = serde_json::from_value(data.clone()).map_err(|_| ())?;
    let [x, y, z] = t.translation.unwrap_or([0.0; 3]);
    finite(&[x, y, z])?;
    let yaw = if let Some(q) = t.rotation {
        finite(&q)?;
        let [qx, qy, qz, qw] = q.map(f64::from);
        (2.0 * (qw * qz + qx * qy)).atan2(1.0 - 2.0 * (qy * qy + qz * qz))
    } else if let Some(m) = t.rotation_matrix {
        finite(&m.concat())?;
        f64::from(m[1][0]).atan2(f64::from(m[0][0]))
    } else {
        0.0
    };
    Ok(Pose2D {
        x: f64::from(x),
        y: f64::from(y),
        yaw,
    })
}

/// The x, y of the first strip of a `linestrips3d` (empty if it has none).
fn first_strip(data: &Value) -> Result<Vec<[f32; 2]>, ()> {
    let lines: LineStrips3DData = serde_json::from_value(data.clone()).map_err(|_| ())?;
    let Some(strip) = lines.strips.first() else {
        return Ok(Vec::new());
    };
    strip
        .iter()
        .map(|p| finite(p).map(|()| [p[0], p[1]]))
        .collect()
}

/// Origin and vector (x, y) of the first arrow of an `arrows3d`.
fn first_arrow(data: &Value) -> Result<Option<([f32; 2], [f32; 2])>, ()> {
    let arrows: Arrows3DData = serde_json::from_value(data.clone()).map_err(|_| ())?;
    let Some(origin) = arrows.origins.first() else {
        return Ok(None);
    };
    let vector = arrows.vectors.first().ok_or(())?;
    finite(origin)?;
    finite(vector)?;
    Ok(Some(([origin[0], origin[1]], [vector[0], vector[1]])))
}

/// The x, y of a `points3d`, from the binary `xyz_f32` or the JSON `positions`.
fn points_xy(msg: &MvizMessage, binary: Option<&[u8]>) -> Result<Vec<[f32; 2]>, ()> {
    let points = match binary {
        Some(bytes) => {
            if msg.format.as_deref() != Some(binary_formats::POINTS_XYZ_F32) {
                return Err(());
            }
            parse_points_xyz_f32(bytes, msg.count.ok_or(())?).ok_or(())?
        }
        None => {
            let data: Points3DData = serde_json::from_value(msg.data.clone()).map_err(|_| ())?;
            data.positions.unwrap_or_default()
        }
    };
    points
        .iter()
        .map(|p| finite(p).map(|()| [p[0], p[1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::{accept, SessionState};
    use dviz_protocol::{binary_formats, serialize_message};
    use serde_json::{json, Value};

    fn msg(msg_type: &str, data: Value) -> MvizMessage {
        MvizMessage {
            msg_type: msg_type.to_string(),
            timestamp: Some(1.0),
            data,
            format: None,
            count: None,
        }
    }

    fn wire(msg_type: &str, data: Value, binary: Option<&[u8]>) -> Vec<u8> {
        let mut m = msg(msg_type, data);
        if let Some(b) = binary {
            m.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
            m.count = Some((b.len() / 12) as u32);
        }
        serialize_message(&m, binary)
    }

    fn grid_header(width: u32, height: u32, origin: [f64; 3]) -> Value {
        json!({"width": width, "height": height, "resolution": 0.05,
               "origin": origin, "kind": "occupancy"})
    }

    #[test]
    fn nav_layer_matches_with_or_without_robot_id() {
        assert_eq!(nav_layer("tracer/nav/plan"), Some(NavLayer::Plan));
        assert_eq!(nav_layer("nav/plan"), Some(NavLayer::Plan));
        assert_eq!(nav_layer("a/b/nav/plan"), None);
        assert_eq!(nav_layer("tracer/map/grid"), Some(NavLayer::Grid));
        assert_eq!(nav_layer("plan"), None);
        assert_eq!(nav_layer("world/lidar"), None);
        for (name, layer) in [
            ("map/cloud", NavLayer::MapCloud),
            ("robot/pose", NavLayer::Pose),
            ("robot/footprint", NavLayer::Footprint),
            ("nav/trail", NavLayer::Trail),
            ("nav/goal", NavLayer::Goal),
            ("sensors/scan", NavLayer::Scan),
            ("sensors/cloud", NavLayer::Cloud),
            ("nav/status", NavLayer::Status),
        ] {
            assert_eq!(nav_layer(name), Some(layer), "{name}");
        }
    }

    #[test]
    fn costmap_layer_parsed() {
        let mut scene = NavScene::default();
        assert_eq!(nav_layer("tracer/costmap/global"), Some(NavLayer::Costmap));
        let header = json!({"width": 3, "height": 2, "resolution": 0.05,
                            "origin": [0.0, 0.0, 0.0], "kind": "costmap"});
        let cells = [0u8, 1, 252, 253, 254, 255];
        assert!(scene.apply(NavLayer::Costmap, &msg("grid2d", header), Some(&cells)));
        let costmap = scene.costmap.as_ref().unwrap();
        assert_eq!(costmap.generation, 1);
        assert_eq!(costmap.cells.len(), 6);
        assert!(scene.grid.is_none());
    }

    #[test]
    fn pose_yaw_from_quaternion() {
        let mut scene = NavScene::default();
        let data =
            json!({"translation": [1.0, 2.0, 0.0], "rotation": [0.0, 0.0, 0.7071068, 0.7071068]});
        assert!(scene.apply(NavLayer::Pose, &msg("transform3d", data), None));
        let pose = scene.pose.clone().unwrap();
        assert!(
            (pose.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-6,
            "{pose:?}"
        );
        assert_eq!((pose.x, pose.y), (1.0, 2.0));
        assert_eq!(scene.updated, 1);
    }

    #[test]
    fn grid_updates_generation() {
        let mut scene = NavScene::default();
        let cells = vec![255u8; 6];
        let first = msg("grid2d", grid_header(3, 2, [0.0; 3]));
        assert!(scene.apply(NavLayer::Grid, &first, Some(&cells)));
        let grid = scene.grid.as_ref().unwrap();
        assert_eq!((grid.generation, grid.cells.len()), (1, 6));
        let second = msg("grid2d", grid_header(2, 2, [1.0, 1.0, 0.0]));
        assert!(scene.apply(NavLayer::Grid, &second, Some(&[0u8; 4])));
        let grid = scene.grid.as_ref().unwrap();
        assert_eq!((grid.generation, grid.cells.len()), (2, 4));
        assert_eq!(grid.header.width, 2);
    }

    #[test]
    fn status_not_forwarded() {
        let mut scene = NavScene::default();
        let status = json!({"loc_ok": true, "loc_reason": "", "armed": false, "estop": false,
                            "gate_reason": ""});
        assert!(!scene.apply(NavLayer::Status, &msg("status", status), None));
        assert!(scene.status.as_ref().unwrap().loc_ok);
        assert!(scene.status_at.is_some());
        let plan = json!({"strips": [[[0.0, 0.0, 0.0], [1.0, 2.0, 0.5]], [[9.0, 9.0, 9.0]]],
                          "color": [0, 255, 0, 255]});
        assert!(scene.apply(NavLayer::Plan, &msg("linestrips3d", plan), None));
        assert_eq!(scene.plan, vec![[0.0, 0.0], [1.0, 2.0]]);
        assert_eq!(scene.colors["nav/plan"], [0, 255, 0, 255]);
    }

    #[test]
    fn goal_and_scan_values() {
        let mut scene = NavScene::default();
        let goal = json!({"origins": [[1.0, 2.0, 0.0], [5.0, 5.0, 0.0]],
                          "vectors": [[0.5, 0.0, 0.0], [1.0, 1.0, 1.0]]});
        assert!(scene.apply(NavLayer::Goal, &msg("arrows3d", goal), None));
        assert_eq!(scene.goal, Some(([1.0, 2.0], [0.5, 0.0])));
        let xyz: Vec<u8> = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut scan = msg("points3d", Value::Null);
        scan.count = Some(2);
        scan.format = Some(binary_formats::POINTS_XYZ_F32.to_string());
        assert!(scene.apply(NavLayer::Scan, &scan, Some(&xyz)));
        assert_eq!(scene.scan, vec![[1.0, 2.0], [4.0, 5.0]]);
        let json_scan = msg("points3d", json!({"positions": [[7.0, 8.0, 9.0]]}));
        assert!(scene.apply(NavLayer::Scan, &json_scan, None));
        assert_eq!(scene.scan, vec![[7.0, 8.0]]);
    }

    #[test]
    fn bad_points_are_malformed() {
        let mut state = SessionState::default();
        let good = wire("points3d", json!({"positions": [[1.0, 2.0, 0.0]]}), None);
        assert!(accept(&mut state, "dviz", "dviz/tracer/sensors/scan", &good).is_some());
        assert_eq!(state.nav.scan, vec![[1.0, 2.0]]);
        let updated = state.nav.updated;

        // JSON cannot carry NaN, so use the binary form.
        let nan: Vec<u8> = [f32::NAN, 0.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let bad = wire("points3d", Value::Null, Some(&nan));
        assert!(accept(&mut state, "dviz", "dviz/tracer/sensors/scan", &bad).is_none());
        assert_eq!(state.malformed, 1);
        assert_eq!(state.nav.scan, vec![[1.0, 2.0]]);
        assert_eq!(state.nav.updated, updated);

        // A wrong length is malformed too.
        let mut short = msg("points3d", Value::Null);
        short.count = Some(5);
        let bytes = serialize_message(&short, Some(&nan));
        assert!(accept(&mut state, "dviz", "dviz/tracer/sensors/scan", &bytes).is_none());
        assert_eq!(state.malformed, 2);
        assert_eq!(state.nav.scan, vec![[1.0, 2.0]]);
    }

    #[test]
    fn non_finite_grid_origin_is_malformed() {
        let mut state = SessionState::default();
        let cells = [0u8; 4];
        let ok = wire("grid2d", grid_header(2, 2, [0.0; 3]), Some(&cells));
        assert!(accept(&mut state, "dviz", "dviz/map/grid", &ok).is_some());
        // 1e39 overflows f32 to infinity when parsed.
        let text = r#"{"type":"grid2d","data":{"width":2,"height":2,"resolution":0.05,"origin":[1e39,0,0],"kind":"occupancy"}}"#;
        let mut bad = text.as_bytes().to_vec();
        bad.push(b'\n');
        bad.extend_from_slice(&cells);
        let before = state.nav.updated;
        let result = accept(&mut state, "dviz", "dviz/map/grid", &bad);
        let grid = state.nav.grid.as_ref().unwrap();
        assert_eq!(grid.generation, 1, "the scene must not change");
        assert_eq!(state.nav.updated, before);
        assert_eq!(state.malformed, 1, "{:?}", result.is_some());
        assert!(result.is_none());
        // The overflow parses as infinity, so it is the origin check that refuses it.
        let header: Grid2DHeader =
            serde_json::from_str(&text[text.find("\"data\":").unwrap() + 7..text.len() - 1])
                .unwrap();
        assert!(header.origin[0].is_infinite());
    }
}
