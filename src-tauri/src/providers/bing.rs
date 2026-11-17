//! Bing Streetside.

use super::{as_f64, field, idx, FetchResult, PanoRequest, Providers};
use crate::dates::{epoch_ms_to_iso, parse_flexible_ms, parse_us_datetime};
use crate::models::{Link, Pano, TimeEntry};
use rand::Rng;
use serde_json::Value;

const ENDPOINT: &str = "https://t.ssl.ak.tiles.virtualearth.net/tiles/cmd/StreetSideBubbleMetaData";

async fn bing_json(providers: &Providers, url: &str) -> Option<Value> {
    // Bing answers with gzip framing that occasionally trips decoders; asking
    // for identity keeps the JSON parse reliable (same trick as the reference).
    let response = providers
        .client
        .get(url)
        .header("accept-encoding", "identity")
        .header("accept", "application/json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok()
}

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let pano_id = match req.pano.as_deref() {
        Some(id) => id.to_string(),
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
            let range = radius / 1000.0 / 111.0;
            let url = format!(
                "{ENDPOINT}?count=50&north={}&south={}&east={}&west={}",
                lat + range,
                lat - range,
                lng + range,
                lng - range
            );
            let Some(json) = bing_json(providers, &url).await else {
                return FetchResult::error();
            };
            let results = json.as_array().map(|a| &a[1..]).unwrap_or(&[]);
            if results.is_empty() {
                return FetchResult::zero();
            }
            let index = rand::thread_rng().gen_range(0..results.len());
            match super::as_string(field(&results[index], "id")) {
                Some(id) => id,
                None => return FetchResult::zero(),
            }
        }
    };

    let url = format!("{ENDPOINT}?id={pano_id}");
    let Some(json) = bing_json(providers, &url).await else {
        return FetchResult::error();
    };
    let result = idx(&json, 1);
    if super::as_string(field(result, "id")).is_none() {
        return FetchResult::zero();
    }
    match parse(result, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn bing_date(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if let Some(ms) = parse_flexible_ms(text) {
            return Some(epoch_ms_to_iso(ms));
        }
        if let Some(iso) = parse_us_datetime(text) {
            return Some(iso);
        }
        if let Some(inner) = text
            .strip_prefix("/Date(")
            .and_then(|rest| rest.split(')').next())
        {
            let digits: String = inner.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            if let Ok(ms) = digits.parse::<i64>() {
                return Some(epoch_ms_to_iso(ms));
            }
        }
        return Some(text.to_string());
    }
    value.as_i64().map(epoch_ms_to_iso)
}

fn parse(result: &Value, pano_id: &str) -> Option<Pano> {
    let lat = as_f64(field(result, "la"))?;
    let lng = as_f64(field(result, "lo"))?;
    let heading = as_f64(field(result, "he")).unwrap_or(0.0);
    let date = bing_date(field(result, "cd"));

    let mut links = Vec::new();
    if let Some(pr) = super::as_string(field(result, "pr")) {
        links.push(Link {
            pano: pr,
            heading: (heading + 180.0) % 360.0,
            description: String::new(),
        });
    }
    if let Some(ne) = super::as_string(field(result, "ne")) {
        links.push(Link {
            pano: ne,
            heading,
            description: String::new(),
        });
    }
    if let Some(neighbours) = field(result, "nbn").as_array() {
        for link in neighbours {
            let id = field(link, "id");
            let Some(id) = super::as_string(id) else { continue };
            if id == pano_id {
                continue;
            }
            links.push(Link {
                pano: id,
                heading: as_f64(field(link, "az")).unwrap_or(0.0),
                description: String::new(),
            });
        }
    }

    let mut time = Vec::new();
    if let Some(date) = &date {
        time.push(TimeEntry {
            pano: pano_id.to_string(),
            date: date.clone(),
        });
    }

    Some(Pano {
        pano_id: pano_id.to_string(),
        lat,
        lng,
        country: None,
        region: None,
        locality: None,
        road: None,
        description: super::as_string(field(result, "ml")),
        short_description: None,
        image_date: date,
        copyright: Some("© Bing Streetside".to_string()),
        service: super::as_string(field(result, "ml")),
        altitude: as_f64(field(result, "al")),
        world_width: 8192,
        world_height: 4096,
        center_heading: heading,
        links,
        time,
        provider: "bing".to_string(),
    })
}
