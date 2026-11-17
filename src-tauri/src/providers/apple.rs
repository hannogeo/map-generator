//! Apple Look Around coverage metadata.
//!
//! Apple publishes a protobuf "GroundMetadataTile" per map tile containing
//! every panorama position. There is no search endpoint, so the closest pano in
//! the tile of the requested point is used, exactly like the reference.

use super::{
    get_bytes, FetchResult, PanoRequest, Providers,
};
use crate::dates::epoch_ms_to_iso;
use crate::geo::{distance_between, tile_coord_to_wgs84, wgs84_to_tile_coord, LatLng};
use crate::models::{Pano, TimeEntry};
use crate::protobuf::{apple_camera_type, apple_heading, parse_apple_tile};

const TILE_STYLE: &str = "style=57&size=2&scale=0&v=0&preflight=2";
const AUTH_TOKEN: &str = "w31CPGRO/n7BsFPh8X7kZnFG0LDj9pAuR8nTtH3xhH8=";

#[derive(Debug, Clone)]
pub struct AppleLocation {
    pub pano: Pano,
}

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    if let Some(id) = req.pano.as_deref() {
        if let Some(pano) = providers.lookup_cached("apple", id) {
            return FetchResult::ok(pano);
        }
        return FetchResult::zero();
    }
    let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
        return FetchResult::error();
    };
    let (x, y) = wgs84_to_tile_coord(lat, lng, 17);
    let locations = get_tile(providers, x, y).await;
    let mut closest: Option<&AppleLocation> = None;
    let mut best = f64::INFINITY;
    for location in &locations {
        let distance = distance_between(
            LatLng::new(lat, lng),
            LatLng::new(location.pano.lat, location.pano.lng),
        );
        if distance < best {
            best = distance;
            closest = Some(location);
        }
    }
    match closest {
        Some(location) => FetchResult::ok(location.pano.clone()),
        None => FetchResult::zero(),
    }
}

async fn get_tile(providers: &Providers, x: u32, y: u32) -> Vec<AppleLocation> {
    if let Ok(tiles) = providers.apple_tiles.lock() {
        if let Some(cached) = tiles.get(&(x, y)) {
            return cached.clone();
        }
    }

    let x_str = x.to_string();
    let y_str = y.to_string();
    let headers = [
        ("Cache-Control", "no-cache"),
        ("maps-tile-style", TILE_STYLE),
        ("maps-tile-x", x_str.as_str()),
        ("maps-tile-y", y_str.as_str()),
        ("maps-tile-z", "17"),
        ("maps-auth-token", AUTH_TOKEN),
    ];

    let mut locations = Vec::new();
    if let Some(bytes) = get_bytes(
        providers,
        "https://gspe76-ssl.ls.apple.com/api/tile?",
        &headers,
    )
    .await
    {
        let tile = parse_apple_tile(&bytes);
        for pano in &tile.panos {
            let pano_x = x as f64 + (pano.x as f64 / 64.0) / 255.0;
            let pano_y = y as f64 + (255.0 - (pano.y as f64 / 64.0)) / 255.0;
            let coords = tile_coord_to_wgs84(pano_x, pano_y, 17);
            let coverage = pano
                .build_idx
                .and_then(|i| tile.coverage_types.get(i).copied())
                .unwrap_or(0);
            let camera_type = pano
                .camera_idx
                .first()
                .and_then(|i| tile.cameras.get(*i))
                .map(apple_camera_type)
                .unwrap_or("unknown");
            let service = if coverage == 3 {
                "backpack".to_string()
            } else {
                camera_type.to_string()
            };
            let heading = apple_heading(pano.pitch as f64, pano.roll as f64);
            let date = epoch_ms_to_iso(pano.timestamp_ms);
            let id = pano.pano_id.to_string();
            let pano = Pano {
                pano_id: id.clone(),
                lat: coords.lat,
                lng: coords.lng,
                country: None,
                region: None,
                locality: None,
                road: None,
                description: Some("© Apple Look Around".to_string()),
                short_description: None,
                image_date: if date.is_empty() { None } else { Some(date.clone()) },
                copyright: Some("© Apple Look Around".to_string()),
                service: Some(service),
                altitude: None,
                world_width: 16384,
                world_height: 8192,
                center_heading: heading,
                links: Vec::new(),
                time: vec![TimeEntry {
                    pano: id,
                    date,
                }],
                provider: "apple".to_string(),
            };
            providers.cache_pano(pano.clone());
            locations.push(AppleLocation { pano });
        }
    }

    if let Ok(mut tiles) = providers.apple_tiles.lock() {
        if tiles.len() > 4000 {
            tiles.clear();
        }
        tiles.insert((x, y), locations.clone());
    }
    locations
}
