//! Baidu Street View (mapsv0.bdimg.com).

use super::{as_f64, as_str, field, get_json, idx, FetchResult, PanoRequest, Providers};
use crate::dates::extract_date_from_pano_id;
use crate::geo::{bd09mc_to_gcj02, wgs84_to_bd09mc};
use crate::models::{Link, Pano, TimeEntry};

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let pano_id = match req.pano.as_deref() {
        Some(id) => id.to_string(),
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let (x, y) = wgs84_to_bd09mc(lng, lat);
            let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
            let url = format!("https://mapsv0.bdimg.com/?qt=qsdata&x={x}&y={y}&r={radius}");
            let Some(json) = get_json(providers, &url).await else {
                return FetchResult::error();
            };
            match super::as_string(field(field(&json, "content"), "id")) {
                Some(id) => id,
                None => return FetchResult::zero(),
            }
        }
    };

    let url = format!("https://mapsv0.bdimg.com/?qt=sdata&sid={pano_id}");
    let Some(json) = get_json(providers, &url).await else {
        return FetchResult::error();
    };
    let result = idx(field(&json, "content"), 0);
    if as_str(field(result, "ID")).is_none() {
        return FetchResult::zero();
    }
    match parse(result, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(result: &serde_json::Value, pano_id: &str) -> Option<Pano> {
    let x = as_f64(field(result, "X"))? / 100.0;
    let y = as_f64(field(result, "Y"))? / 100.0;
    let (lng, lat) = bd09mc_to_gcj02(x, y);
    let date_part = pano_id.get(10..22).unwrap_or("");
    let image_date = extract_date_from_pano_id(date_part);

    let mut links = Vec::new();
    if let Some(list) = field(result, "Links").as_array() {
        for link in list {
            if let Some(id) = super::as_string(field(link, "PID")) {
                links.push(Link {
                    pano: id,
                    heading: 0.0,
                    description: String::new(),
                });
            }
        }
    }

    let mut time = Vec::new();
    if let Some(list) = field(result, "TimeLine").as_array() {
        for item in list {
            let Some(id) = super::as_string(field(item, "ID")) else {
                continue;
            };
            if let Some(date) = id.get(10..22).and_then(extract_date_from_pano_id) {
                time.push(TimeEntry { pano: id, date });
            }
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
        country: Some("CN".to_string()),
        region: None,
        locality: None,
        road: None,
        description: super::as_string(field(result, "Rname")),
        short_description: None,
        image_date,
        copyright: Some("© Baidu Maps".to_string()),
        service: None,
        altitude: as_f64(field(result, "Z")),
        world_width: 8192,
        world_height: 4096,
        center_heading: as_f64(field(result, "Heading")).unwrap_or(0.0),
        links,
        time,
        provider: "baidu".to_string(),
    })
}
