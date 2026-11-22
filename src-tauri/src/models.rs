use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A panorama link (arrow) to another panorama.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub pano: String,
    #[serde(default)]
    pub heading: f64,
    #[serde(default)]
    pub description: String,
}

/// One entry of a panorama's history.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntry {
    pub pano: String,
    /// ISO date, usually `YYYY-MM` or `YYYY-MM-DD`.
    pub date: String,
}

/// One panorama point from a Google photometa tile. Entries without a
/// resolvable pano id still have coordinates and are useful for the close-zoom
/// coverage dots, so the id may be empty.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PanoDot {
    pub pano_id: String,
    pub lat: f64,
    pub lng: f64,
}

/// Dots grouped by the z17 tile they came from, so the UI can cache each tile
/// independently as the viewport moves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanoDotTile {
    pub x: u32,
    pub y: u32,
    pub dots: Vec<PanoDot>,
}

/// Provider independent panorama metadata. Field names mirror the browser
/// reference implementation so that filters and exports stay compatible.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pano {
    pub pano_id: String,
    pub lat: f64,
    pub lng: f64,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub locality: Option<String>,
    #[serde(default)]
    pub road: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub image_date: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
    /// Some providers expose a numeric camera/service id, others a string.
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub altitude: Option<f64>,
    #[serde(default)]
    pub world_width: u32,
    #[serde(default)]
    pub world_height: u32,
    #[serde(default)]
    pub center_heading: f64,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub time: Vec<TimeEntry>,
    #[serde(default)]
    pub provider: String,
}

impl Pano {
    pub fn has_any_description(&self) -> bool {
        self.description.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
            || self
                .short_description
                .as_deref()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
    }

    pub fn service_truthy(&self) -> bool {
        match self.service.as_deref() {
            Some(s) => {
                let s = s.trim();
                !s.is_empty() && s != "false" && s != "0" && s != "null"
            }
            None => false,
        }
    }

    pub fn is_photosphere(&self) -> bool {
        self.links.is_empty()
    }

    pub fn is_drone(&self) -> bool {
        self.is_photosphere() && (self.world_height == 2048 || self.world_height == 7200)
    }
}

/// The location object handed to the frontend and to exports.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub pano_id: String,
    pub lat: f64,
    pub lng: f64,
    pub heading: f64,
    pub pitch: f64,
    pub zoom: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_date: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_type: Option<String>,
    /// Camera generation of the current pano, in the userscript's notation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
    /// Camera generation of the previous pano, used for update tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_generation: Option<String>,
    /// Whether the pano has any older capture in its history.
    #[serde(default)]
    pub has_history: bool,
    pub links: Vec<String>,
    pub extra: MarkerExtra,
    /// Only used client-side, never exported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MarkerExtra {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PolygonInput {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub geometry: Value,
    #[serde(default)]
    pub nb_needed: u32,
    #[serde(default)]
    pub found: Vec<Marker>,
}

/// Goal changes sent while a generation is running.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalUpdate {
    pub id: String,
    pub nb_needed: u32,
}

