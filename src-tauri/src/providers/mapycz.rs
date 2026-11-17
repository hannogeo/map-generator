//! Mapy.cz panoramas.
//!
//! Uses the public v1 API when the user configured an API key, and the
//! XML-RPC endpoint otherwise, exactly like the reference project.

use super::{as_f64, as_str, field, get_json, post_json, FetchResult, PanoRequest, Providers};
use crate::dates::epoch_seconds_to_iso;
use crate::geo::opk_to_hpr;
use crate::models::{Link, Pano, TimeEntry};
use serde_json::Value;

const XMLRPC: &str = "https://pro.mapy.cz/panorpc";
const V1: &str = "https://api.mapy.cz/v1/panorama";

enum XmlValue<'a> {
    Int(i64),
    Double(f64),
    Struct(Vec<(&'a str, XmlValue<'a>)>),
}

impl<'a> XmlValue<'a> {
    fn render(&self) -> String {
        match self {
            XmlValue::Int(v) => format!("<int>{v}</int>"),
            XmlValue::Double(v) => format!("<double>{v}</double>"),
            XmlValue::Struct(members) => {
                let mut out = String::from("<struct>");
                for (key, value) in members {
                    out.push_str(&format!(
                        "<member><name>{}</name><value>{}</value></member>",
                        key,
                        value.render()
                    ));
                }
                out.push_str("</struct>");
                out
            }
        }
    }
}

fn xml_call(method: &str, params: &[XmlValue]) -> String {
    let mut body = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?><methodCall>");
    body.push_str(&format!("<methodName>{method}</methodName><params>"));
    for param in params {
        body.push_str(&format!("<param><value>{}</value></param>", param.render()));
    }
    body.push_str("</params></methodCall>");
    body
}

async fn rpc(providers: &Providers, body: &str) -> Option<Value> {
    let headers = [("Accept", "application/json"), ("Content-Type", "text/xml")];
    if let Some(value) = post_json(providers, XMLRPC, body, &headers).await {
        if field(&value, "result").is_null() && field(&value, "failure").is_null() {
            // Not a valid XML-RPC answer; fall through to the proxy.
        } else {
            return Some(value);
        }
    }
    let proxied = format!("https://cors-proxy.ac4.stocc.dev/{XMLRPC}");
    post_json(providers, &proxied, body, &headers).await
}

pub async fn fetch(providers: &Providers, req: &PanoRequest) -> FetchResult {
    let api_key = providers.mapycz_key();

    let (result, pano_id) = match req.pano.as_deref() {
        Some(id) => {
            let Some(result) = load_detail(providers, id, &api_key).await else {
                return FetchResult::zero();
            };
            (result, id.to_string())
        }
        None => {
            let (Some(lat), Some(lng)) = (req.lat, req.lng) else {
                return FetchResult::error();
            };
            let radius = if req.radius > 0.0 { req.radius } else { 50.0 };
            let Some(mut result) = load_best(providers, lng, lat, radius, &api_key).await else {
                return FetchResult::zero();
            };
            if !field(&result, "panInfo").is_null() {
                result = field(&result, "panInfo").clone();
            }
            let Some(id) = super::as_string(field(&result, "pid")) else {
                return FetchResult::zero();
            };
            (result, id)
        }
    };

    let neighbours = load_neighbours(providers, &pano_id, &api_key)
        .await
        .unwrap_or(Value::Null);
    match parse(&result, &neighbours, &pano_id) {
        Some(pano) => FetchResult::ok(pano),
        None => FetchResult::zero(),
    }
}

