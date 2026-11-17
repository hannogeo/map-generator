//! Tile based checks.
//!
//! Two features rely on raster map tiles:
//! * blue-line coverage checks, which read the alpha channel of Google's
//!   Street View overlay (`svv`) tiles;
//! * tile colour filters, which count exact RGB matches on a map tile.
//!
//! Both are implemented against decoded pixels instead of a browser canvas.

use crate::geo::{lat_lng_to_pixel, LatLng};
use crate::models::TileColorConfig;
use crate::providers::{get_bytes, Providers};
use image::imageops::FilterType;
use image::RgbaImage;
use std::collections::HashMap;
use std::sync::Arc;

const TILE_SIZE: i64 = 256;
const MASK_MAX_TILES: i64 = 5;
const MAX_CACHED_TILES: usize = 256;

fn thin_url(x: i64, y: i64, z: u32) -> String {
    format!(
        "https://maps.googleapis.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m8!1e2!2ssvv!4m2!1scc!2s*211m3*211e2*212b1*213e2*212b1*214b1!4m2!1ssvl!2s*211b0*212b1!3m8!2sen!3sus!5e1105!12m4!1e68!2m2!1sset!2sRoadmap!4e0!5m4!1e0!8m2!1e1!1e1!6m6!1e12!2i2!11e0!39b0!44e0!50e"
    )
}

fn blue_line_pixel_url(x: i64, y: i64, z: u32) -> String {
    format!(
        "https://www.google.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m8!1e2!2ssvv!4m2!1scc!2s*211m3*211e2*212b1*213e2*211m3*211e3*212b1*213e2*212b1*214b1!4m2!1ssvl!2s*211b0*212b0!3m8!2sen!3sus!5e1105!12m4!1e68!2m2!1sset!2sRoadmap!4e0!5m4!1e0!8m2!1e1!1e1!6m6!1e12!2i2!11e0!39b0!44e0!50e0"
    )
}

fn color_tile_url(provider: &str, z: u32, x: i64, y: i64) -> String {
    if provider == "osm" {
        format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png")
    } else {
        format!(
            "https://www.google.com/maps/vt?pb=!1m7!8m6!1m3!1i{z}!2i{x}!3i{y}!2i9!3x1!2m2!1e0!2sm!3m5!2sen!3sus!5e1105!12m1!1e3!4e0!5m4!1e0!8m2!1e1!1e1!6m6!1e12!2i2!11e0!39b0!44e0!50e0"
        )
    }
}

async fn decode_rgba(bytes: &[u8]) -> Option<RgbaImage> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(image.to_rgba8())
}

/// Alpha channel of one overlay tile, cached per variant/zoom/tile.
async fn overlay_tile(
    providers: &Providers,
    variant: u8,
    z: u32,
    x: i64,
    y: i64,
    url: String,
) -> Option<Arc<Vec<u8>>> {
    let span = 1i64 << z;
    let key = (
        variant,
        z as u8,
        x.rem_euclid(span) as u32,
        y.rem_euclid(span) as u32,
    );
    if let Ok(cache) = providers.line_tiles.lock() {
        if let Some(tile) = cache.get(&key) {
            return Some(tile.clone());
        }
    }

    let bytes = get_bytes(providers, &url, &[]).await?;
    let image = decode_rgba(&bytes).await?;

    let mut alpha = Vec::with_capacity((TILE_SIZE * TILE_SIZE) as usize);
    if image.width() == TILE_SIZE as u32 && image.height() == TILE_SIZE as u32 {
        alpha.extend(image.pixels().map(|pixel| pixel[3]));
    } else {
        let resized =
            image::imageops::resize(&image, TILE_SIZE as u32, TILE_SIZE as u32, FilterType::Triangle);
        alpha.extend(resized.pixels().map(|pixel| pixel[3]));
    }

    let tile = Arc::new(alpha);
    if let Ok(mut cache) = providers.line_tiles.lock() {
        if cache.len() >= MAX_CACHED_TILES {
            cache.clear();
        }
        cache.insert(key, tile.clone());
    }
    Some(tile)
}

/// The single pixel check used after a location was found. At zoom 12 one
/// pixel covers roughly 38 m, which keeps panoramas beside a road inside the
/// blue line.
pub async fn has_blue_line_at(providers: &Providers, lat: f64, lng: f64) -> bool {
    const ZOOM: u32 = 12;
    let (pixel_x, pixel_y) = lat_lng_to_pixel(lat, lng, ZOOM);
    let pixel_x = pixel_x.floor() as i64;
    let pixel_y = pixel_y.floor() as i64;
    let tile_x = pixel_x.div_euclid(TILE_SIZE);
    let tile_y = pixel_y.div_euclid(TILE_SIZE);
    let span = 1i64 << ZOOM;
    let url = blue_line_pixel_url(
        tile_x.rem_euclid(span),
        tile_y.rem_euclid(span),
        ZOOM,
    );
    let Some(tile) = overlay_tile(providers, 1, ZOOM, tile_x, tile_y, url).await else {
        return false;
    };
    let ix = pixel_x.rem_euclid(TILE_SIZE) as usize;
    let iy = pixel_y.rem_euclid(TILE_SIZE) as usize;
    tile.get(iy * TILE_SIZE as usize + ix).copied().unwrap_or(0) > 0
}

