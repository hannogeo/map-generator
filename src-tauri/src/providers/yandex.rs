//! Yandex Panoramas (keyless service endpoint).

use super::{
    as_f64, as_str, field, get_json, idx, FetchResult, PanoRequest, Providers,
};
use crate::dates::yandex_date_from_id;
use crate::models::{Link, Pano, TimeEntry};
use serde_json::Value;

const ENDPOINT: &str = "https://api-maps.yandex.com/services/panoramas/1.x/";

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let pano_id = match req.pano.as_deref() {
        Some(id) => id.to_string(),
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let url = format!(
                "{ENDPOINT}?l=stv&lang=en_US&origin=userAction&provider=streetview&ll={lng},{lat}"
            );
            let Some(json) = get_json(providers, &url).await else {
                return FetchResult::error();
            };
            match as_str(field(field(field(&json, "data"), "Data"), "panoramaId")) {
                Some(id) => id.to_string(),
                None => return FetchResult::zero(),
            }
        }
    };

    let url = format!(
        "{ENDPOINT}?l=stv&lang=en_US&origin=userAction&provider=streetview&oid={pano_id}"
    );
    let Some(json) = get_json(providers, &url).await else {
        return FetchResult::error();
    };
    let data = field(&json, "data");
    let point = field(data, "Data");
    if as_str(field(point, "panoramaId")).is_none() {
        return FetchResult::zero();
    }
    match parse(data, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

fn parse(data: &Value, pano_id: &str) -> Option<Pano> {
    let point = field(data, "Data");
    let coordinates = field(field(point, "Point"), "coordinates");
    let lng = as_f64(idx(coordinates, 0))?;
    let lat = as_f64(idx(coordinates, 1))?;
    let origin = field(field(point, "EquirectangularProjection"), "Origin");
    let heading = (as_f64(idx(origin, 0)).unwrap_or(0.0) + 180.0) % 360.0;

    let zooms = field(field(point, "Images"), "Zooms");
    let width = as_f64(field(idx(zooms, 0), "width"))
        .or_else(|| as_f64(idx(idx(zooms, 0), 0)))
        .unwrap_or(0.0) as u32;
    let height = as_f64(field(idx(zooms, 0), "height"))
        .or_else(|| as_f64(idx(idx(zooms, 0), 1)))
        .unwrap_or(0.0) as u32;

    let image_date = yandex_date_from_id(pano_id);
    let annotation = field(data, "Annotation");

    let mut links = Vec::new();
    if let Some(thoroughfares) = field(annotation, "Thoroughfares").as_array() {
        for thoroughfare in thoroughfares {
            let href = as_str(field(field(thoroughfare, "Connection"), "href"));
            if let Some(oid) = href.and_then(query_param) {
                links.push(Link {
                    pano: oid,
                    heading: 0.0,
                    description: String::new(),
                });
            }
        }
    }

    let mut time: Vec<TimeEntry> = Vec::new();
    if let Some(historical) = field(annotation, "HistoricalPanoramas").as_array() {
        for item in historical {
            let oid = as_str(field(field(item, "Connection"), "oid"))
                .or_else(|| as_str(field(field(item, "Connection"), "panoramaId")));
            let Some(oid) = oid else { continue };
            let date = yandex_date_from_id(oid).unwrap_or_default();
            time.push(TimeEntry {
                pano: oid.to_string(),
                date,
            });
        }
    }
    time.push(TimeEntry {
        pano: pano_id.to_string(),
        date: image_date.clone().unwrap_or_default(),
    });
    time.sort_by(|a, b| a.date.cmp(&b.date));

    let copyright = as_str(field(field(data, "Author"), "name"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "© Yandex Maps".to_string());

    Some(Pano {
        pano_id: pano_id.to_string(),
        lat,
        lng,
        country: None,
        region: None,
        locality: None,
        road: None,
        description: as_str(field(field(point, "Point"), "name")).map(|s| s.to_string()),
        short_description: None,
        image_date,
        copyright: Some(copyright),
        service: None,
        altitude: None,
        world_width: width,
        world_height: height,
        center_heading: heading,
        links,
        time,
        provider: "yandex".to_string(),
    })
}

fn query_param(url: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some("oid") {
            return parts.next().map(|s| s.trim().to_string());
        }
    }
    None
}