/// Shared, live-editable goals for one generation run.
pub type Goals = Arc<Mutex<HashMap<String, usize>>>;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub provider: String,
    pub radius: f64,
    pub speed: usize,
    pub strategy: String,
    /// Skip random points in open water. On by default for older saved settings.
    #[serde(default = "default_true")]
    pub use_land_mask: bool,
    pub num_of_generators: usize,
    pub reject_unofficial: bool,
    pub reject_official: bool,
    pub reject_road_name: bool,
    pub reject_dateless: bool,
    pub reject_no_description: bool,
    pub reject_description: bool,
    pub check_all_dates: bool,
    pub random_in_timeline: bool,
    pub select_months: bool,
    pub from_date: String,
    pub to_date: String,
    pub from_month: String,
    pub to_month: String,
    pub from_year: String,
    pub to_year: String,
    pub one_country_at_a_time: bool,
    pub only_check_blue_lines: bool,
    pub disable_check_blue_line: bool,
    pub find_regions: bool,
    pub region_radius: f64,
    pub ignore_badcam: bool,
    pub find_night_coverage: bool,
    pub only_one_in_timeframe: bool,
    pub check_links: bool,
    pub find_photospheres: bool,
    pub find_drones: bool,
    pub filter_by_minutes: ToggleRange<u32>,
    pub find_by_tile_color: TileColorConfig,
    pub filter_by_links_length: ToggleRange<u32>,
    pub filter_by_altitude: ToggleRange<f64>,
    pub get_curve: bool,
    pub min_curve_angle: f64,
    pub heading: HeadingConfig,
    pub pitch: AdjustRange,
    pub zoom: AdjustRange,
    pub search_in_description: SearchConfig,
    pub find_by_author: AuthorConfig,
    #[serde(default)]
    pub find_by_generation: Value,
    pub api_keys: ApiKeys,
    pub tag: bool,
    pub pano_id: String,
    #[serde(default)]
    pub notification: NotificationConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: "google".into(),
            radius: 500.0,
            speed: 100,
            strategy: "random".into(),
            use_land_mask: true,
            num_of_generators: 1,
            reject_unofficial: true,
            reject_official: false,
            reject_road_name: false,
            reject_dateless: true,
            reject_no_description: true,
            reject_description: false,
            check_all_dates: false,
            random_in_timeline: false,
            select_months: false,
            from_date: "2009-01".into(),
            to_date: crate::dates::current_month(),
            from_month: "01".into(),
            to_month: "12".into(),
            from_year: "2007".into(),
            to_year: crate::dates::current_year().to_string(),
            one_country_at_a_time: false,
            only_check_blue_lines: false,
            disable_check_blue_line: false,
            find_regions: false,
            region_radius: 100.0,
            ignore_badcam: false,
            find_night_coverage: false,
            only_one_in_timeframe: false,
            check_links: false,
            find_photospheres: false,
            find_drones: false,
            filter_by_minutes: ToggleRange::default(),
            find_by_tile_color: TileColorConfig::default(),
            filter_by_links_length: ToggleRange::default(),
            filter_by_altitude: ToggleRange::default(),
            get_curve: false,
            min_curve_angle: 10.0,
            heading: HeadingConfig::default(),
            pitch: AdjustRange::default(),
            zoom: AdjustRange::default(),
            search_in_description: SearchConfig::default(),
            find_by_author: AuthorConfig::default(),
            find_by_generation: serde_json::json!({
        "enabled": true,
        "google": { "1": false, "2": true, "3": true, "23": true, "4": true, "badcam": true },
        "apple": { "bigcam": true, "smallcam": true, "backpack": true },
        "bing": { "3": true, "4": true, "5": true },
        "naver": { "1": true, "3": true, "13": true, "15": true },
        "yandex": { "1": true, "2": true, "trekker": false },
        "mapillary": { "1": true, "2": true }
    }),
            api_keys: ApiKeys::default(),
            tag: true,
            pano_id: "enable".into(),
            notification: NotificationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ToggleRange<T> {
    pub enabled: bool,
    pub range: [T; 2],
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HeadingConfig {
    pub adjust: bool,
    pub reference: String,
    pub range: [f64; 2],
    pub random_in_range: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AdjustRange {
    pub adjust: bool,
    pub range: [f64; 2],
    pub random_in_range: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchConfig {
    pub enabled: bool,
    pub search_terms: String,
    pub search_mode: String,
    pub filter_type: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AuthorConfig {
    pub enabled: bool,
    pub author: String,
    pub filter_type: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ApiKeys {
    pub mapycz: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub any_location: bool,
    pub any_no_blue_line: bool,
    pub one_polygon_complete: bool,
    pub all_polygons_complete: bool,
    /// Where notifications go: "desktop", "discord", or "both".
    pub target: String,
    pub discord_webhook: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TileColorConfig {
    pub enabled: bool,
    pub zoom: u8,
    pub filter_type: String,
    pub operator: String,
    pub tile_provider: String,
    pub tile_colors: HashMap<String, Vec<TileColorGroup>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TileColorGroup {
    pub label: String,
    pub active: bool,
    pub threshold: f64,
    pub colors: Vec<String>,
}

impl Settings {
    pub fn find_by_generation_enabled(&self) -> bool {
        self.find_by_generation
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn effective_radius(&self) -> f64 {
        if self.radius > 0.0 {
            self.radius
        } else {
            50.0
        }
    }

    /// Whether a camera generation is allowed for the active provider.
    pub fn generation_allowed(&self, generation: &str) -> bool {
        let provider = if self.provider == "googleZoom" {
            "google"
        } else {
            self.provider.as_str()
        };
        self.find_by_generation
            .get(provider)
            .and_then(|v| v.get(generation))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

/// Event payloads.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationEvent {
    pub polygon_id: String,
    pub location: Marker,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub polygon_id: String,
    pub found: usize,
    pub needed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonDoneEvent {
    pub polygon_id: String,
    pub found: usize,
    pub needed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyEvent {
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Marker>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceResult {
    pub display_name: String,
    pub osm_id: i64,
    pub osm_type: Option<String>,
    pub addresstype: Option<String>,
    pub address: Option<Value>,
    pub lat: Option<String>,
    pub lon: Option<String>,
}
