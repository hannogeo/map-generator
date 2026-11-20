//! Street-level imagery providers.
//!
//! Every provider is implemented against its real, keyless endpoint. The
//! parsers mirror the browser reference project so that filters, exports and
//! update-type detection keep working the same way. All network access lives
//! here; nothing outside this module knows about provider specifics.

pub mod apple;
pub mod baidu;
pub mod bing;
pub mod google;
pub mod ja;
pub mod kakao;
pub mod mapillary;
pub mod mapycz;
pub mod naver;
pub mod openmap;
pub mod tencent;
pub mod vegbilder;
pub mod yandex;
pub mod asig;

use crate::models::Pano;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    Ok,
    ZeroResults,
    UnknownError,
}

#[derive(Debug, Clone)]
pub struct FetchResult {
    pub status: FetchStatus,
    pub pano: Option<Pano>,
}

impl FetchResult {
    pub fn ok(pano: Pano) -> Self {
        Self {
            status: FetchStatus::Ok,
            pano: Some(pano),
        }
    }

    pub fn zero() -> Self {
        Self {
            status: FetchStatus::ZeroResults,
            pano: None,
        }
    }

    pub fn error() -> Self {
        Self {
            status: FetchStatus::UnknownError,
            pano: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PanoRequest {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub radius: f64,
    pub pano: Option<String>,
    /// `rejectUnofficial` in the UI: restrict Google's search to official coverage.
    pub official_only: bool,
}

impl PanoRequest {
    pub fn by_location(lat: f64, lng: f64, radius: f64, official_only: bool) -> Self {
        Self {
            lat: Some(lat),
            lng: Some(lng),
            radius,
            pano: None,
            official_only,
        }
    }

    pub fn by_pano(pano: impl Into<String>) -> Self {
        Self {
            pano: Some(pano.into()),
            radius: 50.0,
            official_only: false,
            ..Default::default()
        }
    }
}

const CACHE_LIMIT: usize = 6000;

pub struct Providers {
    pub client: reqwest::Client,
    cache: Mutex<HashMap<String, Pano>>,
    pub apple_tiles: Mutex<HashMap<(u32, u32), Vec<apple::AppleLocation>>>,
    /// Cached z17 photometa tile IDs with their fetch time. Entries expire so
    /// coverage released mid-run can still be picked up.
    pub google_zoom: Mutex<HashMap<(u32, u32), (std::time::Instant, Vec<String>)>>,
    pub openmap_tiles: Mutex<HashMap<(u32, u32), Vec<openmap::OpenMapFeature>>>,
    pub asig_tiles: Mutex<HashMap<(u32, u32), Vec<asig::AsigFeature>>>,
    pub vegbilder: Mutex<HashMap<String, Vec<vegbilder::VegbilderFeature>>>,
    /// Alpha channels of overlay tiles, keyed by `(variant, zoom, x, y)`.
    pub line_tiles: Mutex<HashMap<(u8, u8, u32, u32), std::sync::Arc<Vec<u8>>>>,
    pub mapycz_key: Mutex<String>,
}

impl Default for Providers {
    fn default() -> Self {
        Self::new()
    }
}

impl Providers {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            // Generation keeps up to 128 requests in flight; keep their
            // connections warm instead of reconnecting for every sample.
            .pool_max_idle_per_host(160)
            .tcp_nodelay(true)
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
            )
            .build()
            .unwrap_or_default();
        Self {
            client,
            cache: Mutex::new(HashMap::new()),
            apple_tiles: Mutex::new(HashMap::new()),
            google_zoom: Mutex::new(HashMap::new()),
            openmap_tiles: Mutex::new(HashMap::new()),
            asig_tiles: Mutex::new(HashMap::new()),
            vegbilder: Mutex::new(HashMap::new()),
            line_tiles: Mutex::new(HashMap::new()),
            mapycz_key: Mutex::new(String::new()),
        }
    }

    pub fn set_mapycz_key(&self, key: &str) {
        if let Ok(mut slot) = self.mapycz_key.lock() {
            *slot = key.trim().to_string();
        }
    }

    pub fn mapycz_key(&self) -> String {
        self.mapycz_key
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    fn cache_get(&self, provider: &str, id: &str) -> Option<Pano> {
        let cache = self.cache.lock().ok()?;
        cache.get(&format!("{provider}:{id}")).cloned()
    }

    fn cache_set(&self, pano: &Pano) {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(format!("{}:{}", pano.provider, pano.pano_id), pano.clone());
        }
    }

    pub fn cache_pano(&self, pano: Pano) {
        self.cache_set(&pano);
    }

    pub fn lookup_cached(&self, provider: &str, id: &str) -> Option<Pano> {
        self.cache_get(provider, id)
    }

    pub async fn fetch(&self, provider: &str, req: PanoRequest) -> FetchResult {
        if let Some(id) = req.pano.as_deref() {
            if let Some(pano) = self.cache_get(provider, id) {
                return FetchResult::ok(pano);
            }
        }
        let result = match provider {
            "google" | "googleZoom" => google::fetch(self, &req, provider == "googleZoom").await,
            "apple" => apple::fetch(self, &req).await,
            "yandex" => yandex::fetch(self, &req).await,
            "tencent" => tencent::fetch(self, &req).await,
            "bing" => bing::fetch(self, &req).await,
            "kakao" => kakao::fetch(self, &req).await,
            "naver" => naver::fetch(self, &req).await,
            "baidu" => baidu::fetch(self, &req).await,
            "mapycz" => mapycz::fetch(self, &req).await,
            "mapillary" => mapillary::fetch(self, &req).await,
            "openmap" => openmap::fetch(self, &req).await,
            "asig" => asig::fetch(self, &req).await,
            "ja" => ja::fetch(self, &req).await,
            "vegbilder" => vegbilder::fetch(self, &req).await,
            _ => FetchResult::error(),
        };
        if let Some(pano) = &result.pano {
            self.cache_set(pano);
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub const NULL: Value = Value::Null;

pub fn idx(value: &Value, index: usize) -> &Value {
    value.get(index).unwrap_or(&NULL)
}

pub fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&NULL)
}

pub fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

pub fn as_str(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}

pub fn as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub async fn get_text(providers: &Providers, url: &str) -> Option<String> {
    let response = providers.client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().await.ok()
}

pub async fn get_json(providers: &Providers, url: &str) -> Option<Value> {
    let text = get_text(providers, url).await?;
    serde_json::from_str(&text).ok()
}

/// Some endpoints only answer cross-origin requests through this proxy. A
/// plain Rust client can usually talk to them directly, so try direct first.
pub async fn get_json_proxied(providers: &Providers, url: &str) -> Option<Value> {
    if let Some(value) = get_json(providers, url).await {
        return Some(value);
    }
    let proxied = format!("https://cors-proxy.ac4.stocc.dev/{url}");
    get_json(providers, &proxied).await
}

pub async fn get_bytes(
    providers: &Providers,
    url: &str,
    headers: &[(&str, &str)],
) -> Option<Vec<u8>> {
    let mut request = providers.client.get(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|b| b.to_vec())
}

pub async fn post_json(
    providers: &Providers,
    url: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> Option<Value> {
    let mut request = providers.client.post(url).body(body.to_string());
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().await.ok()?;
    serde_json::from_str(&text).ok()
}

/// Formats a month from a two element `[year, month]` array.
pub fn month_string(year: &Value, month: &Value) -> Option<String> {
    let y = year.as_i64()?;
    let m = month.as_i64()?;
    if !(1..=12).contains(&m) || !(1900..=2200).contains(&y) {
        return None;
    }
    Some(format!("{:04}-{:02}", y, m))
}
