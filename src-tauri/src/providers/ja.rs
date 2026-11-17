//! Já360 (Iceland).

use super::{
    as_f64, as_string, field, get_json, get_json_proxied, FetchResult, PanoRequest, Providers,
};
use crate::models::{Link, Pano, TimeEntry};

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let pano_id = match req.pano.as_deref() {
        Some(id) => id.to_string(),
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
            let url = format!("https://ja.is/kort/closest/?lat={lat}&lng={lng}&meters={radius}");
            let Some(result) = get_json_proxied(providers, &url).await else {
                return FetchResult::error();
            };
            let Some(id) = as_string(field(&result, "id")) else {
                return FetchResult::zero();
            };
            id
        }
    };

    let url = format!("https://ja.is/kort/scene/?json=1&image_id={pano_id}");
    let result = match get_json(providers, &url).await {
        Some(value) => value,
        None => {
            let proxied = format!("https://cors-proxy.ac4.stocc.dev/{url}");
            match get_json(providers, &proxied).await {
                Some(value) => value,
                None => return FetchResult::error(),
            }
        }
    };

    let image = field(&result, "image");
    if as_string(field(image, "id")).is_none() {
        return FetchResult::zero();
    }

    match parse(&result, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(result: &serde_json::Value, pano_id: &str) -> Option<Pano> {
    let image = field(result, "image");
    let lat = as_f64(field(image, "lat"))?;
    let lng = as_f64(field(image, "lng"))?;
    let street = field(field(result, "streets"), "street");
    let name = as_string(field(street, "name")).unwrap_or_else(|| "Já360".to_string());
    let date = as_string(field(image, "month"));

    let mut links = Vec::new();
    if let Some(azimuths) = field(street, "azimuths").as_array() {
        let prefix = pano_id
            .char_indices()
            .next_back()
            .map(|(i, _)| &pano_id[..i])
            .unwrap_or("");
        for (index, azimuth) in azimuths.iter().enumerate() {
            links.push(Link {
                pano: format!("{prefix}{index}"),
                heading: as_f64(azimuth).unwrap_or(0.0),
                description: name.clone(),
            });
        }
    }

    let time = match &date {
        Some(date) if !date.is_empty() => vec![TimeEntry {
            pano: pano_id.to_string(),
            date: date.clone(),
        }],
        _ => Vec::new(),
    };

    Some(Pano {
        pano_id: pano_id.to_string(),
        lat,
        lng,
        country: Some("IS".to_string()),
        region: None,
        locality: None,
        road: None,
        description: Some(name),
        short_description: None,
        image_date: date,
        copyright: None,
        service: None,
        altitude: None,
        world_width: 8192,
        world_height: 4096,
        center_heading: as_f64(field(image, "heading")).unwrap_or(0.0),
        links,
        time,
        provider: "ja".to_string(),
    })
}
