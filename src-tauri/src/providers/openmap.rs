//! OpenMap (Vietnam) street imagery.
//!
//! Coverage is published as Mapbox Vector Tiles at zoom 18. A tile contains
//! every picture in the `pictures` layer; picking one is random, like the
//! reference implementation.

use super::{get_bytes, FetchResult, PanoRequest, Providers};
use crate::dates::{epoch_ms_to_iso, epoch_seconds_to_iso};
use crate::geo::{pixel_to_lat_lng, wgs84_to_tile_coord, LatLng};
use crate::models::{Link, Pano, TimeEntry};
use crate::protobuf::{parse_mvt, MvtFeature, MvtValue};
use rand::seq::SliceRandom;

const BASE_URL: &str = "https://gpx-tiles.streetview.vn";
const MIN_LAT: f64 = 8.18;
const MAX_LAT: f64 = 23.39;
const MIN_LNG: f64 = 102.14;
const MAX_LNG: f64 = 109.46;

#[derive(Debug, Clone, Default)]
pub struct OpenMapFeature {
    pub id: String,
    pub lat: f64,
    pub lng: f64,
    pub heading: f64,
    pub sequences: Vec<String>,
    pub time: Option<f64>,
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

    let (x, y) = wgs84_to_tile_coord(lat, lng, 18);
    let features = load_tile(providers, x, y).await;
    if features.is_empty() {
        return FetchResult::zero();
    }

    let mut rng = rand::thread_rng();
    let Some(feature) = features.choose(&mut rng) else {
        return FetchResult::zero();
    };
    let position = features.iter().position(|f| f.id == feature.id);
    let mut links = Vec::new();
    if let Some(index) = position {
        let candidates = [
            index.checked_sub(1).and_then(|i| features.get(i)),
            features.get(index + 1),
        ];
        for candidate in candidates.into_iter().flatten() {
            if candidate
                .sequences
                .iter()
                .any(|seq| feature.sequences.contains(seq))
            {
                links.push(Link {
                    pano: candidate.id.clone(),
                    heading: candidate.heading,
                    description: "OpenMap".to_string(),
                });
            }
        }
    }
    links.sort_by(|a, b| a.pano.cmp(&b.pano));
    links.dedup_by(|a, b| a.pano == b.pano);

    FetchResult::ok(build_pano(feature, links))
}

fn find_cached(providers: &Providers, id: &str) -> Option<OpenMapFeature> {
    let tiles = providers.openmap_tiles.lock().ok()?;
    for features in tiles.values() {
        if let Some(feature) = features.iter().find(|f| f.id == id) {
            return Some(feature.clone());
        }
    }
    None
}

async fn load_tile(providers: &Providers, x: u32, y: u32) -> Vec<OpenMapFeature> {
    if let Ok(tiles) = providers.openmap_tiles.lock() {
        if let Some(cached) = tiles.get(&(x, y)) {
            return cached.clone();
        }
    }

    let url = format!("{BASE_URL}/18/{x}/{y}.mvt");
    let mut features = Vec::new();
    if let Some(bytes) = get_bytes(providers, &url, &[]).await {
        for (name, extent, mvt_features) in parse_mvt(&bytes) {
            if name != "pictures" {
                continue;
            }
            for (index, feature) in mvt_features.iter().enumerate() {
                if let Some(parsed) = parse_feature(feature, extent, x, y, index) {
                    features.push(parsed);
                }
            }
        }
    }

    if let Ok(mut tiles) = providers.openmap_tiles.lock() {
        if tiles.len() > 500 {
            tiles.clear();
        }
        tiles.insert((x, y), features.clone());
    }
    features
}

fn parse_feature(
    feature: &MvtFeature,
    extent: i32,
    x: u32,
    y: u32,
    index: usize,
) -> Option<OpenMapFeature> {
    let (gx, gy) = *feature.geometry.first()?;
    let coords: LatLng = pixel_to_lat_lng(
        gx as f64,
        gy as f64,
        18,
        x,
        y,
        if extent > 0 { extent as f64 } else { 4096.0 },
    );

    let id = feature
        .properties
        .get("id")
        .and_then(|v| match v {
            MvtValue::String(s) => Some(s.clone()),
            other => other.as_f64().map(|n| n.to_string()),
        })
        .or_else(|| feature.id.map(|id| id.to_string()))
        .unwrap_or_else(|| format!("{x}_{y}_{index}"));

    let sequences = feature
        .properties
        .get("sequences")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();

    let heading = feature
        .properties
        .get("heading")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let time = feature.properties.get("ts").and_then(|v| v.as_f64());

    Some(OpenMapFeature {
        id,
        lat: coords.lat,
        lng: coords.lng,
        heading,
        sequences,
        time,
    })
}

fn build_pano(feature: &OpenMapFeature, links: Vec<Link>) -> Pano {
    let date = feature.time.and_then(timestamp_to_iso);
    Pano {
        pano_id: feature.id.clone(),
        lat: feature.lat,
        lng: feature.lng,
        country: Some("VN".to_string()),
        region: None,
        locality: None,
        road: None,
        description: Some("OpenMap".to_string()),
        short_description: None,
        image_date: date.clone(),
        copyright: Some("© OpenMap".to_string()),
        service: None,
        altitude: None,
        world_width: 8192,
        world_height: 4096,
        center_heading: feature.heading,
        links,
        time: date
            .map(|date| {
                vec![TimeEntry {
                    pano: feature.id.clone(),
                    date,
                }]
            })
            .unwrap_or_default(),
        provider: "openmap".to_string(),
    }
}

fn timestamp_to_iso(ts: f64) -> Option<String> {
    if !ts.is_finite() || ts <= 0.0 {
        return None;
    }
    // OpenMap has used both seconds and milliseconds over time; anything past
    // the year 5138 must be milliseconds.
    Some(if ts >= 1e11 {
        epoch_ms_to_iso(ts as i64)
    } else {
        epoch_seconds_to_iso(ts)
    })
}
