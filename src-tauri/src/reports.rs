//! VirtualStreets update report messages, matching the Google StreetView
//! Toolkit userscript's `generateUpdateReportText` output.

use crate::dates::parse_month;
use crate::models::Marker;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const UPDATE_TYPE_URL: &str =
    "https://vs-update-map.netlify.app/.netlify/functions/checkUpdateType";
const COUNTRY_URL: &str =
    "https://cdn.jsdelivr.net/gh/umpirsky/country-list/data/en_US/country.json";
const OSM_URL: &str = "https://nominatim.openstreetmap.org/reverse";
const SHORT_URL_URL: &str = "https://www.google.com/maps/rpc/shorturl";
const NORMAL_CHANNEL: &str = "905824309963014195";
const BAD_CAM_CHANNEL: &str = "1231106542351421495";

const FULL_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const SMALL_CAM_COUNTRIES: [&str; 7] = ["IN", "PR", "CY", "MK", "XK", "ME", "AL"];

#[derive(Clone, Debug, Default, Deserialize)]
struct UpdateTypes {
    #[serde(default)]
    newyear: bool,
    #[serde(default)]
    newcountry: bool,
    #[serde(default)]
    newregion: bool,
}

#[derive(Debug, Default, Deserialize)]
struct OsmAddress {
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    province: Option<String>,
    #[serde(default)]
    county: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    city_district: Option<String>,
    #[serde(default)]
    town: Option<String>,
    #[serde(default)]
    road: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OsmResponse {
    address: OsmAddress,
}

static UPDATE_TYPES: OnceLock<Mutex<HashMap<String, UpdateTypes>>> = OnceLock::new();
static COUNTRY_NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Builds the exact text copied by the StreetView Toolkit's report button.
pub async fn update_report(marker: &Marker) -> String {
    let mut marker = marker.clone();
    let osm_country_name = resolve_address(&mut marker).await;
    let (types, short_url, country_name) = tokio::join!(
        fetch_update_types(&marker),
        fetch_short_url(&marker),
        fetch_country_name(&marker, osm_country_name.as_deref())
    );
    format_report(&marker, &types, short_url.as_deref(), &country_name)
}

/// The userscript falls back to OSM when Google reports no usable country or
/// region, or when the panorama has no Google generation.
async fn resolve_address(marker: &mut Marker) -> Option<String> {
    let is_ari = marker.generation.as_deref() == Some("ari");
    let missing_country = non_empty(marker.country.as_deref()).is_none();
    let missing_region = non_empty(marker.region.as_deref()).is_none();
    if !is_ari && !missing_country && !missing_region {
        return None;
    }

    let response = http_client()
        .get(OSM_URL)
        .query(&[
            ("format", "json"),
            ("lat", &marker.lat.to_string()),
            ("lon", &marker.lng.to_string()),
            ("accept-language", "en"),
        ])
        .send()
        .await
        .ok()?;
    let address = response.json::<OsmResponse>().await.ok()?.address;

    let country_name = address.country;
    if missing_country {
        marker.country = address.country_code.or(marker.country.take());
    }
    if missing_region {
        marker.region = address
            .state
            .or(address.region)
            .or(address.province)
            .or(address.county)
            .or(marker.region.take());
    }
    if marker
        .locality
        .as_deref()
        .map_or(true, |value| value.trim().is_empty())
    {
        marker.locality = address
            .city
            .or(address.city_district)
            .or(address.town)
            .or(marker.locality.take());
    }
    if marker
        .road
        .as_deref()
        .map_or(true, |value| value.trim().is_empty())
    {
        marker.road = address.road.or(marker.road.take());
    }
    country_name
}

/// `parseMeta` folds these into CN before the report is generated.
fn report_country(marker: &Marker) -> &str {
    let country = marker.country.as_deref().unwrap_or("");
    if matches!(
        country.trim().to_ascii_uppercase().as_str(),
        "TW" | "HK" | "MO"
    ) {
        "CN"
    } else {
        country
    }
}

fn format_report(
    marker: &Marker,
    types: &UpdateTypes,
    short_url: Option<&str>,
    country_name: &str,
) -> String {
    let country = report_country(marker);
    let mut parts = Vec::new();

    parts.push(format!(
        ":flag_{}:",
        if country.is_empty() {
            String::new()
        } else {
            country.trim().to_ascii_lowercase()
        }
    ));

    // `activeTypes` from `checkUpdateTypes`, in the userscript's order.
    if types.newyear {
        parts.push(":newyear:".to_string());
    }
    if types.newcountry {
        parts.push(":newcountry:".to_string());
    }
    if types.newregion {
        parts.push(":newregion:".to_string());
    }

    if !marker.has_history {
        parts.push(":newroad:".to_string());
    }

    if let Some(generation) = marker.previous_generation.as_deref() {
        let generation = if generation == "BadCam" {
            "ari"
        } else {
            generation
        };
        if generation != "Gen4" {
            parts.push(format!(":{}update:", generation.to_lowercase()));
        }
    }

    if marker.generation.as_deref() == Some("Gen4") && is_small_cam_country(country) {
        parts.push(":smallcam:".to_string());
    }

    if let Some(date) = format_date(marker.image_date.as_deref()) {
        parts.push(date);
    }
    parts.push(place_description(marker, country_name));

    let fallback = format!(
        "https://www.google.com/maps/@?api=1&map_action=pano&pano={}",
        marker.pano_id
    );
    parts.push(short_url.unwrap_or(&fallback).to_string());

    // The userscript uses `reportParts.join(' ')`, so this is one line.
    parts.join(" ")
}

fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| Client::new())
}

