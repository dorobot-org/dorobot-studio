//! Frame maths: body (`base_footprint`) to map.

use crate::formats::{Odom, Scan};

pub fn yaw_from_quat(x: f64, y: f64, z: f64, w: f64) -> f64 {
    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
}

/// Body-frame point to map frame.
pub fn to_map(pose: &Odom, p: [f64; 2]) -> [f64; 2] {
    let (s, c) = pose.yaw.sin_cos();
    [pose.x + c * p[0] - s * p[1], pose.y + s * p[0] + c * p[1]]
}

/// Scan points (body frame) with a finite range inside `[range_min, range_max]`.
pub fn scan_points(scan: &Scan) -> Vec<[f64; 2]> {
    scan.ranges
        .iter()
        .enumerate()
        .filter(|(_, r)| r.is_finite() && **r >= scan.range_min && **r <= scan.range_max)
        .map(|(i, r)| {
            let a = f64::from(scan.angle_min) + i as f64 * f64::from(scan.angle_increment);
            [f64::from(*r) * a.cos(), f64::from(*r) * a.sin()]
        })
        .collect()
}

/// Footprint polygon placed in the map frame, closed (first point repeated), z = 0.
/// Empty for an empty polygon.
pub fn footprint_in_map(pose: &Odom, poly: &[[f32; 2]]) -> Vec<[f32; 3]> {
    let mut out: Vec<[f32; 3]> = poly
        .iter()
        .map(|p| {
            let m = to_map(pose, [f64::from(p[0]), f64::from(p[1])]);
            [m[0] as f32, m[1] as f32, 0.0]
        })
        .collect();
    if let Some(first) = out.first().copied() {
        out.push(first);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn odom(x: f64, y: f64, yaw: f64) -> Odom {
        Odom { stamp: 0.0, x, y, yaw, v: 0.0, w: 0.0 }
    }

    #[test]
    fn yaw_from_quat_basic() {
        let h = (FRAC_PI_2 / 2.0).sin_cos();
        assert!((yaw_from_quat(0.0, 0.0, h.0, h.1) - FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn to_map_rotates_and_translates() {
        let p = to_map(&odom(1.0, 2.0, FRAC_PI_2), [1.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 1e-9 && (p[1] - 3.0).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn footprint_closed_and_placed() {
        let poly = [[0.09, 0.20], [0.09, -0.20], [-0.31, -0.20], [-0.31, 0.20]];
        let f = footprint_in_map(&odom(0.0, 0.0, 0.0), &poly);
        assert_eq!(f.len(), 5);
        assert_eq!(f[0], f[4]);
        let min = f.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let max = f.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        assert!((min - -0.31).abs() < 1e-6 && (max - 0.09).abs() < 1e-6);
        assert!(f.iter().all(|p| p[2] == 0.0));
        assert!(footprint_in_map(&odom(0.0, 0.0, 0.0), &[]).is_empty());
    }
}
