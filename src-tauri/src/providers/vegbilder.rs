//! Vegbilder (Norwegian Public Roads Administration imagery).

use super::{as_f64, as_str, as_string, field, get_json, idx, FetchResult, PanoRequest, Providers};
use crate::models::{Link, Pano, TimeEntry};
use rand::seq::SliceRandom;
use std::collections::HashSet;

const BASE_URL: &str = "https://ogckart-sn1.atlas.vegvesen.no/vegbilder_1_0/ows";
const MAX_FEATURE_COUNT: u32 = 100;

#[derive(Debug, Clone, Default)]
pub struct VegbilderFeature {
    pub id: String,
    pub lat: f64,
    pub lng: f64,
    pub heading: f64,
    pub date: Option<String>,
    pub description: String,
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
    let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
    let features = load_nearby(providers, lat, lng, radius).await;
    if features.is_empty() {
        return FetchResult::zero();
    }

    let mut rng = rand::thread_rng();
    let Some(feature) = features.choose(&mut rng) else {
        return FetchResult::zero();
    };
    let index = features.iter().position(|f| f.id == feature.id);
    let mut links = Vec::new();
    if let Some(index) = index {
        if let Some(prev) = index.checked_sub(1).and_then(|i| features.get(i)) {
            links.push(Link {
                pano: prev.id.clone(),
                heading: (feature.heading + 180.0) % 360.0,
                description: String::new(),
            });
        }
        if let Some(next) = features.get(index + 1) {
            links.push(Link {
                pano: next.id.clone(),
                heading: feature.heading,
                description: String::new(),
            });
        }
    }

    FetchResult::ok(build_pano(feature, links))
}

fn find_cached(providers: &Providers, id: &str) -> Option<VegbilderFeature> {
    let tiles = providers.vegbilder.lock().ok()?;
    for features in tiles.values() {
        if let Some(feature) = features.iter().find(|f| f.id == id) {
            return Some(feature.clone());
        }
    }
    None
}

async fn load_nearby(
    providers: &Providers,
    lat: f64,
    lng: f64,
    radius: f64,
) -> Vec<VegbilderFeature> {
    let (min_lat, min_lng, max_lat, max_lng) = bounds(lat, lng, radius);
    let key = format!("{min_lat:.5},{min_lng:.5},{max_lat:.5},{max_lng:.5}");
    if let Ok(cached) = providers.vegbilder.lock() {
        if let Some(features) = cached.get(&key) {
            return features.clone();
        }
    }

    let url = format!(
        "{BASE_URL}?service=WFS&version=2.0.0&request=GetFeature&typenames=vegbilder_1_0:Vegbilder_360_2025&startindex=0&count={MAX_FEATURE_COUNT}&srsname=urn:ogc:def:crs:EPSG::4326&bbox={min_lat},{min_lng},{max_lat},{max_lng},urn:ogc:def:crs:EPSG::4326&outputformat=application/json"
    );

    let mut features = Vec::new();
    let mut seen = HashSet::new();
    if let Some(json) = get_json(providers, &url).await {
        if let Some(list) = field(&json, "features").as_array() {
            for item in list {
                let props = field(item, "properties");
                let coords = field(field(item, "geometry"), "coordinates");
                let lng = as_f64(idx(coords, 0)).unwrap_or(0.0);
                let lat = as_f64(idx(coords, 1)).unwrap_or(0.0);
                let id = as_string(field(item, "id"))
                    .or_else(|| as_string(field(props, "id")))
                    .unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let dedupe_key = if !id.is_empty() {
                    id.clone()
                } else {
                    format!("{lng},{lat}")
                };
                if !seen.insert(dedupe_key) {
                    continue;
                }
                let description = format!(
                    "{}{}{}",
                    as_str(field(props, "VEGKATEGORI")).unwrap_or(""),
                    as_str(field(props, "VEGSTATUS")).unwrap_or(""),
                    as_str(field(props, "VEGNUMMER")).unwrap_or(""),
                );
                features.push(VegbilderFeature {
                    id,
                    lat,
                    lng,
                    heading: as_f64(field(props, "RETNING")).unwrap_or(0.0),
                    date: as_string(field(props, "TIDSPUNKT")),
                    description,
                });
            }
        }
    }

    if let Ok(mut cached) = providers.vegbilder.lock() {
        if cached.len() > 500 {
            cached.clear();
        }
        cached.insert(key, features.clone());
    }
    features
}

fn bounds(lat: f64, lng: f64, radius: f64) -> (f64, f64, f64, f64) {
    let d_lat = (radius / 6_378_137.0).to_degrees();
    let d_lng = d_lat / lat.to_radians().cos().max(1e-6);
    (lat - d_lat, lng - d_lng, lat + d_lat, lng + d_lng)
}

fn build_pano(feature: &VegbilderFeature, links: Vec<Link>) -> Pano {
    Pano {
        pano_id: feature.id.clone(),
        lat: feature.lat,
        lng: feature.lng,
        country: Some("NO".to_string()),
        region: None,
        locality: None,
        road: None,
        description: if feature.description.is_empty() {
            None
        } else {
            Some(feature.description.clone())
        },
        short_description: None,
        image_date: feature.date.clone(),
        copyright: Some("© Statens vegvesen".to_string()),
        service: None,
        altitude: None,
        world_width: 8192,
        world_height: 4096,
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
        provider: "vegbilder".to_string(),
    }
}
