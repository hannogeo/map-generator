//! Kakao Road View.

use super::{as_f64, field, get_json, idx, FetchResult, PanoRequest, Providers};
use crate::models::{Link, Pano, TimeEntry};

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let pano_id = match req.pano.as_deref() {
        Some(id) => id.to_string(),
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
            let url = format!(
                "https://rv.map.kakao.com/roadview-search/v2/nodes?PX={lng}&PY={lat}&RAD={radius}&PAGE_SIZE=1&INPUT=wgs&TYPE=w&SERVICE=glpano"
            );
            let Some(json) = get_json(providers, &url).await else {
                return FetchResult::error();
            };
            let list = field(field(&json, "street_view"), "streetList");
            match super::as_string(field(idx(list, 0), "id")) {
                Some(id) => id,
                None => return FetchResult::zero(),
            }
        }
    };

    let url = format!("https://rv.map.kakao.com/roadview-search/v2/node/{pano_id}?SERVICE=glpano");
    let Some(json) = get_json(providers, &url).await else {
        return FetchResult::error();
    };
    let street = field(field(&json, "street_view"), "street");
    if street.is_null() {
        return FetchResult::zero();
    }
    match parse(street, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(street: &serde_json::Value, pano_id: &str) -> Option<Pano> {
    let lat = as_f64(field(street, "wgsy"))?;
    let lng = as_f64(field(street, "wgsx"))?;
    let angle = as_f64(field(street, "angle")).unwrap_or(0.0);
    let heading = (angle + 180.0) % 360.0;
    let image_date = super::as_string(field(street, "shot_date"));

    let mut links = Vec::new();
    if let Some(spots) = field(street, "spot").as_array() {
        for spot in spots {
            let Some(id) = super::as_string(field(spot, "id")) else {
                continue;
            };
            let pan = as_f64(field(spot, "pan")).unwrap_or(0.0);
            links.push(Link {
                pano: id,
                heading: (pan % 180.0) + if heading > 180.0 { 180.0 } else { 0.0 },
                description: String::new(),
            });
        }
    }

    let mut time = Vec::new();
    if let Some(past) = field(street, "past").as_array() {
        for item in past {
            let (Some(id), Some(date)) = (
                super::as_string(field(item, "id")),
                super::as_string(field(item, "shot_date")),
            ) else {
                continue;
            };
            time.push(TimeEntry { pano: id, date });
        }
    }
    if let Some(date) = &image_date {
        time.push(TimeEntry {
            pano: pano_id.to_string(),
            date: date.clone(),
        });
    }
    time.sort_by(|a, b| a.date.cmp(&b.date));

    Some(Pano {
        pano_id: pano_id.to_string(),
        lat,
        lng,
        country: Some("KR".to_string()),
        region: None,
        locality: None,
        road: None,
        description: super::as_string(field(street, "addr")),
        short_description: None,
        image_date,
        copyright: Some("© Kakao Maps".to_string()),
        service: None,
        altitude: None,
        world_width: 8192,
        world_height: 4096,
        center_heading: heading,
        links,
        time,
        provider: "kakao".to_string(),
    })
}
