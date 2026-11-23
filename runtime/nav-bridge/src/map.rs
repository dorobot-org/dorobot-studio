//! Map loading: PGM + YAML occupancy grid, PCD point cloud, voxel downsampling.

use dviz_protocol::{Grid2DHeader, GridKind, GRID_FREE, GRID_OCCUPIED, GRID_UNKNOWN};
use std::collections::HashSet;
use std::path::Path;

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn yaml_value<'a>(yaml: &'a str, key: &str) -> Option<&'a str> {
    yaml.lines().find_map(|l| {
        let l = l.split('#').next().unwrap_or("");
        let (k, v) = l.split_once(':')?;
        (k.trim() == key).then(|| v.trim())
    })
}

fn yaml_f64(yaml: &str, key: &str) -> Result<f64, String> {
    yaml_value(yaml, key)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("map yaml: missing or bad {key}"))
}

/// Next whitespace-delimited PGM header token, skipping `#` comments.
fn pgm_token(b: &[u8], pos: &mut usize) -> Option<String> {
    loop {
        while *pos < b.len() && b[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        if *pos < b.len() && b[*pos] == b'#' {
            while *pos < b.len() && b[*pos] != b'\n' {
                *pos += 1;
            }
        } else {
            break;
        }
    }
    let start = *pos;
    while *pos < b.len() && !b[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
    (start < *pos).then(|| String::from_utf8_lossy(&b[start..*pos]).into_owned())
}

/// Load an occupancy grid. Trinary rule with `negate: 0`: `p = (255 - v) / 255`;
/// `p >= occupied_thresh` is occupied (100), `p <= free_thresh` is free (0), else
/// unknown (255). Rows are flipped so row 0 has the smallest y.
pub fn load_occupancy(yaml: &Path) -> Result<(Grid2DHeader, Vec<u8>), String> {
    let text = String::from_utf8(read(yaml)?).map_err(|_| format!("{}: not utf-8", yaml.display()))?;
    let image = yaml_value(&text, "image").ok_or("map yaml: missing image")?;
    let mode = yaml_value(&text, "mode").unwrap_or("trinary");
    if mode != "trinary" {
        return Err(format!("map yaml: unsupported mode {mode:?}"));
    }
    let resolution = yaml_f64(&text, "resolution")?;
    if resolution <= 0.0 {
        return Err("map yaml: resolution must be > 0".into());
    }
    let negate = yaml_f64(&text, "negate").unwrap_or(0.0) != 0.0;
    let occ = yaml_f64(&text, "occupied_thresh")?;
    let free = yaml_f64(&text, "free_thresh")?;
    let origin: Vec<f32> = yaml_value(&text, "origin")
        .map(|v| v.trim_matches(|c| c == '[' || c == ']'))
        .map(|v| v.split(',').filter_map(|s| s.trim().parse::<f32>().ok()).collect())
        .unwrap_or_default();
    if origin.len() != 3 || origin.iter().any(|v| !v.is_finite()) {
        return Err("map yaml: origin must be [x, y, yaw]".into());
    }

    let pgm_path = yaml.parent().unwrap_or_else(|| Path::new(".")).join(image);
    let b = read(&pgm_path)?;
    let mut pos = 0;
    if pgm_token(&b, &mut pos).as_deref() != Some("P5") {
        return Err(format!("{}: not a P5 pgm", pgm_path.display()));
    }
    let mut dim = |what: &str| {
        pgm_token(&b, &mut pos)
            .and_then(|t| t.parse::<usize>().ok())
            .ok_or_else(|| format!("{}: bad pgm {what}", pgm_path.display()))
    };
    let (w, h, maxval) = (dim("width")?, dim("height")?, dim("maxval")?);
    if w == 0 || h == 0 || maxval != 255 || w > u32::MAX as usize || h > u32::MAX as usize {
        return Err(format!("{}: unsupported pgm size/maxval", pgm_path.display()));
    }
    pos += 1; // single whitespace byte after maxval
    let cells = w.checked_mul(h).filter(|c| *c <= dviz_protocol::GRID_MAX_CELLS);
    let cells = cells.ok_or_else(|| format!("{}: grid too large", pgm_path.display()))?;
    let data = b.get(pos..).filter(|d| d.len() >= cells);
    let data = data.ok_or_else(|| format!("{}: pgm pixel data truncated", pgm_path.display()))?;

    let mut out = vec![GRID_UNKNOWN; cells];
    for row in 0..h {
        let dst = (h - 1 - row) * w;
        for col in 0..w {
            let v = f64::from(data[row * w + col]);
            let p = if negate { v / 255.0 } else { (255.0 - v) / 255.0 };
            out[dst + col] = if p >= occ {
                GRID_OCCUPIED
            } else if p <= free {
                GRID_FREE
            } else {
                GRID_UNKNOWN
            };
        }
    }
    let header = Grid2DHeader {
        width: w as u32,
        height: h as u32,
        resolution: resolution as f32,
        origin: [origin[0], origin[1], origin[2]],
        kind: GridKind::Occupancy,
    };
    Ok((header, out))
}

/// Load x, y, z from an ASCII or binary PCD (float32 fields); non-finite points are dropped.
pub fn load_pcd_xyz(path: &Path) -> Result<Vec<[f32; 3]>, String> {
    let b = read(path)?;
    let name = path.display();
    let (mut fields, mut sizes, mut types, mut counts) = (vec![], vec![], vec![], vec![]);
    let mut points = None;
    let mut pos = 0;
    let data_kind = loop {
        let end = b[pos..].iter().position(|c| *c == b'\n').ok_or(format!("{name}: no DATA line"))?;
        let line = String::from_utf8_lossy(&b[pos..pos + end]).trim().to_owned();
        pos += end + 1;
        let mut it = line.split_whitespace();
        match it.next() {
            Some("FIELDS") => fields = it.map(str::to_owned).collect::<Vec<_>>(),
            Some("SIZE") => sizes = it.filter_map(|s| s.parse::<usize>().ok()).collect::<Vec<_>>(),
            Some("TYPE") => types = it.map(str::to_owned).collect::<Vec<_>>(),
            Some("COUNT") => counts = it.filter_map(|s| s.parse::<usize>().ok()).collect::<Vec<_>>(),
            Some("POINTS") => points = it.next().and_then(|s| s.parse::<usize>().ok()),
            Some("DATA") => break it.next().unwrap_or("").to_owned(),
            _ => {}
        }
    };
    let n = points.ok_or(format!("{name}: missing POINTS"))?;
    if counts.is_empty() {
        counts = vec![1; fields.len()];
    }
    if fields.is_empty() || sizes.len() != fields.len() || types.len() != fields.len() || counts.len() != fields.len() {
        return Err(format!("{name}: inconsistent FIELDS/SIZE/TYPE/COUNT"));
    }
    let idx = |f: &str| fields.iter().position(|x| x == f).ok_or(format!("{name}: no field {f}"));
    let (ix, iy, iz) = (idx("x")?, idx("y")?, idx("z")?);
    for i in [ix, iy, iz] {
        if sizes[i] != 4 || types[i] != "F" || counts[i] != 1 {
            return Err(format!("{name}: x/y/z must be single float32"));
        }
    }
    let offsets: Vec<usize> = sizes
        .iter()
        .zip(&counts)
        .scan(0usize, |acc, (s, c)| {
            let o = *acc;
            *acc += s * c;
            Some(o)
        })
        .collect();
    let stride: usize = sizes.iter().zip(&counts).map(|(s, c)| s * c).sum();
    let body = &b[pos..];
    let mut out = Vec::new();
    match data_kind.as_str() {
        "binary" => {
            if stride == 0 || n.checked_mul(stride).is_none_or(|need| body.len() < need) {
                return Err(format!("{name}: binary data truncated"));
            }
            let f = |rec: &[u8], o: usize| f32::from_le_bytes([rec[o], rec[o + 1], rec[o + 2], rec[o + 3]]);
            for rec in body.chunks_exact(stride).take(n) {
                let p = [f(rec, offsets[ix]), f(rec, offsets[iy]), f(rec, offsets[iz])];
                if p.iter().all(|v| v.is_finite()) {
                    out.push(p);
                }
            }
        }
        "ascii" => {
            let text = std::str::from_utf8(body).map_err(|_| format!("{name}: ascii data not utf-8"))?;
            let width: usize = counts.iter().sum();
            let mut rows = 0;
            for line in text.lines().filter(|l| !l.trim().is_empty()).take(n) {
                let vals: Vec<f32> = line.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                if vals.len() < width {
                    return Err(format!("{name}: bad ascii row"));
                }
                rows += 1;
                let col = |i: usize| counts[..i].iter().sum::<usize>();
                let p = [vals[col(ix)], vals[col(iy)], vals[col(iz)]];
                if p.iter().all(|v| v.is_finite()) {
                    out.push(p);
                }
            }
            if rows < n {
                return Err(format!("{name}: ascii data has {rows} rows, expected {n}"));
            }
        }
        other => return Err(format!("{name}: unsupported DATA {other:?}")),
    }
    Ok(out)
}

/// Keep the first point of every `voxel`-sized cell; if that is still more than
/// `max_points`, take an evenly strided subset.
pub fn voxel_downsample(points: &[[f32; 3]], voxel: f32, max_points: usize) -> Vec<[f32; 3]> {
    let mut kept: Vec<[f32; 3]> = if voxel.is_finite() && voxel > 0.0 {
        let mut seen = HashSet::new();
        let key = |v: f32| (v / voxel).floor() as i64;
        points
            .iter()
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .filter(|p| seen.insert((key(p[0]), key(p[1]), key(p[2]))))
            .copied()
            .collect()
    } else {
        points.to_vec()
    };
    if kept.len() > max_points {
        let n = kept.len();
        kept = (0..max_points).map(|i| kept[i * n / max_points]).collect();
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("navbridge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const YAML: &str = "image: map.pgm\nmode: trinary\nresolution: 0.05\norigin: [-3.95, -6.25, 0.0]\nnegate: 0\noccupied_thresh: 0.65\nfree_thresh: 0.196\n";

    #[test]
    fn occupancy_from_pgm() {
        let d = tmpdir("pgm");
        // 3x2: top row 254 0 205, bottom row 205 205 254 (comment line in header)
        let mut pgm = b"P5\n# made by test\n3 2\n255\n".to_vec();
        pgm.extend_from_slice(&[254, 0, 205, 205, 205, 254]);
        std::fs::write(d.join("map.pgm"), pgm).unwrap();
        std::fs::write(d.join("map.yaml"), YAML).unwrap();
        let (h, cells) = load_occupancy(&d.join("map.yaml")).unwrap();
        assert_eq!((h.width, h.height), (3, 2));
        assert!((h.resolution - 0.05).abs() < 1e-9);
        assert_eq!(h.origin, [-3.95, -6.25, 0.0]);
        assert_eq!(h.kind, GridKind::Occupancy);
        // flipped: row 0 is the image's bottom row
        assert_eq!(cells, vec![255, 255, 0, 0, 100, 255]);
    }

    #[test]
    fn occupancy_rejects_bad_files() {
        let d = tmpdir("badpgm");
        std::fs::write(d.join("map.yaml"), YAML).unwrap();
        std::fs::write(d.join("map.pgm"), b"P5\n3 2\n255\n\x00\x01").unwrap();
        assert!(load_occupancy(&d.join("map.yaml")).is_err());
        std::fs::write(d.join("map.pgm"), b"P6\n1 1\n255\n\x00\x00\x00").unwrap();
        assert!(load_occupancy(&d.join("map.yaml")).is_err());
    }

    #[test]
    fn missing_map_is_not_fatal() {
        let p = std::env::temp_dir().join("navbridge-nonexistent/map.yaml");
        let e = load_occupancy(&p).unwrap_err();
        assert!(e.contains("navbridge-nonexistent"), "{e}");
        let e = load_pcd_xyz(&p).unwrap_err();
        assert!(e.contains("navbridge-nonexistent"), "{e}");
    }

    #[test]
    fn pcd_binary_and_ascii() {
        let d = tmpdir("pcd");
        let head = |data: &str, n: usize| {
            format!("# .PCD v0.7\nVERSION 0.7\nFIELDS x y z intensity\nSIZE 4 4 4 4\nTYPE F F F F\nCOUNT 1 1 1 1\nWIDTH {n}\nHEIGHT 1\nVIEWPOINT 0 0 0 1 0 0 0\nPOINTS {n}\nDATA {data}\n")
        };
        let mut bin = head("binary", 2).into_bytes();
        for v in [1.0f32, 2.0, 3.0, 9.0, 4.0, 5.0, 6.0, 9.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(d.join("b.pcd"), &bin).unwrap();
        assert_eq!(load_pcd_xyz(&d.join("b.pcd")).unwrap(), vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        std::fs::write(d.join("a.pcd"), format!("{}1 2 3 9\n4 5 6 9\n", head("ascii", 2))).unwrap();
        assert_eq!(load_pcd_xyz(&d.join("a.pcd")).unwrap().len(), 2);
        // truncated binary
        std::fs::write(d.join("t.pcd"), &bin[..bin.len() - 3]).unwrap();
        assert!(load_pcd_xyz(&d.join("t.pcd")).is_err());
    }

    #[test]
    fn voxel_caps_points() {
        let mut s = 12345u64;
        let mut rnd = move || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 33) as f32) / (1u64 << 31) as f32
        };
        let pts: Vec<[f32; 3]> = (0..100_000).map(|_| [rnd() * 20.0, rnd() * 20.0, rnd() * 5.0]).collect();
        let out = voxel_downsample(&pts, 0.1, 50_000);
        assert!(out.len() <= 50_000 && out.len() > 40_000, "{}", out.len());
        let mut seen = std::collections::HashSet::new();
        for p in &out {
            let k = ((p[0] / 0.1).floor() as i64, (p[1] / 0.1).floor() as i64, (p[2] / 0.1).floor() as i64);
            assert!(seen.insert(k), "two points in voxel {k:?}");
        }
        // dense cluster collapses to one per voxel
        let dense: Vec<[f32; 3]> = (0..1000).map(|i| [0.001 * (i % 10) as f32, 0.0, 0.0]).collect();
        assert_eq!(voxel_downsample(&dense, 0.1, 10).len(), 1);
    }
}
