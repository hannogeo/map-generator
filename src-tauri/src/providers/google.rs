//! Google Street View via the internal Maps JS RPC endpoints.
//!
//! These are the same keyless endpoints the browser reference uses. `google`
//! uses a spatial search (`SingleImageSearch`), which returns the complete
//! panorama record. `googleZoom` instead lists the panoramas of a z17
//! `photometa` tile and resolves one of them through `GetMetadata`, matching
//! the reference project's `getFromGoogleZoom`. Parsing follows the reference
//! `parseGoogle` implementation field by field.

use super::{
    as_f64, as_str, get_text, idx, month_string, post_json, FetchResult, PanoRequest, Providers,
};
use crate::models::{Link, Pano, PanoDot, TimeEntry};
use rand::seq::SliceRandom;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const RPC: &str = "https://maps.googleapis.com/$rpc/google.internal.maps.mapsjs.v1.MapsJsInternalService";
const PHOTOMETA: &str = "https://www.google.com/maps/photometa/ac/v1";
/// How many tile panoramas to try before giving up on a sample.
const ZOOM_METADATA_TRIES: usize = 4;
/// Tile lists are refetched periodically so coverage released during a long
/// run does not stay invisible behind an old cache entry.
const ZOOM_CACHE_TTL: Duration = Duration::from_secs(60);

pub async fn fetch(providers: &Providers, req: &PanoRequest, zoom: bool) -> FetchResult {
    if let Some(pano) = req.pano.as_deref().filter(|p| !p.is_empty()) {
        return by_id(providers, pano).await;
    }
    let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
        return FetchResult::error();
    };
    if zoom {
        if let Some(pano) = zoom_search(providers, lat, lng).await {
            return FetchResult::ok(pano);
        }
    }
    spatial_search(providers, lat, lng, req.radius, req.official_only).await
}

async fn by_id(providers: &Providers, id: &str) -> FetchResult {
    match metadata(providers, id).await {
        Some(root) => match parse_root(&root) {
            Some(pano) => FetchResult::ok(pano),
            None => FetchResult::zero(),
        },
        None => FetchResult::error(),
    }
}

fn photometa_url(x: u32, y: u32) -> String {
    format!("{PHOTOMETA}?pb=!1m1!1smaps_sv.tactile!6m3!1i{x}!2i{y}!3i17!8b1")
}

/// Tile dots are shared by samples in the same z17 tile, but expire after a
/// minute so a drive published mid-run can still be discovered. Empty tiles
/// are never cached, so new coverage there is picked up on the next sample.
async fn tile_dots(providers: &Providers, lat: f64, lng: f64) -> Vec<PanoDot> {
    let (x, y) = zoom_tile(lat, lng);
    if let Ok(cache) = providers.google_zoom.lock() {
        if let Some((fetched, dots)) = cache.get(&(x, y)) {
            if fetched.elapsed() < ZOOM_CACHE_TTL {
                return dots.clone();
            }
        }
    }
    let Some(text) = get_text(providers, &photometa_url(x, y)).await else {
        return Vec::new();
    };
    let dots = parse_photometa(&text);
    if let Ok(mut cache) = providers.google_zoom.lock() {
        if dots.is_empty() {
            cache.remove(&(x, y));
        } else {
            cache.insert((x, y), (Instant::now(), dots.clone()));
        }
    }
    dots
}

async fn tile_pano_ids(providers: &Providers, lat: f64, lng: f64) -> Vec<String> {
    tile_dots(providers, lat, lng)
        .await
        .into_iter()
        .filter(|dot| !dot.pano_id.is_empty())
        .map(|dot| dot.pano_id)
        .collect()
}

/// Every point in one z17 photometa tile, including coverage-only entries
/// with no resolvable pano id. Used by the close-zoom dot layer.
pub async fn fetch_pano_dots(providers: &Providers, x: u32, y: u32) -> Vec<PanoDot> {
    const ZOOM: u32 = 17;
    let scale = 1_u64 << ZOOM;
    if u64::from(x) >= scale || u64::from(y) >= scale {
        return Vec::new();
    }
    let Some(text) = get_text(providers, &photometa_url(x, y)).await else {
        return Vec::new();
    };
    parse_photometa(&text)
}

