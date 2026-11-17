//! Tencent Street View (QQ Maps).

use super::{as_f64, as_str, field, get_json, FetchResult, PanoRequest, Providers};
use crate::dates::extract_date_from_pano_id;
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
                "https://sv.map.qq.com/xf?output=json&charset=utf-8&lng={lng}&lat={lat}&r={radius}"
            );
            let Some(json) = get_json(providers, &url).await else {
                return FetchResult::error();
            };
            match as_str(field(field(&json, "detail"), "svid")) {
                Some(id) => id.to_string(),
                None => return FetchResult::zero(),
            }
        }
    };

    let url = format!("https://sv.map.qq.com/sv?output=json&svid={pano_id}");
    let Some(json) = get_json(providers, &url).await else {
        return FetchResult::error();
    };
    let detail = field(&json, "detail");
    let basic = field(detail, "basic");
    if as_str(field(basic, "svid")).is_none() {
        return FetchResult::zero();
    }
    match parse(detail, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(detail: &serde_json::Value, pano_id: &str) -> Option<Pano> {
    let basic = field(detail, "basic");
    let addr = field(detail, "addr");
    let lat = as_f64(field(addr, "y_lat"))?;
    let lng = as_f64(field(addr, "x_lng"))?;
    let svid = as_str(field(basic, "svid")).unwrap_or(pano_id);
    let date_part = svid.get(8..20).unwrap_or("");
    let image_date = extract_date_from_pano_id(date_part);

    let mode = as_str(field(basic, "mode")).unwrap_or("");
    let trans_svid = as_str(field(basic, "trans_svid"));
    let short_description = if mode == "night" {
        Some(pano_id.to_string())
    } else {
        trans_svid.map(|s| s.to_string())
    };

    let mut links = Vec::new();
    if let Some(scenes) = field(detail, "all_scenes").as_array() {
        for scene in scenes {
            if let Some(id) = as_str(field(scene, "svid")) {
                links.push(Link {
                    pano: id.to_string(),
                    heading: 0.0,
                    description: String::new(),
                });
            }
        }
    }

    let mut time = Vec::new();
    if let Some(nodes) = field(field(detail, "history"), "nodes").as_array() {
        for node in nodes {
            if let Some(id) = as_str(field(node, "svid")) {
                if let Some(date) = id.get(8..20).and_then(extract_date_from_pano_id) {
                    time.push(TimeEntry {
                        pano: id.to_string(),
                        date,
                    });
                }
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
        description: as_str(field(basic, "append_addr")).map(|s| s.to_string()),
        short_description,
        image_date,
        copyright: Some("© Tencent Maps".to_string()),
        service: None,
        altitude: None,
        world_width: 8192,
        world_height: 4096,
        center_heading: as_f64(field(basic, "dir")).unwrap_or(0.0),
        links,
        time,
        provider: "tencent".to_string(),
    })
}