async fn fetch_short_url(marker: &Marker) -> Option<String> {
    let pb = format!(
        "!1shttps://www.google.com/maps/@{},{},{}a,0y,{}h,{}t/data=*213m5*211e1*213m3*211s{}*212e0*216shttps%3A%2F%2Fstreetviewpixels-pa.googleapis.com%2Fv1%2Fthumbnail%3Fpanoid%3D{}%26cb_client%3Dmaps_sv.share%26w%3D900%26h%3D600%26yaw%3D{}%26pitch%3D{}%26thumbfov%3D100*217i16384*218i8192?coh=205410&entry=tts!2m1!7e81!6b1",
        marker.lat,
        marker.lng,
        0,
        0,
        90,
        marker.pano_id,
        marker.pano_id,
        0,
        0,
    );
    let text = http_client()
        .get(SHORT_URL_URL)
        .query(&[("authuser", "0"), ("hl", "en"), ("gl", "us"), ("pb", &pb)])
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    first_quoted(&text)
}

fn first_quoted(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let end = start + text[start..].find('"')?;
    Some(text[start..end].to_string())
}

async fn fetch_update_types(marker: &Marker) -> UpdateTypes {
    let Some((year, month)) = marker.image_date.as_deref().and_then(parse_month) else {
        return UpdateTypes::default();
    };
    let country = report_country(marker);
    let region = marker.region.as_deref().unwrap_or("");
    let channel = if marker.generation.as_deref() == Some("BadCam") {
        BAD_CAM_CHANNEL
    } else {
        NORMAL_CHANNEL
    };
    let key = format!("{}|{}|{}|{}|{}", country, region, year, month, channel);

    let cache = UPDATE_TYPES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(types) = cache.lock().ok().and_then(|cache| cache.get(&key).cloned()) {
        return types;
    }

    let response = http_client()
        .get(UPDATE_TYPE_URL)
        .query(&[
            ("country", country),
            ("region", region),
            ("year", &year.to_string()),
            ("month", &month.to_string()),
            ("channel_id", channel),
        ])
        .send()
        .await
        .ok();
    let types = match response {
        Some(response) => response.json::<UpdateTypes>().await.unwrap_or_default(),
        None => UpdateTypes::default(),
    };

    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, types.clone());
    }
    types
}