/// Pick a random panorama from the sample's tile. The tile only carries IDs,
/// so the full record still comes from `GetMetadata`. Tiles without usable
/// entries fall back to the spatial search in `fetch`.
async fn zoom_search(providers: &Providers, lat: f64, lng: f64) -> Option<Pano> {
    let mut ids = tile_pano_ids(providers, lat, lng).await;
    if ids.is_empty() {
        return None;
    }
    ids.shuffle(&mut rand::thread_rng());
    for id in ids.into_iter().take(ZOOM_METADATA_TRIES) {
        if let Some(root) = metadata(providers, &id).await {
            if let Some(pano) = parse_root(&root) {
                return Some(pano);
            }
        }
    }
    None
}

fn zoom_tile(lat: f64, lng: f64) -> (u32, u32) {
    const ZOOM: i32 = 17;
    let scale = 2_f64.powi(ZOOM);
    let x = ((lng + 180.0) / 360.0 * scale).floor() as u32;
    let y = ((1.0 - lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0 * scale).floor()
        as u32;
    (x, y)
}

/// Tile entries live at `response[1][1][..][0]`; the id is `[0][1]` and the
/// position is `[2][0][2..3]`. Coverage-only entries have no id but still have
/// a position, so they are kept for the dot layer and filtered out when the
/// generator needs a resolvable panorama.
fn parse_photometa(text: &str) -> Vec<PanoDot> {
    let text = text.strip_prefix(")]}'").unwrap_or(text).trim_start();
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(entries) = idx(&idx(&value, 1), 1).as_array() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let info = idx(entry, 0);
            let pos = idx(&idx(info, 2), 0);
            let lat = idx(pos, 2).as_f64()?;
            let lng = idx(pos, 3).as_f64()?;
            if !lat.is_finite()
                || !lng.is_finite()
                || !(-90.0..=90.0).contains(&lat)
                || !(-180.0..=180.0).contains(&lng)
            {
                return None;
            }
            Some(PanoDot {
                pano_id: as_str(idx(&idx(info, 0), 1)).unwrap_or("").to_string(),
                lat,
                lng,
            })
        })
        .collect()
}

async fn spatial_search(
    providers: &Providers,
    lat: f64,
    lng: f64,
    radius: f64,
    official_only: bool,
) -> FetchResult {
    let Some(root) = search(providers, lat, lng, radius, official_only).await else {
        return FetchResult::zero();
    };
    // The spatial search already returns the full panorama record: location,
    // links, address and history match `GetMetadata` field for field (only the
    // unused entitlement token differs). Skipping the second round trip halves
    // the traffic per sample.
    if let Some(pano) = parse_root(&root) {
        return FetchResult::ok(pano);
    }
    // Older/newer response shapes may not parse directly; fall back to the
    // metadata endpoint for the panorama the search points at.
    let pano_id = idx(&root, 1)
        .get(1)
        .and_then(|value| value.as_str())
        .map(|id| id.to_string());
    match pano_id {
        Some(id) => match metadata(providers, &id).await {
            Some(full) => match parse_root(&full) {
                Some(pano) => FetchResult::ok(pano),
                None => FetchResult::zero(),
            },
            None => FetchResult::error(),
        },
        None => FetchResult::zero(),
    }
}

async fn search(
    providers: &Providers,
    lat: f64,
    lng: f64,
    radius: f64,
    official_only: bool,
) -> Option<Value> {
    let source = if official_only { 1 } else { 2 };
    let radius = if radius > 0.0 { radius } else { 50.0 };
    let payload = json!([
        ["apiv3", null, null, null, "US", null, null, null, null, null, [[0]]],
        [[null, null, lat, lng], radius],
        [
            null,
            ["en", "US"],
            null,
            null,
            null,
            null,
            null,
            null,
            [source],
            null,
            [[[2, 1, 2]]]
        ],
        [[1, 2, 3, 4, 8, 6]]
    ]);
    let url = format!("{RPC}/SingleImageSearch");
    let data = post_json(
        providers,
        &url,
        &payload.to_string(),
        &[
            ("content-type", "application/json+protobuf"),
            ("x-user-agent", "grpc-web-javascript/0.1"),
        ],
    )
    .await?;
    let result = idx(&data, 1);
    if result.is_null() {
        return None;
    }
    // The verified response keeps the panorama root at `data[1]`; accept
    // `data[1][0]` as well for older/newer response shapes.
    let candidates = [result, idx(result, 0)];
    for candidate in candidates {
        if parse_root(candidate).is_some() {
            return Some(candidate.clone());
        }
    }
    None
}

