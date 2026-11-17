//! Naver Street View.

use super::{as_f64, as_str, field, get_json_proxied, idx, FetchResult, PanoRequest, Providers};
use crate::dates::{epoch_ms_to_iso, parse_flexible_ms};
use crate::models::{Link, Pano, TimeEntry};

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let (pano_id, nearby_heading) = match req.pano.as_deref() {
        Some(id) => (id.to_string(), None),
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let url = format!(
                "https://panorama.map.naver.com/api/v2/nearby/{lng}/{lat}?lang=en"
            );
            let Some(json) = get_json_proxied(providers, &url).await else {
                return FetchResult::error();
            };
            let feature = idx(field(&json, "features"), 0);
            let Some(id) = as_str(field(field(feature, "properties"), "id")) else {
                return FetchResult::zero();
            };
            (
                id.to_string(),
                as_f64(field(field(feature, "properties"), "heading")),
            )
        }
    };

    let basic_url =
        format!("https://panorama.map.naver.com/metadataV3/basic/{pano_id}?lang=en");
    let timeline_url = format!("https://panorama.map.naver.com/metadata/timeline/{pano_id}");
    let (basic, timeline) = tokio::join!(
        get_json_proxied(providers, &basic_url),
        get_json_proxied(providers, &timeline_url)
    );
    let Some(basic) = basic else {
        return FetchResult::error();
    };
    if as_str(field(&basic, "id")).is_none() {
        return FetchResult::zero();
    }
    match parse(&basic, timeline.as_ref(), &pano_id, nearby_heading) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(
    basic: &serde_json::Value,
    timeline: Option<&serde_json::Value>,
    pano_id: &str,
    nearby_heading: Option<f64>,
) -> Option<Pano> {
    let lat = as_f64(field(basic, "latitude"))?;
    let lng = as_f64(field(basic, "longitude"))?;
    let info = field(basic, "info");
    let image_date = super::as_string(field(info, "photodate"));

    let mut links = Vec::new();
    if let Some(list) = field(basic, "links").as_array() {
        for link in list {
            if let Some(id) = as_str(field(link, "id")) {
                links.push(Link {
                    pano: id.to_string(),
                    heading: 0.0,
                    description: String::new(),
                });
            }
        }
    }

    let mut time = Vec::new();
    if let Some(panoramas) = timeline
        .and_then(|t| field(field(t, "timeline"), "panoramas").as_array())
    {
        for item in panoramas.iter().skip(1) {
            let Some(id) = as_str(idx(item, 0)) else {
                continue;
            };
            let date = idx(item, 4);
            let date = match date.as_str().and_then(parse_flexible_ms) {
                Some(ms) => epoch_ms_to_iso(ms),
                None => super::as_string(date).unwrap_or_default(),
            };
            time.push(TimeEntry {
                pano: id.to_string(),
                date,
            });
        }
        time.sort_by(|a, b| a.date.cmp(&b.date));
    }
    if time.is_empty() {
        if let Some(date) = &image_date {
            time.push(TimeEntry {
                pano: pano_id.to_string(),
                date: date.clone(),
            });
        }
    }

    Some(Pano {
        pano_id: pano_id.to_string(),
        lat,
        lng,
        country: Some("KR".to_string()),
        region: None,
        locality: None,
        road: None,
        description: super::as_string(field(info, "description")),
        short_description: None,
        image_date,
        copyright: Some("© Naver".to_string()),
        service: super::as_string(field(basic, "dtl_type")),
        altitude: as_f64(field(basic, "altitude")),
        world_width: 8192,
        world_height: 4096,
        center_heading: nearby_heading.unwrap_or(0.0),
        links,
        time,
        provider: "naver".to_string(),
    })
}