async fn fetch_country_name(marker: &Marker, osm_country_name: Option<&str>) -> String {
    if marker.generation.as_deref() == Some("ari") {
        return osm_country_name.unwrap_or("Unknown").to_string();
    }
    let code = report_country(marker);
    if code.is_empty() {
        return "Unknown".to_string();
    }
    let code = code.trim().to_ascii_uppercase();

    let cache = COUNTRY_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(name) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&code).cloned())
    {
        return name;
    }

    let response = http_client().get(COUNTRY_URL).send().await.ok();
    let name = match response {
        Some(response) => response
            .json::<Value>()
            .await
            .ok()
            .and_then(|countries| {
                countries
                    .get(&code)
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Unknown".to_string()),
        None => "Unknown".to_string(),
    };

    if let Ok(mut cache) = cache.lock() {
        cache.insert(code, name.clone());
    }
    name
}

fn format_date(image_date: Option<&str>) -> Option<String> {
    let (year, month) = parse_month(image_date?)?;
    let month = FULL_MONTHS.get((month - 1) as usize)?;
    Some(format!("{} {}", month, year))
}

fn is_small_cam_country(code: &str) -> bool {
    let code = code.trim().to_ascii_uppercase();
    SMALL_CAM_COUNTRIES.contains(&code.as_str())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn place_description(marker: &Marker, country: &str) -> String {
    let locality = non_empty(marker.locality.as_deref());
    let road = non_empty(marker.road.as_deref());
    let region = marker.region.as_deref().unwrap_or("");
    let primary = locality.or(road).unwrap_or("");

    let position_word = match (locality, road) {
        (Some(locality), Some(road)) if locality == road => "on",
        (Some(_), _) => "in",
        (None, Some(_)) => "on",
        (None, None) => "in",
    };

    let separator = if locality.or(road).is_some() {
        ", "
    } else {
        ""
    };
    format!(
        "{} {}{}{}, {}",
        position_word, primary, separator, region, country
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_userscript_report_shape() {
        let marker = Marker {
            pano_id: "ULqC3DgIlj0HMfDpUY6sZw".into(),
            lat: 42.464,
            lng: 20.424,
            zoom: 0.0,
            heading: 0.0,
            pitch: 0.0,
            country: Some("xk".into()),
            locality: Some("Gjilan".into()),
            region: Some("Gjilan".into()),
            road: Some("Rrugë Fadil Jakupi".into()),
            image_date: Some("2025-07".into()),
            generation: Some("Gen4".into()),
            ..Marker::default()
        };

        assert_eq!(
            format_report(
                &marker,
                &UpdateTypes::default(),
                Some("https://maps.app.goo.gl/LsJJqnfP8wNmkDeg7"),
                "Unknown",
            ),
            ":flag_xk: :newroad: :smallcam: July 2025 in Gjilan, Gjilan, Unknown \
             https://maps.app.goo.gl/LsJJqnfP8wNmkDeg7"
        );
    }

    #[test]
    fn uses_previous_generation_and_current_smallcam() {
        let marker = Marker {
            pano_id: "abc".into(),
            country: Some("IN".into()),
            image_date: Some("2024-01-15".into()),
            has_history: true,
            generation: Some("Gen4".into()),
            previous_generation: Some("Gen1".into()),
            ..Marker::default()
        };

        assert_eq!(
            format_report(&marker, &UpdateTypes::default(), None, "India"),
            ":flag_in: :gen1update: :smallcam: January 2024 in , India \
             https://www.google.com/maps/@?api=1&map_action=pano&pano=abc"
        );
    }

    #[test]
    fn adds_api_types_in_userscript_order() {
        let marker = Marker {
            pano_id: "abc".into(),
            country: Some("US".into()),
            region: Some("California".into()),
            image_date: Some("2024-01".into()),
            has_history: true,
            generation: Some("Gen3".into()),
            previous_generation: Some("Gen4".into()),
            ..Marker::default()
        };
        let types = UpdateTypes {
            newyear: true,
            newcountry: true,
            newregion: true,
        };

        assert_eq!(
            format_report(&marker, &types, None, "United States"),
            ":flag_us: :newyear: :newcountry: :newregion: January 2024 in California, United States \
             https://www.google.com/maps/@?api=1&map_action=pano&pano=abc"
        );
    }

    #[test]
    fn uses_on_when_road_matches_locality() {
        let marker = Marker {
            pano_id: "abc".into(),
            country: Some("FR".into()),
            locality: Some("Paris".into()),
            region: Some("Île-de-France".into()),
            road: Some("Paris".into()),
            image_date: Some("2024-01".into()),
            has_history: true,
            ..Marker::default()
        };

        assert!(place_description(&marker, "France").starts_with("on Paris, "));
    }
}
