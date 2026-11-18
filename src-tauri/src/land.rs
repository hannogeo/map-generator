//! Bundled land mask.
//!
//! Country polygons follow administrative borders, so a uniform random sample
//! inside them lands in the sea surprisingly often (the Philippines are ~80%
//! water, Greece ~66%). A sea sample can never produce a panorama, so we drop
//! it before spending a request on it.
//!
//! The mask is generated from Natural Earth 1:50m land polygons by
//! `scripts/build-land-mask.mjs` and embedded in the binary (about 140 KiB).
//! Cells are 0.05°, stored as inclusive column spans per row, rows running
//! south to north.

use std::sync::OnceLock;

const MAGIC: [u8; 4] = *b"MGL1";
const DATA: &[u8] = include_bytes!("../assets/land_mask.bin");

/// Mask cells are 0.05°, so the 3×3 neighbourhood spans roughly 8 km. Coastal
/// street view coverage often sits just outside the coastline cell.
const MARGIN_CELLS: i32 = 1;

struct Mask {
    cols: i32,
    rows: i32,
    step: f64,
    /// Start offset of each row inside `spans`.
    row_start: Vec<u32>,
    /// Inclusive column spans, grouped by row.
    spans: Vec<(u16, u16)>,
}

impl Mask {
    fn parse() -> Option<Mask> {
        if DATA.len() < 12 || DATA[0..4] != MAGIC {
            return None;
        }
        let cols = u16::from_le_bytes([DATA[4], DATA[5]]) as i32;
        let rows = u16::from_le_bytes([DATA[6], DATA[7]]) as i32;
        let step = f32::from_le_bytes([DATA[8], DATA[9], DATA[10], DATA[11]]) as f64;
        if cols <= 0 || rows <= 0 || !(step > 0.0) {
            return None;
        }

        let mut offset = 12usize;
        let mut row_start = Vec::with_capacity(rows as usize);
        let mut spans: Vec<(u16, u16)> = Vec::new();
        for _ in 0..rows {
            let count = u16::from_le_bytes([*DATA.get(offset)?, *DATA.get(offset + 1)?]) as usize;
            offset += 2;
            row_start.push(spans.len() as u32);
            for _ in 0..count {
                let start = u16::from_le_bytes([*DATA.get(offset)?, *DATA.get(offset + 1)?]);
                let end = u16::from_le_bytes([*DATA.get(offset + 2)?, *DATA.get(offset + 3)?]);
                spans.push((start, end));
                offset += 4;
            }
        }
        row_start.push(spans.len() as u32);

        Some(Mask {
            cols,
            rows,
            step,
            row_start,
            spans,
        })
    }

    fn row_has_land(&self, row: i32, col: i32) -> bool {
        if row < 0 || row >= self.rows || col < 0 || col >= self.cols {
            return false;
        }
        let start = self.row_start[row as usize] as usize;
        let end = self.row_start[row as usize + 1] as usize;
        let spans = &self.spans[start..end];
        if spans.is_empty() {
            return false;
        }
        // First span whose end column is at or past `col`.
        match spans.binary_search_by(|span| (span.1 as i32).cmp(&col)) {
            Ok(_) => true,
            Err(index) => index < spans.len() && (spans[index].0 as i32) <= col,
        }
    }
}

fn mask() -> Option<&'static Mask> {
    static MASK: OnceLock<Option<Mask>> = OnceLock::new();
    MASK.get_or_init(Mask::parse).as_ref()
}

fn cell(lat: f64, lng: f64, step: f64) -> (i32, i32) {
    let lng = (lng + 180.0).rem_euclid(360.0) - 180.0;
    let row = ((lat + 90.0) / step).floor() as i32;
    let col = ((lng + 180.0) / step).floor() as i32;
    (row, col)
}

/// True when the coordinate is on land, or within one mask cell of land.
///
/// The soft margin keeps coastal cities whose street view coverage reaches a
/// little beyond the coastline. If the mesh data is missing the function is
/// permissive so generation still works.
pub fn is_land_near(lat: f64, lng: f64) -> bool {
    let Some(mask) = mask() else {
        return true;
    };
    let (row, col) = cell(lat, lng, mask.step);
    for dr in -MARGIN_CELLS..=MARGIN_CELLS {
        for dc in -MARGIN_CELLS..=MARGIN_CELLS {
            if mask.row_has_land(row + dr, col + dc) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_is_embedded_and_parses() {
        let mask = mask().expect("land mask must parse");
        assert!(mask.cols > 1000);
        assert!(mask.rows > 1000);
    }

    #[test]
    fn land_and_sea_are_told_apart() {
        assert!(is_land_near(48.8584, 2.2945), "Paris");
        assert!(is_land_near(40.7128, -74.0060), "New York");
        assert!(is_land_near(35.6595, 139.7005), "Tokyo");
        assert!(is_land_near(-33.8688, 151.2093), "Sydney");
        assert!(!is_land_near(0.0, -30.0), "mid Atlantic");
        assert!(!is_land_near(43.0, 5.0), "Mediterranean");
    }

    #[test]
    fn coast_is_lenient() {
        assert!(is_land_near(59.3293, 18.0686), "Stockholm");
        assert!(is_land_near(64.1466, -21.9426), "Reykjavik");
        assert!(is_land_near(25.2048, 55.2708), "Dubai");
    }
}
