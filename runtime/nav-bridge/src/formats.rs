//! Parsers for the DORA_NAV wire formats. Bad data is an `Err`, never a panic.

use dviz_protocol::{parse_grid2d, Grid2DHeader, GridKind};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Odom {
    pub stamp: f64,
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
    pub v: f64,
    pub w: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathMsg {
    pub goal_id: String,
    pub points: Vec<[f64; 2]>,
    pub error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalMsg {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub yaw: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scan {
    pub stamp: f64,
    pub angle_min: f32,
    pub angle_increment: f32,
    pub range_min: f32,
    pub range_max: f32,
    pub ranges: Vec<f32>,
}

pub fn parse_json(b: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(b).map_err(|e| format!("bad json: {e}"))
}

fn num(v: &Value, path: &[&str]) -> Result<f64, String> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k).ok_or_else(|| format!("missing {}", path.join(".")))?;
    }
    match cur.as_f64() {
        Some(f) if f.is_finite() => Ok(f),
        _ => Err(format!("{} is not a finite number", path.join("."))),
    }
}

fn text(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing string {key}"))
}

pub fn parse_odometry(b: &[u8]) -> Result<Odom, String> {
    let v = parse_json(b)?;
    let (qx, qy) = (num(&v, &["pose", "orientation", "x"])?, num(&v, &["pose", "orientation", "y"])?);
    let (qz, qw) = (num(&v, &["pose", "orientation", "z"])?, num(&v, &["pose", "orientation", "w"])?);
    Ok(Odom {
        stamp: num(&v, &["header", "timestamp"])?,
        x: num(&v, &["pose", "position", "x"])?,
        y: num(&v, &["pose", "position", "y"])?,
        yaw: crate::geom::yaw_from_quat(qx, qy, qz, qw),
        v: num(&v, &["twist", "linear", "x"])?,
        w: num(&v, &["twist", "angular", "z"])?,
    })
}

pub fn parse_path(b: &[u8]) -> Result<PathMsg, String> {
    let v = parse_json(b)?;
    let poses = v.get("poses").and_then(Value::as_array).ok_or("missing poses")?;
    let points = poses
        .iter()
        .map(|p| Ok([num(p, &["x"])?, num(p, &["y"])?]))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PathMsg {
        goal_id: v.get("goal_id").and_then(Value::as_str).unwrap_or("").to_owned(),
        points,
        error: v.get("error").and_then(Value::as_bool).unwrap_or(false),
    })
}

pub fn parse_goal(b: &[u8]) -> Result<GoalMsg, String> {
    let v = parse_json(b)?;
    let yaw = match v.get("yaw") {
        None | Some(Value::Null) => None,
        Some(_) => Some(num(&v, &["yaw"])?),
    };
    Ok(GoalMsg { id: text(&v, "id")?, x: num(&v, &["x"])?, y: num(&v, &["y"])?, yaw })
}

/// Returns (id, state, reason).
pub fn parse_goal_status(b: &[u8]) -> Result<(String, String, String), String> {
    let v = parse_json(b)?;
    let reason = v.get("reason").and_then(Value::as_str).unwrap_or("").to_owned();
    Ok((text(&v, "id")?, text(&v, "state")?, reason))
}