fn lat_lng_to_tile(lat: f64, lng: f64, zoom: u32) -> (i64, i64) {
    let (x, y) = lat_lng_to_pixel(lat, lng, zoom);
    (
        (x / TILE_SIZE as f64).floor() as i64,
        (y / TILE_SIZE as f64).floor() as i64,
    )
}

/// Checks a rectangle around a point instead of a single pixel, so that
/// panoramas slightly off the road are still accepted.
pub struct BlueLineMask {
    zoom: u32,
    tile_x: i64,
    tile_y: i64,
    cols: usize,
    rows: usize,
    alpha: Vec<u8>,
}

impl BlueLineMask {
    pub async fn build(providers: &Providers, north_west: LatLng, south_east: LatLng) -> Self {
        let (zoom, cols, rows, tile_x, tile_y) = calculate_zoom(north_west, south_east);
        let width = cols * TILE_SIZE as usize;
        let height = rows * TILE_SIZE as usize;
        let mut alpha = vec![0u8; width * height];

        let mut tasks = Vec::with_capacity(cols * rows);
        let span = 1i64 << zoom;
        for row in 0..rows {
            for col in 0..cols {
                let x = tile_x + col as i64;
                let y = tile_y + row as i64;
                let url = thin_url(x.rem_euclid(span), y.rem_euclid(span), zoom);
                tasks.push(async move {
                    (
                        col,
                        row,
                        overlay_tile(providers, 0, zoom, x, y, url).await,
                    )
                });
            }
        }

        for (col, row, tile) in futures::future::join_all(tasks).await {
            let Some(tile) = tile else { continue };
            for y in 0..TILE_SIZE as usize {
                let source = y * TILE_SIZE as usize;
                let target = (row * TILE_SIZE as usize + y) * width + col * TILE_SIZE as usize;
                alpha[target..target + TILE_SIZE as usize]
                    .copy_from_slice(&tile[source..source + TILE_SIZE as usize]);
            }
        }

        Self {
            zoom,
            tile_x,
            tile_y,
            cols,
            rows,
            alpha,
        }
    }

    pub fn contains(&self, lat: f64, lng: f64, radius: f64) -> bool {
        let (pixel_x, pixel_y) = lat_lng_to_pixel(lat, lng, self.zoom);
        let (tile_x, tile_y) = lat_lng_to_tile(lat, lng, self.zoom);
        let point_x = (tile_x - self.tile_x) * TILE_SIZE + pixel_x.floor() as i64;
        let point_y = (tile_y - self.tile_y) * TILE_SIZE + pixel_y.floor() as i64;

        let west = destination(lat, lng, 270.0, radius);
        let north = destination(lat, lng, 180.0, radius);
        let (Some(west), Some(north)) = (west, north) else {
            return false;
        };
        let (west_x, _) = lat_lng_to_pixel(west.lat, west.lng, self.zoom);
        let (_, north_y) = lat_lng_to_pixel(north.lat, north.lng, self.zoom);
        let (west_tile_x, _) = lat_lng_to_tile(west.lat, west.lng, self.zoom);
        let (_, north_tile_y) = lat_lng_to_tile(north.lat, north.lng, self.zoom);
        let point_nw_x = (west_tile_x - self.tile_x) * TILE_SIZE + west_x.floor() as i64;
        let point_nw_y = (north_tile_y - self.tile_y) * TILE_SIZE + north_y.floor() as i64;

        let dx = (point_x - point_nw_x).abs();
        let dy = (point_y - point_nw_y).abs();
        let width = self.cols as i64 * TILE_SIZE;
        let height = self.rows as i64 * TILE_SIZE;

        for py in (point_y - dy)..=(point_y + dy) {
            if py < 0 || py >= height {
                continue;
            }
            for px in (point_x - dx)..=(point_x + dx) {
                if px < 0 || px >= width {
                    continue;
                }
                if self.alpha[(py * width + px) as usize] > 0 {
                    return true;
                }
            }
        }
        false
    }
}

