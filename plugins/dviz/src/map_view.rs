//! Occupancy grid to picture conversion, shared by the Rerun sink and the 2D
//! map view. Compiled in every build.

use dviz_protocol::{Grid2DHeader, GridKind, GRID_FREE, GRID_OCCUPIED};

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
}
