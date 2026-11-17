//! ASIG (Albanian Street View).
//!
//! Coverage is a static GeoJSON tile pyramid. Generation only ever asks for the
//! zoom 15 tile that contains the point, which is one HTTP request per tile.

use super::{as_f64, as_str, field, get_json, idx, FetchResult, PanoRequest, Providers};
use crate::geo::wgs84_to_tile_coord;
use crate::models::{Link, Pano, TimeEntry};
use rand::seq::SliceRandom;
use std::collections::HashSet;

const TILE_URL: &str =
    "https://360.asig.gov.al/AlbaniaStreetView/player2/tiles-1674737600";
const ZOOM: u32 = 15;
const MIN_LAT: f64 = 39.6;
const MAX_LAT: f64 = 42.7;
const MIN_LNG: f64 = 19.2;
const MAX_LNG: f64 = 21.1;

#[derive(Debug, Clone, Default)]
pub struct AsigFeature {
    pub id: String,
    pub lat: f64,
    pub lng: f64,
    pub heading: f64,
    pub height: Option<f64>,
    pub date: Option<String>,
}

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    if let Some(id) = req.pano.as_deref() {
        return match find_cached(providers, id) {
            Some(feature) => FetchResult::ok(build_pano(&feature, Vec::new())),
            None => FetchResult::zero(),
        };
    }

    let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
        return FetchResult::error();
    };
    if !(MIN_LAT..=MAX_LAT).contains(&lat) || !(MIN_LNG..=MAX_LNG).contains(&lng) {
        return FetchResult::zero();
    }

    let (x, y) = wgs84_to_tile_coord(lat, lng, ZOOM);
    let features = load_tile(providers, x, y).await;
    let points: Vec<&AsigFeature> = features.iter().filter(|f| f.lat != 0.0 || f.lng != 0.0).collect();
    if points.is_empty() {
        return FetchResult::zero();
    }

    let mut rng = rand::thread_rng();
    let Some(feature) = points.choose(&mut rng).copied() else {
        return FetchResult::zero();
    };

    let mut links = Vec::new();
    if let (Some(prefix), Some((digits, digits_start))) = (
        camera_prefix(&feature.id),
        trailing_number(&feature.id),
    ) {
        let width = feature.id.len() - digits_start;
        if let Ok(number) = digits.parse::<u64>() {
            let prev_id = number
                .checked_sub(1)
                .map(|n| format!("{prefix}-{:0width$}", n, width = width));
            let next_id = number
                .checked_add(1)
                .map(|n| format!("{prefix}-{:0width$}", n, width = width));
            for candidate in &points {
                if Some(&candidate.id) == prev_id.as_ref() {
                    links.push(Link {
                        pano: candidate.id.clone(),
                        heading: (feature.heading + 180.0) % 360.0,
                        description: "AIGS".to_string(),
                    });
                } else if Some(&candidate.id) == next_id.as_ref() {
                    links.push(Link {
                        pano: candidate.id.clone(),
                        heading: feature.heading,
                        description: "AIGS".to_string(),
                    });
                }
            }
        }
    }

    FetchResult::ok(build_pano(feature, links))
}

fn find_cached(providers: &Providers, id: &str) -> Option<AsigFeature> {
    let tiles = providers.asig_tiles.lock().ok()?;
    for features in tiles.values() {
        if let Some(feature) = features.iter().find(|f| f.id == id) {
            return Some(feature.clone());
        }
    }
    None
}

async fn load_tile(providers: &Providers, x: u32, y: u32) -> Vec<AsigFeature> {
    if let Ok(tiles) = providers.asig_tiles.lock() {
        if let Some(cached) = tiles.get(&(x, y)) {
            return cached.clone();
        }
    }

    let direct = format!("{TILE_URL}/{ZOOM}/{x}/{y}.geojson");
    let proxied = format!("https://cors-proxy.ac4.stocc.dev/{direct}");
    let json = match get_json(providers, &direct).await {
        Some(value) => Some(value),
        None => get_json(providers, &proxied).await,
    };

    let mut features = Vec::new();
    let mut seen = HashSet::new();
    if let Some(json) = json {
        if let Some(list) = field(&json, "features").as_array() {
            for item in list {
                let geometry = field(item, "geometry");
                if as_str(field(geometry, "type")) != Some("Point") {
                    continue;
                }
                let props = field(item, "properties");
                let Some(id) = as_str(field(props, "id")) else {
                    continue;
                };
                if !seen.insert(id.to_string()) {
                    continue;
                }
                let coords = field(geometry, "coordinates");
                let lng = as_f64(idx(coords, 0))
                    .or_else(|| as_f64(field(props, "lon")))
                    .unwrap_or(0.0);
                let lat = as_f64(idx(coords, 1))
                    .or_else(|| as_f64(field(props, "lat")))
                    .unwrap_or(0.0);
                features.push(AsigFeature {
                    id: id.to_string(),
                    lat,
                    lng,
                    heading: as_f64(field(props, "heading")).unwrap_or(0.0),
                    height: as_f64(field(props, "height")),
                    date: super::as_string(field(props, "date")),
                });
            }
        }
    }

    if let Ok(mut tiles) = providers.asig_tiles.lock() {
        if tiles.len() > 500 {
            tiles.clear();
        }
        tiles.insert((x, y), features.clone());
    }
    features
}

fn build_pano(feature: &AsigFeature, links: Vec<Link>) -> Pano {
    Pano {
        pano_id: feature.id.clone(),
        lat: feature.lat,
        lng: feature.lng,
        country: Some("AL".to_string()),
        region: None,
        locality: None,
        road: None,
        description: Some("AIGS".to_string()),
        short_description: None,
        image_date: feature.date.clone(),
        copyright: Some("© AIGS".to_string()),
        service: None,
        altitude: feature.height,
        world_width: 2048,
        world_height: 1024,
        center_heading: feature.heading,
        links,
        time: feature
            .date
            .clone()
            .map(|date| {
                vec![TimeEntry {
                    pano: feature.id.clone(),
                    date,
                }]
            })
            .unwrap_or_default(),
        provider: "asig".to_string(),
    }
}

/// `camera-<digits>-<digits>` at the start of an ASIG id.
fn camera_prefix(id: &str) -> Option<&str> {
    let start = id.find("camera-")?;
    let rest = &id[start + "camera-".len()..];
    let first = rest.find(|c: char| !c.is_ascii_digit())?;
    if first == 0 || rest.as_bytes().get(first) != Some(&b'-') {
        return None;
    }
    let after = &rest[first + 1..];
    let second = after.find(|c: char| !c.is_ascii_digit())?;
    if second == 0 {
        return None;
    }
    Some(&id[..start + "camera-".len() + first + 1 + second])
}

/// Trailing run of at least nine digits, as used for the sequence number.
fn trailing_number(id: &str) -> Option<(&str, usize)> {
    let bytes = id.as_bytes();
    let mut start = bytes.len();
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if bytes.len() - start < 9 {
        return None;
    }
    Some((&id[start..], start))
}
