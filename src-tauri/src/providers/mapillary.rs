//! Mapillary street-level imagery.

use super::{as_f64, as_str, field, get_json, idx, FetchResult, PanoRequest, Providers};
use crate::dates::epoch_ms_to_iso;
use crate::models::{Pano, TimeEntry};

const TOKEN: &str = "MLY|4756369651124824|daee50b6cb15570a90b6a151bbd97bf3";
const FIELDS: &str = "id,computed_geometry,computed_altitude,geometry,captured_at,is_pano,make,model,thumb_2048_url,compass_angle,computed_compass_angle,sequence,mesh,merge_cc";

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let by_pano = req.pano.is_some();
    let image = match req.pano.as_deref() {
        Some(id) => {
            let url = format!("https://graph.mapillary.com/{id}?access_token={TOKEN}&fields={FIELDS}");
            match get_json(providers, &url).await {
                Some(value) if !field(&value, "id").is_null() => value,
                Some(_) => return FetchResult::zero(),
                None => return FetchResult::error(),
            }
        }
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
            let lat_rad = lat.to_radians();
            let delta_lat = radius / 1000.0 / 110.54;
            let delta_lng = radius / 1000.0 / (110.54 * lat_rad.cos().max(1e-6));
            // Mapillary rejects oversized bounding boxes ("Please reduce the
            // amount of data you're asking for"), so clamp each half-extent.
            const MAX_HALF_DEG: f64 = 0.006;
            let delta_lat = delta_lat.min(MAX_HALF_DEG);
            let delta_lng = delta_lng.min(MAX_HALF_DEG);
            let bbox = format!(
                "{},{},{},{}",
                lng - delta_lng,
                lat - delta_lat,
                lng + delta_lng,
                lat + delta_lat
            );
            let url = format!(
                "https://graph.mapillary.com/images?access_token={TOKEN}&bbox={bbox}&limit=1&fields={FIELDS}"
            );
            let Some(value) = get_json(providers, &url).await else {
                return FetchResult::error();
            };
            match idx(field(&value, "data"), 0).clone() {
                value if !value.is_null() => value,
                _ => return FetchResult::zero(),
            }
        }
    };

    match parse(&image, by_pano) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(image: &serde_json::Value, by_pano: bool) -> Option<Pano> {
    let id = as_str(field(image, "id"))?.to_string();
    let coords = {
        let computed = field(field(image, "computed_geometry"), "coordinates");
        if computed.is_array() {
            computed
        } else {
            field(field(image, "geometry"), "coordinates")
        }
    };
    let lng = as_f64(idx(coords, 0))?;
    let lat = as_f64(idx(coords, 1))?;

    let date = match as_f64(field(image, "captured_at")) {
        Some(ms) => epoch_ms_to_iso(ms as i64),
        None => epoch_ms_to_iso(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        ),
    };

    let make = as_str(field(image, "make")).unwrap_or("");
    let model = as_str(field(image, "model")).unwrap_or("");
    let description = format!("{make} - {model}").trim().to_string();

    let heading = as_f64(field(image, "computed_compass_angle"))
        .or_else(|| as_f64(field(image, "compass_angle")))
        .unwrap_or(0.0);

    let time = if by_pano {
        Vec::new()
    } else {
        vec![TimeEntry {
            pano: id.clone(),
            date: date.clone(),
        }]
    };

    Some(Pano {
        pano_id: id,
        lat,
        lng,
        country: None,
        region: None,
        locality: None,
        road: None,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        short_description: None,
        image_date: Some(date),
        copyright: Some("© Mapillary".to_string()),
        service: Some(if field(image, "is_pano").as_bool().unwrap_or(false) {
            "true".to_string()
        } else {
            "false".to_string()
        }),
        altitude: as_f64(field(image, "computed_altitude")),
        world_width: 2048,
        world_height: 1024,
        center_heading: heading,
        links: Vec::new(),
        time,
        provider: "mapillary".to_string(),
    })
}