fn le_f32(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le_f64(b: &[u8]) -> f64 {
    f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Little-endian: f64 stamp, 4 x f32 (angle_min, angle_increment, range_min,
/// range_max), u32 n, n x f32. Total length must be 28 + 4n.
pub fn parse_scan(b: &[u8]) -> Result<Scan, String> {
    if b.len() < 28 {
        return Err(format!("scan too short: {} bytes", b.len()));
    }
    let n = le_u32(b, 24) as usize;
    let expected = n.checked_mul(4).and_then(|x| x.checked_add(28));
    if expected != Some(b.len()) {
        return Err(format!("scan length {} does not match n={n}", b.len()));
    }
    Ok(Scan {
        stamp: le_f64(b),
        angle_min: le_f32(b, 8),
        angle_increment: le_f32(b, 12),
        range_min: le_f32(b, 16),
        range_max: le_f32(b, 20),
        ranges: (0..n).map(|i| le_f32(b, 28 + 4 * i)).collect(),
    })
}

/// f64 stamp, u32 N, N x (x, y, z, intensity) f32 (little-endian).
pub fn parse_cloud_xyzi(b: &[u8]) -> Result<(f64, Vec<[f32; 4]>), String> {
    if b.len() < 12 {
        return Err(format!("cloud too short: {} bytes", b.len()));
    }
    let n = le_u32(b, 8) as usize;
    let expected = n.checked_mul(16).and_then(|x| x.checked_add(12));
    if expected != Some(b.len()) {
        return Err(format!("cloud length {} does not match N={n}", b.len()));
    }
    let pts = (0..n)
        .map(|i| {
            let o = 12 + 16 * i;
            [le_f32(b, o), le_f32(b, o + 4), le_f32(b, o + 8), le_f32(b, o + 12)]
        })
        .collect();
    Ok((le_f64(b), pts))
}

/// Size of the costmap header: f64 stamp, u32 width, u32 height, f32 resolution,
/// f32 origin x, y, yaw (little-endian), then `width * height` Nav2 cost bytes.
const COSTMAP_HEADER: usize = 32;

/// The planner's costmap: 32-byte header, then `width * height` u8 Nav2 costs
/// (0 free, 1..=252 inflated, 253 inscribed, 254 lethal, 255 unknown), row 0 = min y.
/// A wrong length, empty grid, over-large grid or bad resolution is an error.
pub fn parse_costmap(b: &[u8]) -> Result<(Grid2DHeader, Vec<u8>), String> {
    if b.len() < COSTMAP_HEADER {
        return Err(format!("costmap too short: {} bytes", b.len()));
    }
    let (width, height) = (le_u32(b, 8), le_u32(b, 12));
    let header = Grid2DHeader {
        width,
        height,
        resolution: le_f32(b, 16),
        origin: [le_f32(b, 20), le_f32(b, 24), le_f32(b, 28)],
        kind: GridKind::Costmap,
    };
    let cells = &b[COSTMAP_HEADER..];
    match parse_grid2d(&header, cells) {
        Some(cells) => Ok((header, cells.to_vec())),
        None => Err(format!("costmap {width}x{height} res {} does not match its {} cells", header.resolution, cells.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn costmap_bytes(w: u32, h: u32, res: f32, cells: &[u8]) -> Vec<u8> {
        let mut b = 12.5f64.to_le_bytes().to_vec();
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        for v in [res, -1.0, 2.0, 0.5] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(cells);
        b
    }

    #[test]
    fn parse_costmap_layout() {
        let cells = [0u8, 1, 253, 254, 255, 7];
        let (h, c) = parse_costmap(&costmap_bytes(3, 2, 0.05, &cells)).unwrap();
        assert_eq!((h.width, h.height, h.resolution), (3, 2, 0.05));
        assert_eq!(h.origin, [-1.0, 2.0, 0.5]);
        assert_eq!(h.kind, GridKind::Costmap);
        assert_eq!(c, cells);
        // wrong length, short header, empty, bad resolution, absurd size: Err, no panic
        assert!(parse_costmap(&costmap_bytes(3, 2, 0.05, &cells[..5])).is_err());
        assert!(parse_costmap(&costmap_bytes(3, 2, 0.05, &[cells.as_slice(), &[0]].concat())).is_err());
        assert!(parse_costmap(&[0u8; 31]).is_err());
        assert!(parse_costmap(&[]).is_err());
        assert!(parse_costmap(&costmap_bytes(0, 2, 0.05, &[])).is_err());
        assert!(parse_costmap(&costmap_bytes(3, 2, 0.0, &cells)).is_err());
        assert!(parse_costmap(&costmap_bytes(3, 2, f32::NAN, &cells)).is_err());
        assert!(parse_costmap(&costmap_bytes(u32::MAX, u32::MAX, 0.05, &cells)).is_err());
    }

    const ODOM: &str = include_str!("../tests/fixtures/map_base.json");

    fn scan_bytes(n_decl: u32, ranges: &[f32]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1.5f64.to_le_bytes());
        for v in [-1.0f32, 1.0, 0.1, 10.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&n_decl.to_le_bytes());
        for r in ranges {
            b.extend_from_slice(&r.to_le_bytes());
        }
        b
    }

    #[test]
    fn parse_odometry_sample() {
        let o = parse_odometry(ODOM.as_bytes()).unwrap();
        assert!((o.x - 0.017345933746).abs() < 1e-9);
        assert!((o.yaw - -0.04615).abs() < 1e-4);
        assert!((o.v - 0.0016644310671836138).abs() < 1e-12);
        assert_eq!(o.w, 0.0);
    }

    #[test]
    fn parse_path_and_error() {
        let ok = br#"{"header":{"timestamp":1.0,"seq":1,"frame_id":"map"},"goal_id":"C1","poses":[{"x":1,"y":2}],"error":false}"#;
        let p = parse_path(ok).unwrap();
        assert_eq!(p.goal_id, "C1");
        assert_eq!(p.points, vec![[1.0, 2.0]]);
        assert!(!p.error);
        let bad = br#"{"header":{},"goal_id":"C1","poses":[],"error":true}"#;
        assert!(parse_path(bad).unwrap().error);
    }

    #[test]
    fn parse_goal_and_status_samples() {
        let g = parse_goal(include_str!("../tests/fixtures/goal.json").as_bytes()).unwrap();
        assert_eq!((g.id.as_str(), g.x, g.y, g.yaw), ("C1", 0.0, 0.0, Some(0.0)));
        let g = parse_goal(br#"{"id":"C9","x":1.5,"y":-2}"#).unwrap();
        assert_eq!(g.yaw, None);
        let (id, state, reason) =
            parse_goal_status(include_str!("../tests/fixtures/goal_status.json").as_bytes()).unwrap();
        assert_eq!((id.as_str(), state.as_str(), reason.as_str()), ("C1", "ACTIVE", ""));
    }

    #[test]
    fn parse_scan_layout() {
        let b = scan_bytes(3, &[1.0, f32::INFINITY, 2.0]);
        assert_eq!(b.len(), 28 + 12);
        let s = parse_scan(&b).unwrap();
        assert_eq!(s.ranges.len(), 3);
        assert_eq!(s.angle_min, -1.0);
        assert_eq!(s.range_max, 10.0);
        assert_eq!(crate::geom::scan_points(&s).len(), 2);
        assert!(parse_scan(&b[..b.len() - 1]).is_err());
        let mut longer = b.clone();
        longer.push(0);
        assert!(parse_scan(&longer).is_err());
    }

    #[test]
    fn parse_cloud_xyzi_layout() {
        let mut b = Vec::new();
        b.extend_from_slice(&2.5f64.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let (stamp, pts) = parse_cloud_xyzi(&b).unwrap();
        assert_eq!(stamp, 2.5);
        assert_eq!(pts, vec![[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]]);
        b[8..12].copy_from_slice(&3u32.to_le_bytes());
        assert!(parse_cloud_xyzi(&b).is_err());
    }

    #[test]
    fn bad_inputs_are_skipped_and_counted() {
        let s = scan_bytes(3, &[1.0, 2.0, 3.0]);
        assert!(parse_scan(&s[..20]).is_err());
        assert!(parse_scan(&[]).is_err());
        assert!(parse_scan(&scan_bytes(u32::MAX, &[1.0])).is_err());
        assert!(parse_cloud_xyzi(&[1, 2, 3]).is_err());
        let nan = ODOM.replace("0.01734593374616823", "NaN");
        assert!(parse_odometry(nan.as_bytes()).is_err());
        let nan2 = br#"{"header":{"timestamp":1},"pose":{"position":{"x":1e999,"y":0},"orientation":{"x":0,"y":0,"z":0,"w":1}},"twist":{"linear":{"x":0},"angular":{"z":0}}}"#;
        assert!(parse_odometry(nan2).is_err());
        assert!(parse_odometry(b"not json").is_err());
        assert!(parse_path(b"{").is_err());
        assert!(parse_goal(b"[]").is_err());
        assert!(parse_goal_status(b"nope").is_err());
        assert!(parse_json(b"\xff\xfe").is_err());
        assert!(parse_json(b"{\"a\":1}").is_ok());
    }
}