async fn metadata(providers: &Providers, pano: &str) -> Option<Value> {
    let pano_type = if pano.starts_with("CIHM") || pano.len() != 22 {
        10
    } else {
        2
    };
    let payload = json!([
        ["apiv3", null, null, null, "US", null, null, null, null, null, [[0]]],
        ["en", "US"],
        [[[pano_type, pano]]],
        [[1, 2, 3, 4, 8, 6]]
    ]);
    let url = format!("{RPC}/GetMetadata");
    let data = post_json(
        providers,
        &url,
        &payload.to_string(),
        &[
            ("content-type", "application/json+protobuf"),
            ("x-user-agent", "grpc-web-javascript/0.1"),
        ],
    )
    .await?;
    let root = idx(&data, 1).get(0)?;
    if root.is_null() {
        None
    } else {
        Some(root.clone())
    }
}

fn parse_root(root: &Value) -> Option<Pano> {
    if !root.is_array() {
        return None;
    }
    let meta = idx(idx(root, 5), 0);
    let loc = idx(meta, 1);
    let pano_id = as_str(idx(idx(root, 1), 1))?.to_string();
    let lat = as_f64(idx(idx(loc, 0), 2))?;
    let lng = as_f64(idx(idx(loc, 0), 3))?;

    let world = idx(idx(root, 2), 2);
    let (world_height, world_width) = if world.is_array() {
        (
            as_f64(idx(world, 0)).unwrap_or(4096.0) as u32,
            as_f64(idx(world, 1)).unwrap_or(4096.0) as u32,
        )
    } else {
        (4096, 4096)
    };

    let image_date = month_string(
        idx(idx(idx(root, 6), 7), 0),
        idx(idx(idx(root, 6), 7), 1),
    );

    let altitude = as_f64(idx(idx(loc, 1), 0));
    let country = as_str(idx(loc, 4)).map(str::to_string);

    let addr = idx(root, 3);
    let addr_full = as_str(idx(idx(idx(addr, 2), 1), 0))
        .or_else(|| as_str(idx(idx(idx(addr, 2), 0), 0)));
    let mut region = None;
    let mut locality = None;
    if let Some(full) = addr_full {
        let parts: Vec<&str> = full.split(',').collect();
        if parts.len() > 1 {
            locality = Some(parts[0].trim().to_string());
            region = Some(parts[parts.len() - 1].trim().to_string());
        } else {
            region = Some(full.trim().to_string());
        }
    }
    let desc_raw = as_str(idx(idx(idx(addr, 2), 1), 0))
        .or_else(|| as_str(idx(idx(addr, 0), 0)));
    let short_desc_raw = as_str(idx(idx(idx(addr, 2), 0), 0))
        .or_else(|| as_str(idx(idx(addr, 0), 0)));

    let description = [short_desc_raw, desc_raw]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let description = if description.is_empty() {
        None
    } else {
        Some(description)
    };

    let service = super::as_string(idx(idx(idx(root, 6), 5), 2));
    let nodes = idx(idx(meta, 3), 0);
    let links_raw = idx(meta, 6);
    let history_raw = idx(meta, 8);

    let mut time: Vec<TimeEntry> = Vec::new();
    if let Some(history) = history_raw.as_array() {
        for entry in history {
            let Some(node_idx) = as_f64(idx(entry, 0)) else {
                continue;
            };
            let node = idx(nodes, node_idx as usize);
            let Some(pano) = as_str(idx(idx(node, 0), 1)) else {
                continue;
            };
            if pano.starts_with("CIHM0") {
                continue;
            }
            let Some(date) = month_string(idx(idx(entry, 1), 0), idx(idx(entry, 1), 1)) else {
                continue;
            };
            time.push(TimeEntry {
                pano: pano.to_string(),
                date,
            });
        }
    }
    if let Some(date) = &image_date {
        time.push(TimeEntry {
            pano: pano_id.clone(),
            date: date.clone(),
        });
    }
    time.sort_by(|a, b| a.date.cmp(&b.date));

    let mut links: Vec<Link> = Vec::new();
    if let Some(raw_links) = links_raw.as_array() {
        for link in raw_links {
            let Some(node_idx) = as_f64(idx(link, 0)) else {
                continue;
            };
            let node = idx(nodes, node_idx as usize);
            let Some(pano) = as_str(idx(idx(node, 0), 1)) else {
                continue;
            };
            links.push(Link {
                pano: pano.to_string(),
                heading: as_f64(idx(idx(link, 1), 3)).unwrap_or(0.0),
                description: String::new(),
            });
        }
    }

    Some(Pano {
        pano_id,
        lat,
        lng,
        country,
        region,
        locality,
        road: as_str(idx(idx(idx(idx(idx(idx(meta, 12), 0), 0), 0), 2), 0))
            .map(|s| s.to_string()),
        description,
        short_description: short_desc_raw.map(|s| s.to_string()),
        image_date,
        copyright: Some("© Google".to_string()),
        service,
        altitude,
        world_width,
        world_height,
        center_heading: as_f64(idx(idx(loc, 2), 0)).unwrap_or(0.0),
        links,
        time,
        provider: "google".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_with_country(country: &str) -> Value {
        json!([
            null,
            [null, "abcdefghijklmnopqrstuv"],
            [null, null, [1664, 3328]],
            [],
            null,
            [
                [
                    [1],
                    [
                        [null, null, 48.85, 2.29],
                        [35.0],
                        [123.0],
                        ["node-a", "node-b"],
                        country
                    ],
                    null,
                    [[[[null, "abcdefghijklmnopqrstuv"]], [[null, "node-b"]]]],
                    null,
                    null,
                    [[1, [null, null, null, 90.0]]],
                    null,
                    [[0, [2020, 1]]],
                    null,
                    null,
                    null,
                    [[[[null, null, ["Rue de Test"]]]]]
                ]
            ],
            [null, null, null, null, null, [null, null, "launch"], null, [2021, 5]]
        ])
    }

    #[test]
    fn parses_minimal_metadata() {
        let pano = parse_root(&root_with_country("FR")).expect("parsed");
        assert_eq!(pano.pano_id, "abcdefghijklmnopqrstuv");
        assert_eq!(pano.image_date.as_deref(), Some("2021-05"));
        assert_eq!(pano.country.as_deref(), Some("FR"));
        assert_eq!(pano.road.as_deref(), Some("Rue de Test"));
        assert_eq!(pano.center_heading, 123.0);
        assert_eq!(pano.world_height, 1664);
        assert_eq!(pano.world_width, 3328);
        assert_eq!(pano.service.as_deref(), Some("launch"));
        assert_eq!(pano.links.len(), 1);
        assert_eq!(pano.links[0].pano, "node-b");
        assert_eq!(pano.links[0].heading, 90.0);
        assert_eq!(pano.time.len(), 2);
    }

    #[test]
    fn keeps_tw_hk_mo_country_codes() {
        for code in ["TW", "HK", "MO"] {
            let pano = parse_root(&root_with_country(code)).expect("parsed");
            assert_eq!(pano.country.as_deref(), Some(code));
        }
    }

    #[test]
    fn zoom_tile_matches_reference() {
        assert_eq!(zoom_tile(0.0, 0.0), (1 << 16, 1 << 16));
        assert_eq!(zoom_tile(53.69738, -132.16928), (17414, 42271));
    }

    #[test]
    fn parses_photometa_tile() {
        let body = ")]}'\n[null,[null,[[[[2,\"abcdefghijklmnopqrstuv\"],null,[[null,null,48.85,2.29],[1,2],[3,4]]],[1,19]]]],null]";
        let dots = parse_photometa(body);
        assert_eq!(dots.len(), 1);
        assert_eq!(dots[0].pano_id, "abcdefghijklmnopqrstuv");
        assert_eq!(dots[0].lat, 48.85);
        assert_eq!(dots[0].lng, 2.29);
    }

    #[test]
    fn keeps_photometa_entries_without_id() {
        // Coverage geometry entries still need a dot; only the generator's id
        // lookup ignores them.
        let body = "[null,[null,[[[null,null,[[null,null,53.69,-132.18]]],[1,19]]]],null]";
        let dots = parse_photometa(body);
        assert_eq!(dots.len(), 1);
        assert!(dots[0].pano_id.is_empty());
        assert_eq!(dots[0].lat, 53.69);
        assert_eq!(dots[0].lng, -132.18);
    }
}
