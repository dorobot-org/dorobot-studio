//! The robot body models the Rerun view draws below the robot pose, embedded
//! in the plugin. `assets/tracer_mini.stl` is made by
//! `tools/make_tracer_model.py` from the tracer-mini-ros2 body mesh; see
//! `assets/NOTICE-tracer-mini.md`. Compiled only with the `rerun` feature.

/// The Tracer Mini body: a binary STL in metres, x forward, its bounding box
/// centred on (-0.11, 0) of `base_footprint` with its lowest point at z = 0.
const TRACER_MINI: &[u8] = include_bytes!("../assets/tracer_mini.stl");

/// The embedded model `name` names (as `NavStatus::robot_model` does).
pub(crate) fn model(name: &str) -> Option<&'static [u8]> {
    match name {
        "tracer_mini" => Some(TRACER_MINI),
        _ => None,
    }
}

/// The body's colour: dark grey.
const BODY_COLOR: [u8; 4] = [64, 64, 64, 255];

/// Bytes of a binary STL before its triangles: an 80-byte header, then the
/// triangle count (u32, little-endian).
const STL_HEADER: usize = 84;
/// Bytes per triangle: normal and three vertices (12 f32), 2 attribute bytes.
const STL_TRIANGLE: usize = 50;

/// What a binary STL holds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StlBounds {
    pub triangles: usize,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// The triangle count and bounding box of a binary STL. An error if the
/// length does not match the count, there is no triangle, or a vertex is
/// not finite; never a panic.
pub(crate) fn stl_bounds(bytes: &[u8]) -> Result<StlBounds, String> {
    let count: [u8; 4] = bytes
        .get(80..STL_HEADER)
        .and_then(|count| count.try_into().ok())
        .ok_or_else(|| format!("STL of {} bytes has no header", bytes.len()))?;
    let triangles = u32::from_le_bytes(count) as usize;
    let length = triangles
        .checked_mul(STL_TRIANGLE)
        .and_then(|length| length.checked_add(STL_HEADER));
    if length != Some(bytes.len()) {
        return Err(format!(
            "STL of {} bytes does not hold the {triangles} triangles its header names",
            bytes.len()
        ));
    }
    if triangles == 0 {
        return Err("STL without triangles".to_string());
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for triangle in bytes[STL_HEADER..].chunks_exact(STL_TRIANGLE) {
        // Skip the normal; the vertices follow it.
        for vertex in triangle[12..48].chunks_exact(12) {
            for (axis, value) in vertex.chunks_exact(4).enumerate() {
                let value = f32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                if !value.is_finite() {
                    return Err("STL vertex is not finite".to_string());
                }
                min[axis] = min[axis].min(value);
                max[axis] = max[axis].max(value);
            }
        }
    }
    Ok(StlBounds {
        triangles,
        min,
        max,
    })
}

/// `bytes`, a binary STL, as a dark grey Rerun asset; an error, never a
/// panic, if they are not a usable binary STL (see `stl_bounds`).
pub(crate) fn asset(bytes: &[u8]) -> Result<rerun::Asset3D, String> {
    stl_bounds(bytes)?;
    let [r, g, b, a] = BODY_COLOR;
    Ok(
        rerun::Asset3D::from_file_contents(bytes.to_vec(), Some(rerun::MediaType::stl()))
            .with_albedo_factor(rerun::datatypes::Rgba32::from_unmultiplied_rgba(r, g, b, a)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_asset_small_and_aligned() {
        let bytes = model("tracer_mini").expect("the Tracer Mini model is embedded");
        assert!(bytes.len() <= 1_000_000, "{} bytes", bytes.len());
        let bounds = stl_bounds(bytes).expect("a usable binary STL");
        assert_eq!(bounds.triangles, (bytes.len() - 84) / 50);
        let centre_x = (bounds.min[0] + bounds.max[0]) / 2.0;
        let centre_y = (bounds.min[1] + bounds.max[1]) / 2.0;
        assert!((centre_x + 0.11).abs() <= 0.01, "centre x {centre_x}");
        assert!(centre_y.abs() <= 0.01, "centre y {centre_y}");
        assert!(bounds.min[2].abs() <= 0.001, "floor z {}", bounds.min[2]);
        // In metres: the body is about 0.43 x 0.42 x 0.27 m.
        for axis in 0..3 {
            let extent = bounds.max[axis] - bounds.min[axis];
            assert!((0.2..0.6).contains(&extent), "axis {axis}: {extent} m");
        }
        assert!(asset(bytes).is_ok());
        assert_eq!(model("tracer"), None);
    }

    #[test]
    fn corrupt_model_asset_is_an_error_not_a_panic() {
        let good = model("tracer_mini").unwrap();
        let mut bad: Vec<Vec<u8>> = vec![
            Vec::new(),
            good[..83].to_vec(),
            good[..84].to_vec(),
            good[..good.len() / 2].to_vec(),
            good[..good.len() - 1].to_vec(),
            [good, &[0]].concat(),
        ];
        // A header that names more triangles than any file holds.
        let mut huge = good[..134].to_vec();
        huge[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        bad.push(huge);
        // No triangles at all.
        let mut empty = good[..84].to_vec();
        empty[80..84].copy_from_slice(&0u32.to_le_bytes());
        bad.push(empty);
        // A vertex that is not a number (the first vertex's x).
        let mut nan = good.to_vec();
        nan[96..100].copy_from_slice(&f32::NAN.to_le_bytes());
        bad.push(nan);
        for (case, bytes) in bad.iter().enumerate() {
            assert!(stl_bounds(bytes).is_err(), "case {case} was read");
            assert!(asset(bytes).is_err(), "case {case} became an asset");
        }
    }
}