async fn load_best(
    providers: &Providers,
    lng: f64,
    lat: f64,
    radius: f64,
    api_key: &str,
) -> Option<Value> {
    if !api_key.is_empty() {
        let url = format!(
            "{V1}/getbest?lon={lng}&lat={lat}&radius={radius}&apikey={api_key}"
        );
        return get_json(providers, &url).await;
    }
    let body = xml_call(
        "getbest",
        &[
            XmlValue::Double(lng),
            XmlValue::Double(lat),
            XmlValue::Double(radius),
            XmlValue::Struct(vec![("newest", XmlValue::Int(1))]),
        ],
    );
    let response = rpc(providers, &body).await?;
    let result = field(&response, "result");
    if result.is_null() {
        None
    } else {
        Some(result.clone())
    }
}

async fn load_detail(providers: &Providers, id: &str, api_key: &str) -> Option<Value> {
    if !api_key.is_empty() {
        let url = format!("{V1}/detail?id={id}&apikey={api_key}");
        return get_json(providers, &url).await;
    }
    let Ok(pid) = id.parse::<i64>() else {
        return None;
    };
    let body = xml_call("detail", &[XmlValue::Int(pid)]);
    let response = rpc(providers, &body).await?;
    let result = field(&response, "result");
    if result.is_null() {
        None
    } else {
        Some(result.clone())
    }
}

async fn load_neighbours(providers: &Providers, id: &str, api_key: &str) -> Option<Value> {
    if !api_key.is_empty() {
        let url = format!("{V1}/getneighbours?pid={id}&apikey={api_key}");
        let response = get_json(providers, &url).await?;
        return Some(field(&response, "neighbours").clone());
    }
    let Ok(pid) = id.parse::<i64>() else {
        return None;
    };
    let body = xml_call("getneighbours", &[XmlValue::Int(pid)]);
    let response = rpc(providers, &body).await?;
    let result = field(&response, "result");
    Some(field(result, "neighbours").clone())
}

fn parse(result: &Value, neighbours: &Value, pano_id: &str) -> Option<Pano> {
    let mark = field(result, "mark");
    let lat = as_f64(field(mark, "lat"))?;
    let lng = as_f64(field(mark, "lon"))?;
    let provider = as_str(field(result, "provider")).unwrap_or("MapyCZ");

    let omega = as_f64(field(result, "omega")).unwrap_or(0.0);
    let phi = as_f64(field(result, "phi")).unwrap_or(0.0);
    let kappa = as_f64(field(result, "kappa")).unwrap_or(0.0);
    let (heading, _pitch, _roll) = opk_to_hpr(omega, phi, kappa);
    let car_direction = as_f64(field(field(result, "extra"), "carDirection"));
    let center_heading = car_direction.unwrap_or(heading);

    let created_ts = as_f64(field(result, "createdAtTimestamp"));
    let created = field(result, "createdAt");
    let image_date = match created_ts {
        Some(ts) if ts > 0.0 => match created.as_str() {
            Some(text) => Some(text.to_string()),
            None => Some(epoch_seconds_to_iso(ts)),
        },
        _ => as_f64(created)
            .map(epoch_seconds_to_iso)
            .or_else(|| created.as_str().map(|s| s.to_string())),
    };

    let mut links = Vec::new();
    if let Some(list) = neighbours.as_array() {
        for neighbour in list {
            let near = field(neighbour, "near");
            let far = field(neighbour, "far");
            let pano = super::as_string(field(near, "pid"))
                .or_else(|| super::as_string(field(far, "pid")));
            let Some(pano) = pano else { continue };
            let description = as_str(field(near, "provider"))
                .or_else(|| as_str(field(far, "provider")))
                .unwrap_or("MapyCZ");
            links.push(Link {
                pano,
                heading: as_f64(field(neighbour, "angle")).unwrap_or(0.0),
                description: description.to_string(),
            });
        }
    }

    let mut time = Vec::new();
    if let Some(date) = &image_date {
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
        description: Some(provider.to_string()),
        short_description: None,
        image_date,
        copyright: Some("© MapyCZ".to_string()),
        service: Some(provider.to_string()),
        altitude: as_f64(field(mark, "alt")),
        world_width: 8192,
        world_height: 4096,
        center_heading,
        links,
        time,
        provider: "mapycz".to_string(),
    })
}