fn calculate_zoom(north_west: LatLng, south_east: LatLng) -> (u32, usize, usize, i64, i64) {
    let mut zoom = 16;
    loop {
        if zoom == 0 {
            break;
        }
        zoom -= 1;
        let span = 1i64 << zoom;
        let (nw_x, nw_y) = lat_lng_to_tile(north_west.lat, north_west.lng, zoom);
        let (se_x, se_y) = lat_lng_to_tile(south_east.lat, south_east.lng, zoom);
        let mut cols = 1 + se_x - nw_x;
        let mut rows = 1 + se_y - nw_y;
        if cols < 0 {
            cols = 1 + (span - nw_x) + se_x;
        }
        if rows < 0 {
            rows = 1 + (span - nw_y) + se_y;
        }
        if cols <= MASK_MAX_TILES && rows <= MASK_MAX_TILES {
            return (zoom, cols.max(1) as usize, rows.max(1) as usize, nw_x, nw_y);
        }
    }
    let (x, y) = lat_lng_to_tile(north_west.lat, north_west.lng, 0);
    (0, 1, 1, x, y)
}

fn destination(lat: f64, lng: f64, bearing: f64, distance: f64) -> Option<LatLng> {
    let distance = distance / 6_371_000.0;
    let bearing = bearing.to_radians();
    let lat1 = lat.to_radians();
    let lng1 = lng.to_radians();
    let sin_lat1 = lat1.sin();
    let cos_lat1 = lat1.cos();
    let sin_distance = distance.sin();
    let cos_distance = distance.cos();
    let lat2 = (sin_lat1 * cos_distance + cos_lat1 * sin_distance * bearing.cos()).asin();
    let lng2 = lng1
        + (bearing.sin() * sin_distance * cos_lat1)
            .atan2(cos_distance - sin_lat1 * lat2.sin());
    if lat2.is_nan() || lng2.is_nan() {
        return None;
    }
    Some(LatLng::new(lat2.to_degrees(), lng2.to_degrees()))
}

/// Counts exact RGB matches on a map tile and applies the configured groups.
pub async fn tile_color_matches(
    providers: &Providers,
    loc: LatLng,
    config: &TileColorConfig,
) -> bool {
    let Some(groups) = config.tile_colors.get(&config.tile_provider) else {
        return false;
    };
    let groups: Vec<&crate::models::TileColorGroup> =
        groups.iter().filter(|group| group.active).collect();

    let zoom = config.zoom as u32;
    let (tile_x, tile_y) = lat_lng_to_tile(loc.lat, loc.lng, zoom);
    let url = color_tile_url(&config.tile_provider, zoom, tile_x, tile_y);
    let Some(bytes) = get_bytes(providers, &url, &[]).await else {
        return false;
    };
    let Some(image) = decode_rgba(&bytes).await else {
        return false;
    };
    let image = fit_tile(&image);

    let mut color_groups: HashMap<u32, Vec<usize>> = HashMap::new();
    for (index, group) in groups.iter().enumerate() {
        for color in &group.colors {
            if let Some(key) = parse_rgb(color) {
                color_groups.entry(key).or_default().push(index);
            }
        }
    }

    let mut counts = vec![0u64; groups.len()];
    for pixel in image.pixels() {
        let key = ((pixel[0] as u32) << 16) | ((pixel[1] as u32) << 8) | pixel[2] as u32;
        if let Some(indices) = color_groups.get(&key) {
            for index in indices {
                counts[*index] += 1;
            }
        }
    }

    let pixel_count = 256.0 * 256.0;
    let results: Vec<bool> = groups
        .iter()
        .zip(counts)
        .map(|(group, count)| {
            if (group.threshold - 0.01).abs() < f64::EPSILON {
                count > 50
            } else {
                count as f64 / pixel_count >= group.threshold
            }
        })
        .collect();

    let matched = if config.operator.eq_ignore_ascii_case("AND") {
        !results.is_empty() && results.iter().all(|v| *v)
    } else {
        results.iter().any(|v| *v)
    };

    if config.filter_type.eq_ignore_ascii_case("exclude") {
        !matched
    } else {
        matched
    }
}

fn fit_tile(image: &RgbaImage) -> RgbaImage {
    if image.width() == TILE_SIZE as u32 && image.height() == TILE_SIZE as u32 {
        image.clone()
    } else {
        image::imageops::resize(image, TILE_SIZE as u32, TILE_SIZE as u32, FilterType::Nearest)
    }
}

fn parse_rgb(color: &str) -> Option<u32> {
    let mut parts = color.split(',').map(|value| value.trim().parse::<u32>());
    let r = parts.next()?.ok()?;
    let g = parts.next()?.ok()?;
    let b = parts.next()?.ok()?;
    if r > 255 || g > 255 || b > 255 {
        return None;
    }
    Some((r << 16) | (g << 8) | b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgb_colors() {
        assert_eq!(parse_rgb("255,0,128"), Some((255 << 16) | 128));
        assert_eq!(parse_rgb(" 1, 2, 3 "), Some((1 << 16) | (2 << 8) | 3));
        assert_eq!(parse_rgb("300,0,0"), None);
        assert_eq!(parse_rgb("nope"), None);
    }

    #[test]
    fn destination_moves_west() {
        let point = destination(48.0, 2.0, 270.0, 1000.0).unwrap();
        assert!(point.lng < 2.0);
        assert!((point.lat - 48.0).abs() < 0.001);
    }
}
