//! Build dviz messages: each returns (entity, serialized message bytes).

use crate::formats::{GoalMsg, Odom};
use dviz_protocol::{
    binary_formats, nav_entities, serialize_message, Grid2DHeader, GridKind, MvizMessage,
    NavStatus,
};
use serde_json::{json, Value};

fn msg(msg_type: &str, data: Value) -> MvizMessage {
    MvizMessage { msg_type: msg_type.to_owned(), timestamp: None, data, format: None, count: None }
}

/// An f32 as the f64 with its shortest decimal form (0.025, not 0.02500000037252903).
fn short(v: f32) -> f64 {
    v.to_string().parse().unwrap_or_else(|_| f64::from(v))
}

/// A `grid2d`: the map for an occupancy grid, `costmap/global` for a costmap.
pub fn enc_grid(h: &Grid2DHeader, cells: &[u8]) -> (&'static str, Vec<u8>) {
    let m = msg("grid2d", serde_json::to_value(h).unwrap_or(Value::Null));
    let entity = match h.kind {
        GridKind::Occupancy => nav_entities::MAP_GRID,
        GridKind::Costmap => nav_entities::COSTMAP_GLOBAL,
    };
    (entity, serialize_message(&m, Some(cells)))
}

/// Binary `xyz_f32`. An empty set is sent as inline JSON (`positions: []`) because
/// a zero-length binary payload is not carried by the wire format.
pub fn enc_points(entity: &'static str, pts: &[[f32; 3]], color: Option<[u8; 4]>) -> (&'static str, Vec<u8>) {
    let mut data = serde_json::Map::new();
    if let Some(c) = color {
        data.insert("color".into(), json!(c));
    }
    if pts.is_empty() {
        data.insert("positions".into(), json!([]));
        return (entity, serialize_message(&msg("points3d", Value::Object(data)), None));
    }
    let mut m = msg("points3d", Value::Object(data));
    m.format = Some(binary_formats::POINTS_XYZ_F32.to_owned());
    m.count = Some(pts.len() as u32);
    let mut bin = Vec::with_capacity(pts.len() * 12);
    for p in pts {
        for v in p {
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    (entity, serialize_message(&m, Some(&bin)))
}

/// One strip at z = 0. No points gives a single empty strip (`[[]]`), which clears the layer.
pub fn enc_strip(entity: &'static str, pts: &[[f32; 2]], color: [u8; 4], radius: f32) -> (&'static str, Vec<u8>) {
    let strip: Vec<[f32; 3]> = pts.iter().map(|p| [p[0], p[1], 0.0]).collect();
    let m = msg("linestrips3d", json!({"strips": [strip], "color": color, "radius": short(radius)}));
    (entity, serialize_message(&m, None))
}

pub fn enc_pose(o: &Odom) -> (&'static str, Vec<u8>) {
    let (s, c) = (o.yaw / 2.0).sin_cos();
    let m = msg(
        "transform3d",
        json!({"translation": [o.x as f32, o.y as f32, 0.0], "rotation": [0.0, 0.0, s as f32, c as f32]}),
    );
    (nav_entities::ROBOT_POSE, serialize_message(&m, None))
}

/// `None` clears the goal (empty arrows). With a yaw the arrow is 0.5 m long in that
/// direction; without one it is a 0.5 m vertical marker.
pub fn enc_goal(g: Option<&GoalMsg>, color: [u8; 4], radius: f32) -> (&'static str, Vec<u8>) {
    let data = match g {
        None => json!({"origins": [], "vectors": [], "color": color, "radius": short(radius)}),
        Some(g) => {
            let v = match g.yaw {
                Some(yaw) => [(0.5 * yaw.cos()) as f32, (0.5 * yaw.sin()) as f32, 0.0],
                None => [0.0, 0.0, 0.5],
            };
            json!({"origins": [[g.x as f32, g.y as f32, 0.0]], "vectors": [v], "color": color, "radius": short(radius)})
        }
    };
    (nav_entities::NAV_GOAL, serialize_message(&msg("arrows3d", data), None))
}

pub fn enc_status(s: &NavStatus) -> (&'static str, Vec<u8>) {
    let m = msg("status", serde_json::to_value(s).unwrap_or(Value::Null));
    (nav_entities::NAV_STATUS, serialize_message(&m, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dviz_protocol::{parse_message, GridKind, Grid2DHeader, LineStrips3DData, parse_points_xyz_f32,
        Arrows3DData, Transform3DData, parse_grid2d, NavStatus, nav_entities};

    #[test]
    fn error_path_clears_plan() {
        let (e, bytes) = enc_strip(nav_entities::NAV_PLAN, &[], [0, 255, 0, 255], 0.025);
        assert_eq!(e, "nav/plan");
        let (m, bin) = parse_message(&bytes).unwrap();
        assert!(bin.is_none());
        assert_eq!(m.msg_type, "linestrips3d");
        let d: LineStrips3DData = serde_json::from_value(m.data).unwrap();
        assert_eq!(d.strips, vec![Vec::<[f32; 3]>::new()]);
        assert_eq!(d.color, Some([0, 255, 0, 255]));
    }

    #[test]
    fn strips_carry_radius() {
        let (_, bytes) = enc_strip(nav_entities::NAV_PLAN, &[[0.0, 0.0]], [0, 180, 255, 255], 0.025);
        let (m, _) = parse_message(&bytes).unwrap();
        assert_eq!(m.data["radius"], serde_json::json!(0.025));
        let d: LineStrips3DData = serde_json::from_value(m.data).unwrap();
        assert_eq!(d.radius, Some(0.025));
        let g = crate::formats::GoalMsg { id: "C1".into(), x: 1.0, y: 2.0, yaw: None };
        for goal in [Some(&g), None] {
            let (_, bytes) = enc_goal(goal, [255, 170, 0, 255], 0.015);
            let d: Arrows3DData = serde_json::from_value(parse_message(&bytes).unwrap().0.data).unwrap();
            assert_eq!(d.radius, Some(0.015));
        }
    }

    #[test]
    fn costmap_goes_to_its_own_entity() {
        let h = Grid2DHeader { width: 2, height: 1, resolution: 0.05, origin: [0.0, 0.0, 0.0], kind: GridKind::Costmap };
        let (e, bytes) = enc_grid(&h, &[0, 254]);
        assert_eq!(e, "costmap/global");
        let (m, bin) = parse_message(&bytes).unwrap();
        let h2: Grid2DHeader = serde_json::from_value(m.data).unwrap();
        assert_eq!((h2.kind, bin.unwrap()), (GridKind::Costmap, &[0u8, 254][..]));
    }

    #[test]
    fn strip_with_points() {
        let (_, bytes) = enc_strip(nav_entities::NAV_TRAIL, &[[1.0, 2.0], [3.0, 4.0]], [255, 140, 0, 255], 0.02);
        let (m, _) = parse_message(&bytes).unwrap();
        let d: LineStrips3DData = serde_json::from_value(m.data).unwrap();
        assert_eq!(d.strips, vec![vec![[1.0, 2.0, 0.0], [3.0, 4.0, 0.0]]]);
    }

    #[test]
    fn points_binary_and_empty() {
        let pts = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let (e, bytes) = enc_points(nav_entities::SENSORS_SCAN, &pts, Some([255, 0, 0, 255]));
        assert_eq!(e, "sensors/scan");
        let (m, bin) = parse_message(&bytes).unwrap();
        assert_eq!((m.format.as_deref(), m.count), (Some("xyz_f32"), Some(2)));
        assert_eq!(parse_points_xyz_f32(bin.unwrap(), 2).unwrap(), pts.to_vec());
        assert_eq!(m.data["color"], serde_json::json!([255, 0, 0, 255]));
        // no colour: key absent (plugin colours by height)
        let (_, b2) = enc_points(nav_entities::SENSORS_CLOUD, &pts, None);
        assert!(parse_message(&b2).unwrap().0.data.get("color").is_none());
        // empty: inline JSON so the plugin does not get a count-0 binary message
        let (_, b3) = enc_points(nav_entities::SENSORS_SCAN, &[], Some([255, 0, 0, 255]));
        let (m3, bin3) = parse_message(&b3).unwrap();
        assert!(bin3.is_none() && m3.format.is_none());
        assert_eq!(m3.data["positions"], serde_json::json!([]));
    }

    #[test]
    fn grid_round_trip() {
        let h = Grid2DHeader { width: 2, height: 2, resolution: 0.05, origin: [-1.0, -2.0, 0.0], kind: GridKind::Occupancy };
        let (e, bytes) = enc_grid(&h, &[0, 100, 255, 0]);
        assert_eq!(e, "map/grid");
        let (m, bin) = parse_message(&bytes).unwrap();
        assert_eq!(m.msg_type, "grid2d");
        let h2: Grid2DHeader = serde_json::from_value(m.data).unwrap();
        assert_eq!(h2, h);
        assert_eq!(parse_grid2d(&h2, bin.unwrap()), Some(&[0u8, 100, 255, 0][..]));
    }

    #[test]
    fn pose_goal_status() {
        let o = crate::formats::Odom { stamp: 0.0, x: 1.0, y: 2.0, yaw: std::f64::consts::FRAC_PI_2, v: 0.0, w: 0.0 };
        let (e, bytes) = enc_pose(&o);
        assert_eq!(e, "robot/pose");
        let d: Transform3DData = serde_json::from_value(parse_message(&bytes).unwrap().0.data).unwrap();
        assert_eq!(d.translation, Some([1.0, 2.0, 0.0]));
        let q = d.rotation.unwrap();
        assert!((q[2] - 0.70710677).abs() < 1e-6 && (q[3] - 0.70710677).abs() < 1e-6 && q[0] == 0.0 && q[1] == 0.0);

        let g = crate::formats::GoalMsg { id: "C1".into(), x: 1.0, y: 2.0, yaw: Some(0.0) };
        let (e, bytes) = enc_goal(Some(&g), [255, 0, 255, 255], 0.015);
        assert_eq!(e, "nav/goal");
        let d: Arrows3DData = serde_json::from_value(parse_message(&bytes).unwrap().0.data).unwrap();
        assert_eq!(d.origins, vec![[1.0, 2.0, 0.0]]);
        assert_eq!(d.vectors, vec![[0.5, 0.0, 0.0]]);
        let (_, bytes) = enc_goal(None, [255, 0, 255, 255], 0.015);
        let d: Arrows3DData = serde_json::from_value(parse_message(&bytes).unwrap().0.data).unwrap();
        assert!(d.origins.is_empty() && d.vectors.is_empty());

        let s = NavStatus { armed: true, goal_id: Some("C1".into()), ..Default::default() };
        let (e, bytes) = enc_status(&s);
        assert_eq!(e, "nav/status");
        let (m, bin) = parse_message(&bytes).unwrap();
        assert!(bin.is_none() && m.msg_type == "status");
        assert_eq!(serde_json::from_value::<NavStatus>(m.data).unwrap(), s);
    }
}
