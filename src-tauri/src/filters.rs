//! Panorama filtering.
//!
//! This mirrors the reference browser app's `getLoc`, `isPanoGood`,
//! `getPanoDeep` and `addLoc` functions. Everything here is provider neutral;
//! provider quirks live in their modules and in the small helpers below.

use crate::dates::{
    month_end_ms, month_start_ms, parse_flexible_ms, parse_month, parse_month_year,
};
use crate::geo::{accepts_curve, distance_between, random_in_range, LatLng, PolyRings};
use crate::models::{Marker, MarkerExtra, Pano, Settings, TimeEntry};
use crate::providers::{FetchResult, FetchStatus, PanoRequest, Providers};
use crate::tiles::{has_blue_line_at, tile_color_matches};
use futures::stream::{FuturesUnordered, StreamExt};
use rand::seq::SliceRandom;
use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::sync::Mutex;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// How far a walk may continue through panoramas that fail the filters.
/// Accepted panoramas reset the counter, so this only limits gaps caused by
/// rejected captures (old generations, wrong dates, etc.). The control was
/// removed from the UI, so use a generous fixed value that covers short gaps.
const LINKS_DEPTH: u32 = 5;

const GOOGLE_PROVIDERS: [&str; 2] = ["google", "googleZoom"];
const GENERATION_PROVIDERS: [&str; 7] = [
    "google",
    "googleZoom",
    "apple",
    "yandex",
    "bing",
    "naver",
    "mapillary",
];
const ALTITUDE_PROVIDERS: [&str; 9] = [
    "google",
    "googleZoom",
    "baidu",
    "mapillary",
    "bing",
    "naver",
    "mapycz",
    "asig",
    "tencent",
];
const GEN2_COUNTRIES: [&str; 34] = [
    "AU", "BR", "CA", "CL", "JP", "GB", "IE", "NZ", "MX", "RU", "US", "IT", "DK", "GR", "RO",
    "PL", "CZ", "CH", "SE", "FI", "BE", "LU", "NL", "ZA", "SG", "TW", "HK", "MO", "MC", "SM",
    "AD", "IM", "JE", "FR",
];
const GEN2_COUNTRIES_2: [&str; 4] = ["DE", "ES", "PT", "SJ"];

fn gen3_date(country: &str) -> Option<&'static str> {
    match country {
        "BD" => Some("2021-04"),
        "FI" => Some("2020-09"),
        "IN" => Some("2021-10"),
        "LK" => Some("2021-02"),
        "LB" => Some("2021-05"),
        "NG" => Some("2021-06"),
        "US" => Some("2019-01"),
        "VN" => Some("2021-01"),
        _ => None,
    }
}

/// `year * 12 + month`, matching the reference's `dateToMonthNumber`.
fn month_number(value: &str) -> Option<i64> {
    let (year, month) = parse_month(value)?;
    Some(year as i64 * 12 + month as i64)
}

/// Reference `isOfficial`.
pub fn is_official(pano: &str, provider: &str) -> bool {
    match provider {
        "google" | "googleZoom" => pano.len() == 22,
        "mapycz" | "yandex" | "apple" | "bing" | "baidu" | "naver" | "kakao" | "mapillary"
        | "openmap" | "tencent" | "vegbilder" | "asig" | "ja" => true,
        _ => false,
    }
}

/// Reference `getCameraGeneration`, returned as a settings-map key.
pub fn camera_generation(pano: &Pano, provider: &str) -> String {
    if GOOGLE_PROVIDERS.contains(&provider) {
        return match pano.world_height {
            8192 => "4".to_string(),
            1664 => "1".to_string(),
            6656 => {
                if pano.service.as_deref() == Some("launch") {
                    let country = pano.country.as_deref().unwrap_or("None");
                    let target = gen3_date(country).unwrap_or("9999-99");
                    let image = month_number(pano.image_date.as_deref().unwrap_or(""));
                    if let Some(image) = image {
                        let january_2022 = month_number("2022-01").unwrap_or(i64::MAX);
                        let target = month_number(target).unwrap_or(i64::MAX);
                        if image >= january_2022
                            || (image >= target && (country != "US" || pano.lat > 52.0))
                        {
                            return "badcam".to_string();
                        }
                    }
                }
                let country = pano.country.as_deref().unwrap_or("None");
                let image = month_number(pano.image_date.as_deref().unwrap_or(""));
                let gen2_country = GEN2_COUNTRIES.contains(&country)
                    || GEN2_COUNTRIES_2.contains(&country);
                if gen2_country {
                    if let Some(image) = image {
                        if image <= month_number("2011-11").unwrap_or(i64::MIN) {
                            return if image >= month_number("2010-09").unwrap_or(i64::MAX) {
                                "23".to_string()
                            } else {
                                "2".to_string()
                            };
                        }
                    }
                }
                "3".to_string()
            }
            _ => "0".to_string(),
        };
    }

    if provider == "mapillary" {
        return if pano.service_truthy() { "1" } else { "2" }.to_string();
    }

    if let Some(service) = pano.service.as_deref() {
        if provider == "naver" && service.parse::<f64>().ok() == Some(2.0) {
            return "1".to_string();
        }
        return service.to_string();
    }

    if provider == "yandex" {
        return if pano.world_width == 0 {
            "0".to_string()
        } else if pano.world_width >= 17664 {
            "2".to_string()
        } else if pano.world_width == 5632 {
            "1".to_string()
        } else {
            "trekker".to_string()
        };
    }

    "0".to_string()
}

fn report_month(image_date: &str) -> Option<String> {
    let (year, month) = parse_month(image_date)?;
    Some(format!("{:04}-{:02}", year, month))
}

/// Camera generation in the notation used by the StreetView Toolkit's
/// `getGeneration`, which drives VirtualStreets report tags.
pub fn report_generation(pano: &Pano, provider: &str) -> String {
    if !GOOGLE_PROVIDERS.contains(&provider) {
        return "ari".to_string();
    }

    let country = pano.country.as_deref().unwrap_or("");
    let image = pano.image_date.as_deref().and_then(report_month);

    let generation = match pano.world_height {
        8192 => "Gen4",
        1664 => "Gen1",
        6656 => {
            if let Some(image) = image.as_deref() {
                if image >= "2022-01" {
                    "BadCam"
                } else if gen3_date(country).is_some_and(|target| {
                    image >= target && (country != "US" || pano.lat > 52.0)
                }) {
                    "BadCam"
                } else {
                    let gen2_country = GEN2_COUNTRIES.contains(&country)
                        || GEN2_COUNTRIES_2.contains(&country)
                        || country == "NO";
                    if gen2_country && image <= "2011-11" {
                        if image >= "2010-09" { "Gen2/3" } else { "Gen2" }
                    } else {
                        "Gen3"
                    }
                }
            } else {
                "Gen3"
            }
        }
        _ => "ari",
    };

    generation.to_string()
}

/// `normalizeText` from the reference: lowercase, strip accents and
/// punctuation (hyphens and whitespace survive).
pub fn normalize_text(text: &str) -> String {
    let lowered: String = text.nfd().filter(|c| !is_combining_mark(*c)).collect();
    let mut out = String::with_capacity(lowered.len());
    for ch in lowered.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_alphanumeric() || ch.is_whitespace() || ch == '-' || ch == '_' {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '_' | ',' | '.' | ';' | '!' | '?' | '(' | ')' | '\'' | '"' | '“' | '”' | '«'
                    | '»'
            )
    })
    .filter(|part| !part.is_empty())
    .map(|part| part.to_string())
    .collect()
}

fn contains_word(phrase: &str, term: &str) -> bool {
    if term.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(offset) = phrase[start..].find(term) {
        let index = start + offset;
        let before_ok = index == 0
            || !phrase[..index]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        let after = index + term.len();
        let after_ok = after >= phrase.len()
            || !phrase[after..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        start = index + term.len().max(1);
        if start >= phrase.len() {
            break;
        }
    }
    false
}

fn section_match(text: &str, term: &str) -> bool {
    let normalized: String = text
        .to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect();
    normalized
        .split(',')
        .any(|section| section.trim() == term.trim())
}

/// Reference `searchInDescription`.
pub fn search_in_description(pano: &Pano, settings: &Settings) -> bool {
    let config = &settings.search_in_description;
    if !config.enabled || config.search_terms.trim().is_empty() {
        return true;
    }
    let terms: Vec<String> = config
        .search_terms
        .split(',')
        .map(|term| normalize_text(term.trim()))
        .filter(|term| !term.is_empty())
        .collect();
    if terms.is_empty() {
        return true;
    }

    let description = pano.description.as_deref().unwrap_or("");
    let short = pano.short_description.as_deref().unwrap_or("");
    let combined = format!("{description} {short}");
    let normalized = normalize_text(&combined);
    let words: Vec<String> = tokenize(&combined)
        .into_iter()
        .map(|word| normalize_text(&word))
        .collect();
    let phrase = words.join(" ");

    let matched = terms.iter().any(|term| match config.search_mode.as_str() {
        "fullword" => contains_word(&phrase, term),
        "startswith" => words.iter().any(|word| word.starts_with(term)),
        "endswith" => words.iter().any(|word| word.ends_with(term)),
        "sectionmatch" => section_match(description, term) || section_match(short, term),
        _ => normalized.contains(term),
    });

    if config.filter_type.eq_ignore_ascii_case("exclude") {
        !matched
    } else {
        matched
    }
}

fn time_entry_ms(entry: &TimeEntry) -> Option<i64> {
    parse_flexible_ms(&entry.date)
}

fn pano_ms(pano: &Pano) -> Option<i64> {
    pano.image_date.as_deref().and_then(parse_flexible_ms)
}

/// One generation run's filter settings, with dates pre-computed.
pub struct Filters<'a> {
    pub settings: &'a Settings,
    pub providers: &'a Providers,
    pub rings: &'a PolyRings,
    from_ms: i64,
    to_ms: i64,
    from_month: u32,
    to_month: u32,
    from_year: i32,
    to_year: i32,
    /// The reference disables all-dates matching whenever region spacing is on.
    check_all_dates: bool,
}

impl<'a> Filters<'a> {
    pub fn new(settings: &'a Settings, providers: &'a Providers, rings: &'a PolyRings) -> Self {
        let from_ms = month_start_ms(&settings.from_date).unwrap_or(i64::MIN);
        let to_ms = month_end_ms(&settings.to_date).unwrap_or(i64::MAX);
        Self {
            settings,
            providers,
            rings,
            from_ms,
            to_ms,
            from_month: settings.from_month.parse().unwrap_or(1),
            to_month: settings.to_month.parse().unwrap_or(12),
            from_year: settings.from_year.parse().unwrap_or(2007),
            to_year: settings.to_year.parse().unwrap_or_else(|_| crate::dates::current_year()),
            check_all_dates: settings.check_all_dates && !settings.find_regions,
        }
    }

    pub fn in_polygon(&self, lat: f64, lng: f64) -> bool {
        self.rings.contains(lat, lng)
    }

    fn provider(&self) -> &str {
        &self.settings.provider
    }

    /// Generation gate that `is_pano_good` applies when the timeline is not
    /// being walked. `None` means the filter is inactive for this provider.
    fn generation_allows(&self, pano: &Pano) -> Option<bool> {
        let active = self.settings.find_by_generation_enabled()
            && ((!self.settings.reject_official && !self.check_all_dates)
                || self.settings.select_months)
            && GENERATION_PROVIDERS.contains(&self.provider());
        if !active {
            return None;
        }
        Some(
            self.settings
                .generation_allowed(&camera_generation(pano, self.provider())),
        )
    }

    /// Whether the panorama returned by the spatial search itself matches the
    /// date window and the generation filter.
    fn capture_matches(&self, pano: &Pano) -> bool {
        if let Some(date) = pano_ms(pano) {
            if date < self.from_ms || date > self.to_ms {
                return false;
            }
        }
        self.generation_allows(pano).unwrap_or(true)
    }

    /// Captures newer than `pano` that its timeline knows about and that fall
    /// inside the date window.
    ///
    /// Google's spatial search keeps pointing at the original capture of a
    /// road after a re-drive, so a fresh drop is only reachable through the
    /// older panorama's timeline. Following only entries newer than the
    /// current capture keeps the extra work limited to new drops.
    fn newer_captures(&self, pano: &Pano) -> Vec<String> {
        if self.settings.select_months {
            return Vec::new();
        }
        let Some(current) = pano_ms(pano) else {
            return Vec::new();
        };
        pano.time
            .iter()
            .filter(|entry| entry.pano != pano.pano_id)
            .filter(|entry| {
                !self.settings.reject_unofficial || is_official(&entry.pano, self.provider())
            })
            .filter_map(|entry| {
                let date = time_entry_ms(entry)?;
                (date > current && date >= self.from_ms && date <= self.to_ms)
                    .then(|| entry.pano.clone())
            })
            .collect()
    }

    /// Metadata endpoints occasionally fail transiently. Unlike a known empty
    /// result, an unknown error should not permanently burn the id in `claimed`.
    async fn fetch_by_pano(&self, id: &str) -> Option<Pano> {
        const TRIES: usize = 3;
        for attempt in 0..TRIES {
            match self
                .providers
                .fetch(self.provider(), PanoRequest::by_pano(id))
                .await
            {
                FetchResult {
                    status: FetchStatus::Ok,
                    pano: Some(pano),
                } => return Some(pano),
                FetchResult {
                    status: FetchStatus::UnknownError,
                    ..
                } if attempt + 1 < TRIES => {
                    let delay = 40 * (attempt as u64 + 1) + (rand::random::<u64>() % 40);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                _ => return None,
            }
        }
        None
    }

    /// Sample one random or grid coordinate.
    ///
    /// Each panorama is passed to `on_candidate` as soon as it is resolved,
    /// instead of waiting for the whole linked-pano traversal to finish. The
    /// callback returns `false` to stop the traversal (goal reached or stop
    /// requested). The returned vector is kept for callers that need the full
    /// batch; generation consumes the streamed callbacks.
    pub async fn candidates<F, Fut>(
        &self,
        lat: f64,
        lng: f64,
        found: &[LatLng],
        claimed: &Mutex<HashSet<String>>,
        on_candidate: F,
    ) -> Vec<Pano>
    where
        F: FnMut(Pano) -> Fut,
        Fut: Future<Output = bool>,
    {
        let mut res = match self
            .providers
            .fetch(
                self.provider(),
                PanoRequest::by_location(
                    lat,
                    lng,
                    self.settings.effective_radius(),
                    self.settings.reject_unofficial,
                ),
            )
            .await
        {
            FetchResult {
                status: FetchStatus::Ok,
                pano: Some(pano),
            } => pano,
            _ => return Vec::new(),
        };

        if !search_in_description(&res, self.settings) {
            return Vec::new();
        }

        if self.settings.reject_unofficial && !self.settings.reject_official {
            if self.settings.reject_no_description
                && !self.settings.reject_description
                && !res.has_any_description()
            {
                return Vec::new();
            }
            if self.settings.reject_description {
                if self.provider() == "apple" {
                    // The reference compares Apple's fixed description to
                    // "backpack"; the meaningful signal is the service field.
                    if res.service.as_deref() != Some("backpack") {
                        return Vec::new();
                    }
                } else if res.has_any_description() {
                    return Vec::new();
                }
            }
            if self.provider() == "yandex"
                && !res
                    .copyright
                    .as_deref()
                    .map(|value| value.contains("Yandex"))
                    .unwrap_or(false)
            {
                return Vec::new();
            }
            if self.settings.ignore_badcam
                && self.provider().contains("google")
                && res.world_height == 6656
                && month_number(res.image_date.as_deref().unwrap_or(""))
                    .map(|month| month >= month_number("2019-01").unwrap_or(i64::MAX))
                    .unwrap_or(false)
            {
                match self.non_badcam(&res.pano_id).await {
                    Some(replacement) => res = replacement,
                    None => return Vec::new(),
                }
            }
        }

        if !self.settings.reject_unofficial && self.settings.find_by_author.enabled {
            let author = res
                .copyright
                .as_deref()
                .and_then(|value| value.split('©').nth(1))
                .map(|value| value.trim().to_string())
                .unwrap_or_default();
            let author_match = author
                .to_lowercase()
                .contains(&self.settings.find_by_author.author.to_lowercase());
            let official_author = author.contains("Google");
            if !(!self.settings.reject_official && official_author) {
                match self.settings.find_by_author.filter_type.as_str() {
                    "exclude" if author_match => return Vec::new(),
                    "include" if !author_match => return Vec::new(),
                    _ => {}
                }
            }
        }

        if self.settings.find_regions {
            let limit = self.settings.region_radius * 1000.0;
            let point = LatLng::new(lat, lng);
            if found.iter().any(|other| distance_between(*other, point) < limit) {
                return Vec::new();
            }
        }

        if self.settings.reject_official {
            if is_official(&res.pano_id, self.provider()) {
                return Vec::new();
            }
            if self.settings.find_photospheres && !res.is_photosphere() {
                return Vec::new();
            }
            if self.settings.find_drones && !res.is_drone() {
                return Vec::new();
            }
        }

        if self.settings.find_night_coverage && self.provider() == "tencent" {
            let Some(night) = res.short_description.clone() else {
                return Vec::new();
            };
            let mut starts = vec![(night, 0)];
            self.expand_claimed_root(&res, lat, lng, claimed, &mut starts)
                .await;
            return self.collect(starts, claimed, on_candidate).await;
        }

        if self.settings.filter_by_minutes.enabled && !self.provider().contains("google") {
            if let Some(minutes) = self.pano_minutes(&res) {
                let [min, max] = self.settings.filter_by_minutes.range;
                if minutes < min || minutes > max {
                    return Vec::new();
                }
            }
        }

        let mut starts: Vec<(String, u32)> = Vec::new();

        if self.settings.random_in_timeline {
            if let Some(entries) = res.time.choose(&mut rand::thread_rng()) {
                if let Some(date) = time_entry_ms(entries) {
                    if date < self.from_ms || date > self.to_ms {
                        return Vec::new();
                    }
                }
                starts.push((entries.pano.clone(), 0));
            }
        }

        if self.check_all_dates
            && !self.settings.select_months
            && !self.settings.reject_official
            && !self.settings.random_in_timeline
        {
            if res.time.is_empty() {
                return Vec::new();
            }
            let mut date_within = false;
            for entry in &res.time {
                if self.settings.reject_unofficial
                    && !is_official(&entry.pano, self.provider())
                {
                    continue;
                }
                if let Some(date) = time_entry_ms(entry) {
                    if date >= self.from_ms && date <= self.to_ms {
                        date_within = true;
                        starts.push((entry.pano.clone(), 0));
                    }
                }
            }
            if !date_within {
                return Vec::new();
            }
        } else {
            if self.settings.reject_dateless && res.image_date.is_none() {
                return Vec::new();
            }
            if self.capture_matches(&res) {
                starts.push((res.pano_id.clone(), 0));
            } else {
                // The current capture is filtered out, but a newer drive of the
                // same road may be listed in its timeline. Google keeps
                // returning the old panorama from spatial search, so this is
                // the only way to reach a fresh drop.
                starts.extend(self.newer_captures(&res).into_iter().map(|id| (id, 0)));
                if starts.is_empty() {
                    return Vec::new();
                }
            }
        }

        // Spatial search often returns an existing pano for samples near a
        // filled cluster. Start from its unvisited neighbours instead of
        // dropping the sample, otherwise the stretch between two walks can
        // never grow outwards.
        self.expand_claimed_root(&res, lat, lng, claimed, &mut starts)
            .await;

        self.collect(starts, claimed, on_candidate).await
    }

    /// Panorama links and timeline entries that can seed a walk without
    /// revisiting the already claimed root itself. Timeline walking also runs
    /// in all-dates mode, so it must seed the walk there even without links.
    fn neighbour_starts(&self, pano: &Pano) -> Vec<(String, u32)> {
        let walk_links = self.settings.check_links;
        let walk_time = walk_links || (self.check_all_dates && !self.settings.select_months);
        if !walk_links && !walk_time {
            return Vec::new();
        }
        let mut starts = Vec::new();
        if walk_links {
            starts.extend(pano.links.iter().map(|link| (link.pano.clone(), 1)));
        }
        if walk_time {
            starts.extend(pano.time.iter().map(|entry| (entry.pano.clone(), 1)));
        }
        starts
    }

    /// When spatial search lands on an already found pano, seed the walk from
    /// its unvisited neighbours. If links are asymmetric, the red photometa
    /// dots around the sample are a last resort for finding the other side of
    /// the gap.
    async fn expand_claimed_root(
        &self,
        res: &Pano,
        lat: f64,
        lng: f64,
        claimed: &Mutex<HashSet<String>>,
        starts: &mut Vec<(String, u32)>,
    ) {
        if !pano_is_claimed(claimed, &res.pano_id) {
            return;
        }
        let neighbours = self.neighbour_starts(res);
        let has_unclaimed = neighbours
            .iter()
            .any(|(id, _)| !pano_is_claimed(claimed, id))
            || starts.iter().any(|(id, _)| !pano_is_claimed(claimed, id));
        starts.extend(neighbours);
        if has_unclaimed
            || !self.settings.check_links
            || !GOOGLE_PROVIDERS.contains(&self.provider())
        {
            return;
        }
        let ids = crate::providers::google::nearby_pano_ids(self.providers, lat, lng, 8).await;
        starts.extend(ids.into_iter().map(|id| (id, 1)));
    }

    async fn non_badcam(&self, pano_id: &str) -> Option<Pano> {
        let initial = match self.providers.fetch("google", PanoRequest::by_pano(pano_id)).await {
            FetchResult {
                status: FetchStatus::Ok,
                pano: Some(pano),
            } => pano,
            _ => return None,
        };
        if camera_generation(&initial, "google") != "badcam" {
            return Some(initial);
        }
        let mut candidates: Vec<&TimeEntry> = initial
            .time
            .iter()
            .filter(|entry| entry.pano != initial.pano_id)
            .collect();
        candidates.reverse();
        for entry in candidates {
            let candidate = match self
                .providers
                .fetch("google", PanoRequest::by_pano(&entry.pano))
                .await
            {
                FetchResult {
                    status: FetchStatus::Ok,
                    pano: Some(pano),
                } => pano,
                _ => continue,
            };
            if camera_generation(&candidate, "google") != "badcam" {
                return Some(candidate);
            }
        }
        None
    }

    fn pano_minutes(&self, pano: &Pano) -> Option<u32> {
        let id = &pano.pano_id;
        match self.provider() {
            "baidu" => slice_number(id, 16, 18)
                .map(|hours| hours * 60 + slice_number(id, 18, 20).unwrap_or(0)),
            "tencent" => {
                let mut minutes = slice_number(id, 14, 16)
                    .map(|hours| hours * 60 + slice_number(id, 16, 18).unwrap_or(0));
                if pano.short_description.as_deref() == Some(id.as_str()) {
                    minutes = minutes.map(|value| value + 1200);
                }
                minutes
            }
            _ => {
                let date = pano.image_date.as_deref()?;
                let time = date.split(['T', ' ']).nth(1)?;
                let hours: u32 = time.get(0..2)?.parse().ok()?;
                let minutes: u32 = time.get(3..5)?.parse().ok()?;
                Some(hours * 60 + minutes)
            }
        }
    }

    /// Reference `isPanoGood`.
    pub async fn is_pano_good(&self, pano: &Pano, found: &[LatLng]) -> bool {
        if self.settings.reject_unofficial && !self.settings.reject_official {
            if !is_official(&pano.pano_id, self.provider()) {
                return false;
            }
            if self.settings.reject_road_name && pano.road.is_some() {
                return false;
            }
            if self.settings.reject_description && pano.has_any_description() {
                return false;
            }
            if self.settings.filter_by_links_length.enabled {
                let [min, max] = self.settings.filter_by_links_length.range;
                let count = pano.links.len() as u32;
                if count < min || count > max {
                    return false;
                }
            }

            if self.generation_allows(pano) == Some(false) {
                return false;
            }

            if self.settings.filter_by_altitude.enabled
                && ALTITUDE_PROVIDERS.contains(&self.provider())
            {
                if let Some(altitude) = pano.altitude {
                    let [min, max] = self.settings.filter_by_altitude.range;
                    if altitude < min || altitude > max {
                        return false;
                    }
                }
            }

            if self.settings.get_curve {
                let headings: Vec<f64> = pano.links.iter().map(|link| link.heading).collect();
                if !accepts_curve(&headings, self.settings.min_curve_angle) {
                    return false;
                }
            }

            if self.settings.find_by_tile_color.enabled {
                let matches = tile_color_matches(
                    self.providers,
                    LatLng::new(pano.lat, pano.lng),
                    &self.settings.find_by_tile_color,
                )
                .await;
                if !matches {
                    return false;
                }
            }
        }

        let _ = found;

        if self.settings.reject_dateless && pano.image_date.is_none() {
            return false;
        }

        if !self.settings.select_months {
            if !self.check_all_dates || self.settings.reject_official {
                if let Some(date) = pano_ms(pano) {
                    if date < self.from_ms || date > self.to_ms {
                        return false;
                    }
                }
            }
        }

        if self.settings.only_one_in_timeframe {
            if pano.time.is_empty() {
                return false;
            }
            for entry in &pano.time {
                if self.settings.reject_unofficial
                    && !is_official(&entry.pano, self.provider())
                {
                    continue;
                }
                if entry.pano == pano.pano_id {
                    continue;
                }
                if let Some(date) = time_entry_ms(entry) {
                    if date >= self.from_ms && date <= self.to_ms {
                        return false;
                    }
                }
            }
        }

        if self.check_all_dates && !self.settings.select_months && !self.settings.reject_official {
            if pano.time.is_empty() {
                return false;
            }
            if self.settings.find_by_generation_enabled()
                && GENERATION_PROVIDERS.contains(&self.provider())
            {
                let generation = camera_generation(pano, self.provider());
                if generation == "0" {
                    return false;
                }
                if !self.settings.generation_allowed(&generation) {
                    return false;
                }
            }
            let mut date_within = false;
            for entry in &pano.time {
                if self.settings.reject_unofficial
                    && !is_official(&entry.pano, self.provider())
                {
                    continue;
                }
                if let Some(date) = time_entry_ms(entry) {
                    if date >= self.from_ms && date <= self.to_ms {
                        date_within = true;
                        break;
                    }
                }
            }
            if !date_within {
                return false;
            }
        }

        if self.settings.select_months && !self.settings.reject_official {
            if pano.time.is_empty() {
                return false;
            }
            if self.check_all_dates {
                let mut date_within = false;
                for entry in &pano.time {
                    if self.settings.reject_unofficial
                        && !is_official(&entry.pano, self.provider())
                    {
                        continue;
                    }
                    if let Some((year, month)) = parse_month(&entry.date) {
                        if self.month_in_range(year, month) {
                            date_within = true;
                            break;
                        }
                    }
                }
                if !date_within {
                    return false;
                }
            } else {
                let Some(date) = pano.image_date.as_deref() else {
                    return false;
                };
                let Some((year, month)) = parse_month_year(date) else {
                    return false;
                };
                if !self.month_in_range(year, month) {
                    return false;
                }
            }
        }

        true
    }

    fn month_in_range(&self, year: i32, month: u32) -> bool {
        if year < self.from_year || year > self.to_year {
            return false;
        }
        if self.from_month <= self.to_month {
            month >= self.from_month && month <= self.to_month
        } else {
            month >= self.from_month || month <= self.to_month
        }
    }

    /// Breadth-first equivalent of `getPanoDeep`, streaming accepted panoramas
    /// through `on_candidate` as soon as each request resolves. `claimed` is
    /// shared across the whole run: an id that is already claimed (including
    /// every location already found) ends that branch instead of being walked
    /// through again. Up to 16 requests stay in flight.
    async fn collect<F, Fut>(
        &self,
        starts: Vec<(String, u32)>,
        claimed: &Mutex<HashSet<String>>,
        mut on_candidate: F,
    ) -> Vec<Pano>
    where
        F: FnMut(Pano) -> Fut,
        Fut: Future<Output = bool>,
    {
        let max_depth = LINKS_DEPTH;
        let mut queue: VecDeque<(String, u32)> = VecDeque::new();
        for (id, depth) in starts {
            if depth <= max_depth && claim_pano(claimed, &id) {
                queue.push_back((id, depth));
            }
        }

        let mut out: Vec<Pano> = Vec::new();
        let mut out_ids: HashSet<String> = HashSet::new();
        let mut in_flight = FuturesUnordered::new();

        while !queue.is_empty() || !in_flight.is_empty() {
            while in_flight.len() < 16 {
                let Some((id, depth)) = queue.pop_front() else {
                    break;
                };
                in_flight.push(async move {
                    let pano = self.fetch_by_pano(&id).await;
                    (id, depth, pano)
                });
            }

            let Some((id, depth, pano)) = in_flight.next().await else {
                break;
            };
            let Some(pano) = pano else {
                // A transient failure must not keep later samples out forever.
                unclaim_pano(claimed, &id);
                continue;
            };
            let in_country = self.rings.contains(pano.lat, pano.lng);
            let good = in_country && self.is_pano_good(&pano, &[]).await;
            let next_depth = if good { 1 } else { depth + 1 };

            if self.check_all_dates && !self.settings.select_months {
                for entry in &pano.time {
                    if self.settings.reject_unofficial
                        && !is_official(&entry.pano, self.provider())
                    {
                        continue;
                    }
                    if let Some(date) = time_entry_ms(entry) {
                        if date >= self.from_ms && date <= self.to_ms {
                            enqueue(
                                &mut queue,
                                claimed,
                                &entry.pano,
                                next_depth,
                                max_depth,
                            );
                        }
                    }
                }
            }

            if self.settings.check_links {
                for link in &pano.links {
                    enqueue(
                        &mut queue,
                        claimed,
                        &link.pano,
                        next_depth,
                        max_depth,
                    );
                }
                for entry in &pano.time {
                    enqueue(
                        &mut queue,
                        claimed,
                        &entry.pano,
                        next_depth,
                        max_depth,
                    );
                }
            }

            if good && out_ids.insert(pano.pano_id.clone()) {
                out.push(pano.clone());
                if !on_candidate(pano).await {
                    break;
                }
            }
        }

        out
    }

    /// Reference `addLoc`: turns a panorama into the marker the UI receives.
    pub async fn build_marker(&self, pano: &Pano) -> Marker {
        let settings = self.settings;
        let mut heading = 0.0;
        if settings.heading.adjust {
            if settings.heading.reference == "forward" {
                heading = pano.center_heading;
                if self.provider() == "mapillary" {
                    heading = 0.5;
                }
            } else if settings.heading.reference == "backward" {
                heading = (pano.center_heading + 180.0) % 360.0;
                if self.provider() == "mapillary" {
                    heading = 0.0;
                }
            } else if settings.heading.reference == "link" {
                heading = pano
                    .links
                    .first()
                    .map(|link| link.heading)
                    .unwrap_or(pano.center_heading);
            }
            if settings.heading.random_in_range {
                heading += random_in_range(
                    settings.heading.range[0],
                    settings.heading.range[1],
                );
            } else {
                heading += if rand::random::<bool>() {
                    settings.heading.range[0]
                } else {
                    settings.heading.range[1]
                };
            }
            if self.provider() == "mapillary" {
                heading = heading_to_mapillary_x(heading, pano.center_heading);
            }
        }

        let mut pitch = 0.0;
        if settings.pitch.adjust {
            pitch = if settings.pitch.random_in_range {
                random_in_range(settings.pitch.range[0], settings.pitch.range[1])
            } else if rand::random::<bool>() {
                settings.pitch.range[0]
            } else {
                settings.pitch.range[1]
            };
        }
        if self.provider() == "mapillary" {
            pitch = pitch_to_mapillary_y(pitch);
        }

        let mut zoom = 0.0;
        if settings.zoom.adjust {
            zoom = if settings.zoom.random_in_range {
                random_in_range(settings.zoom.range[0], settings.zoom.range[1])
            } else if rand::random::<bool>() {
                settings.zoom.range[0]
            } else {
                settings.zoom.range[1]
            };
        }

        let mut links: Vec<String> = pano
            .links
            .iter()
            .map(|link| link.pano.clone())
            .chain(pano.time.iter().map(|entry| entry.pano.clone()))
            .collect();
        links.sort();
        links.dedup();
        links.retain(|id| id != &pano.pano_id);

        let mut marker = Marker {
            pano_id: pano.pano_id.clone(),
            lat: pano.lat,
            lng: pano.lng,
            heading,
            pitch,
            zoom,
            country: pano.country.clone(),
            region: pano.region.clone(),
            locality: pano.locality.clone(),
            road: pano.road.clone(),
            image_date: pano.image_date.clone(),
            source: format!("{}_pano", if self.provider() == "tencent" { "qq" } else { self.provider() }),
            update_type: None,
            generation: Some(report_generation(pano, self.provider())),
            previous_generation: None,
            has_history: pano.time.iter().any(|entry| entry.pano != pano.pano_id),
            links,
            extra: MarkerExtra {
                tags: vec![self.provider().to_string()],
            },
            marker_type: None,
        };

        let mut time: Vec<&TimeEntry> = pano.time.iter().collect();
        if settings.reject_unofficial {
            time.retain(|entry| is_official(&entry.pano, self.provider()));
        }
        let previous = if time.len() >= 2 {
            Some(time[time.len() - 2].pano.clone())
        } else {
            None
        };

        let mut previous_generation = None;
        let (update_type, marker_type) = match previous {
            None => {
                if !self.provider().contains("google") {
                    if ["yandex", "baidu", "tencent", "naver", "kakao"].contains(&self.provider()) {
                        (Some("newroad"), Some("newRoad"))
                    } else {
                        (Some("gen4update"), Some("gen4"))
                    }
                } else if settings.disable_check_blue_line {
                    (Some("newroad"), Some("newRoad"))
                } else if has_blue_line_at(self.providers, pano.lat, pano.lng).await {
                    (Some("newroad"), Some("newRoad"))
                } else {
                    (Some("noblueline"), Some("noBlueLine"))
                }
            }
            Some(previous_id) => {
                if !self.provider().contains("google") {
                    (Some("gen4update"), Some("gen4"))
                } else {
                    let previous_pano = self.fetch_by_pano(&previous_id).await;
                    previous_generation = previous_pano
                        .as_ref()
                        .map(|previous| report_generation(previous, self.provider()));
                    match previous_pano.map(|previous| previous.world_height).unwrap_or(0) {
                        1664 => (Some("gen1update"), Some("gen1")),
                        6656 => (Some("gen2or3update"), Some("gen2or3")),
                        _ => (Some("gen4update"), Some("gen4")),
                    }
                }
            }
        };
        marker.previous_generation = previous_generation;

        if let Some(update_type) = update_type {
            marker.update_type = Some(update_type.to_string());
            marker.extra.tags.push(update_type.to_string());
        }
        marker.marker_type = marker_type.map(|value| value.to_string());
        marker
    }
}

fn pano_is_claimed(claimed: &Mutex<HashSet<String>>, id: &str) -> bool {
    claimed
        .lock()
        .map(|claimed| claimed.contains(id))
        .unwrap_or(false)
}

fn claim_pano(claimed: &Mutex<HashSet<String>>, id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    claimed
        .lock()
        .map(|mut claimed| claimed.insert(id.to_string()))
        .unwrap_or(false)
}

/// Frees an id whose metadata could not be fetched, so a later sample can
/// still walk into that branch.
fn unclaim_pano(claimed: &Mutex<HashSet<String>>, id: &str) {
    if id.is_empty() {
        return;
    }
    if let Ok(mut claimed) = claimed.lock() {
        claimed.remove(id);
    }
}

fn enqueue(
    queue: &mut VecDeque<(String, u32)>,
    claimed: &Mutex<HashSet<String>>,
    id: &str,
    depth: u32,
    max_depth: u32,
) {
    if depth <= max_depth && claim_pano(claimed, id) {
        queue.push_back((id.to_string(), depth));
    }
}

fn slice_number(value: &str, start: usize, end: usize) -> Option<u32> {
    value.get(start..end)?.parse().ok()
}

fn heading_to_mapillary_x(heading: f64, image_heading: f64) -> f64 {
    let mut relative = heading - image_heading;
    relative = ((relative % 360.0) + 360.0) % 360.0;
    let mut x = 0.5 - relative / 360.0;
    if x < 0.0 {
        x += 1.0;
    }
    x.clamp(0.0, 1.0)
}

fn pitch_to_mapillary_y(pitch: f64) -> f64 {
    (0.5 - pitch / 180.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Link;

    #[test]
    fn official_ids() {
        assert!(is_official("abcdefghijklmnopqrstuv", "google"));
        assert!(!is_official("short", "google"));
        assert!(is_official("anything", "bing"));
        assert!(!is_official("anything", "unknown"));
    }

    #[test]
    fn generations_match_reference() {
        let mut pano = Pano {
            pano_id: "abcdefghijklmnopqrstuv".into(),
            provider: "google".into(),
            world_height: 6656,
            country: Some("FR".into()),
            image_date: Some("2010-10".into()),
            ..Default::default()
        };
        assert_eq!(camera_generation(&pano, "google"), "23");
        pano.image_date = Some("2012-05".into());
        assert_eq!(camera_generation(&pano, "google"), "3");
        pano.world_height = 1664;
        assert_eq!(camera_generation(&pano, "google"), "1");
        pano.world_height = 8192;
        assert_eq!(camera_generation(&pano, "google"), "4");

        let yandex = Pano {
            world_width: 5632,
            provider: "yandex".into(),
            ..Default::default()
        };
        assert_eq!(camera_generation(&yandex, "yandex"), "1");
    }

    #[test]
    fn report_generation_matches_userscript() {
        let mut pano = Pano {
            world_height: 6656,
            country: Some("US".into()),
            image_date: Some("2010-10".into()),
            lat: 40.0,
            ..Default::default()
        };
        assert_eq!(report_generation(&pano, "google"), "Gen2/3");
        pano.image_date = Some("2022-01".into());
        assert_eq!(report_generation(&pano, "google"), "BadCam");
        pano.world_height = 8192;
        assert_eq!(report_generation(&pano, "google"), "Gen4");
        pano.world_height = 6656;
        pano.country = Some("NO".into());
        pano.image_date = Some("2010-01".into());
        assert_eq!(report_generation(&pano, "google"), "Gen2");
    }

    #[test]
    fn descriptions_normalize() {
        assert_eq!(normalize_text("Café  Rue!"), "cafe  rue");
        assert!(contains_word("rue de la paix", "rue"));
        assert!(!contains_word("ruelle", "rue"));
        assert!(section_match("Rue de Test, Paris", "rue de test"));
    }

    #[test]
    fn marker_heading_helpers() {
        assert!((heading_to_mapillary_x(0.0, 0.0) - 0.5).abs() < 1e-9);
        assert!((heading_to_mapillary_x(90.0, 0.0) - 0.25).abs() < 1e-9);
        assert!((pitch_to_mapillary_y(0.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn curve_links_are_checked() {
        let links = vec![
            Link {
                pano: "a".into(),
                heading: 10.0,
                description: String::new(),
            },
            Link {
                pano: "b".into(),
                heading: 200.0,
                description: String::new(),
            },
        ];
        let headings: Vec<f64> = links.iter().map(|link| link.heading).collect();
        assert!(accepts_curve(&headings, 10.0));
    }

    fn gen4_settings() -> Settings {
        let mut settings = Settings::default();
        settings.provider = "google".into();
        settings.from_date = "2007-01".into();
        settings.to_date = "2026-11".into();
        settings.reject_dateless = false;
        settings.reject_no_description = false;
        settings.find_by_generation = serde_json::json!({
            "enabled": true,
            "google": { "4": true }
        });
        settings
    }

    fn ghana_rings() -> PolyRings {
        PolyRings::from_geometry(&serde_json::json!({
            "type": "Polygon",
            "coordinates": [[
                [-2.70, 4.98],
                [-2.63, 4.98],
                [-2.63, 5.03],
                [-2.70, 5.03],
                [-2.70, 4.98]
            ]]
        }))
        .expect("valid polygon")
    }

    /// A re-drive is published as a new panorama chain while the spatial search
    /// keeps returning the original capture. The fresh panorama only exists as
    /// a timeline entry of the old one, which is how Google actually serves it.
    #[tokio::test]
    async fn gen4_redrive_is_reached_through_the_old_timeline() {
        let settings = gen4_settings();
        let providers = Providers::new();
        let rings = ghana_rings();
        let filters = Filters::new(&settings, &providers, &rings);

        let old_id = "YvpfQLuVG--g4I7t6vmJEQ";
        let fresh_id = "1p3AovNcyTFWrhOFoV301Q";
        let old = Pano {
            pano_id: old_id.into(),
            provider: "google".into(),
            image_date: Some("2016-03".into()),
            world_height: 6656,
            time: vec![
                TimeEntry {
                    pano: old_id.into(),
                    date: "2016-03".into(),
                },
                TimeEntry {
                    pano: fresh_id.into(),
                    date: "2026-09".into(),
                },
            ],
            ..Default::default()
        };
        let fresh = Pano {
            pano_id: fresh_id.into(),
            provider: "google".into(),
            image_date: Some("2026-09".into()),
            world_height: 8192,
            ..Default::default()
        };

        assert!(!filters.capture_matches(&old));
        assert_eq!(filters.newer_captures(&old), vec![fresh_id.to_string()]);
        assert!(filters.capture_matches(&fresh));
        assert!(filters.is_pano_good(&fresh, &[]).await);
        assert!(!filters.is_pano_good(&old, &[]).await);
    }

    #[test]
    fn only_newer_captures_inside_the_window_are_followed() {
        let settings = gen4_settings();
        let providers = Providers::new();
        let rings = ghana_rings();
        let filters = Filters::new(&settings, &providers, &rings);

        let current = "YvpfQLuVG--g4I7t6vmJEQ";
        let mut pano = Pano {
            pano_id: current.into(),
            provider: "google".into(),
            image_date: Some("2026-09".into()),
            world_height: 6656,
            time: vec![
                TimeEntry {
                    pano: current.into(),
                    date: "2026-09".into(),
                },
                TimeEntry {
                    pano: "aaaaaaaaaaaaaaaaaaaaaa".into(),
                    date: "2016-03".into(),
                },
                TimeEntry {
                    pano: "bbbbbbbbbbbbbbbbbbbbbb".into(),
                    date: "2027-04".into(),
                },
            ],
            ..Default::default()
        };
        assert!(filters.newer_captures(&pano).is_empty());

        // An unofficial id is skipped while "reject unofficial" is on.
        pano.time = vec![
            TimeEntry {
                pano: current.into(),
                date: "2026-09".into(),
            },
            TimeEntry {
                pano: "si".into(),
                date: "2026-10".into(),
            },
        ];
        assert!(filters.newer_captures(&pano).is_empty());
    }

    #[test]
    fn generation_gate_is_inactive_when_the_filter_is_off() {
        let mut settings = gen4_settings();
        settings.find_by_generation = serde_json::json!({ "enabled": false });
        let providers = Providers::new();
        let rings = ghana_rings();
        let filters = Filters::new(&settings, &providers, &rings);

        let old = Pano {
            pano_id: "YvpfQLuVG--g4I7t6vmJEQ".into(),
            provider: "google".into(),
            image_date: Some("2016-03".into()),
            world_height: 6656,
            ..Default::default()
        };
        assert_eq!(filters.generation_allows(&old), None);
        assert!(filters.capture_matches(&old));
    }

    #[test]
    fn claimed_locations_end_walks() {
        let claimed = Mutex::new(HashSet::from(["existing".to_string()]));
        let mut queue = VecDeque::new();

        enqueue(&mut queue, &claimed, "existing", 1, 2);
        assert!(queue.is_empty(), "an existing location must not be walked into");

        enqueue(&mut queue, &claimed, "new", 1, 2);
        assert_eq!(queue.len(), 1);
        enqueue(&mut queue, &claimed, "new", 1, 2);
        assert_eq!(queue.len(), 1, "an id must only be claimed once");
    }

    /// A sample in a gap resolves to a pano that was already found. Walking
    /// must start at its unclaimed neighbours, otherwise the whole stretch
    /// between two found clusters stays empty forever.
    #[tokio::test]
    async fn claimed_sample_walks_into_the_gap_between_two_clusters() {
        let mut settings = Settings::default();
        settings.check_links = true;
        settings.from_date = "2007-01".into();
        settings.to_date = "2026-11".into();
        let providers = Providers::new();
        let rings = ghana_rings();
        let filters = Filters::new(&settings, &providers, &rings);

        let existing = "aaaaaaaaaaaaaaaaaaaaaa";
        let gap_start = "bbbbbbbbbbbbbbbbbbbbbb";
        let gap_end = "cccccccccccccccccccccc";
        let other_existing = "dddddddddddddddddddddd";

        let pano = |id: &str, links: &[&str]| Pano {
            pano_id: id.into(),
            provider: "google".into(),
            image_date: Some("2020-01".into()),
            world_height: 8192,
            lat: 5.0,
            lng: -2.65,
            links: links
                .iter()
                .map(|next| Link {
                    pano: (*next).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        for pano in [
            pano(existing, &[gap_start]),
            pano(gap_start, &[existing, gap_end]),
            pano(gap_end, &[gap_start, other_existing]),
            pano(other_existing, &[gap_end]),
        ] {
            providers.cache_pano(pano);
        }

        let claimed = Mutex::new(HashSet::from([
            existing.to_string(),
            other_existing.to_string(),
        ]));
        let root = providers
            .lookup_cached("google", existing)
            .expect("cached root");
        let starts = filters.neighbour_starts(&root);
        assert_eq!(starts, vec![(gap_start.to_string(), 1)]);

        let gap_pano = providers
            .lookup_cached("google", gap_start)
            .expect("cached gap pano");
        assert!(filters.rings.contains(gap_pano.lat, gap_pano.lng));
        assert!(filters.is_pano_good(&gap_pano, &[]).await);
        let found = filters.collect(starts, &claimed, |_| async { true }).await;
        let ids: Vec<&str> = found.iter().map(|pano| pano.pano_id.as_str()).collect();
        assert_eq!(ids, vec![gap_start, gap_end]);
    }

    #[test]
    fn all_dates_seeds_timeline_without_link_walking() {
        let mut settings = Settings::default();
        settings.check_links = false;
        settings.check_all_dates = true;
        settings.from_date = "2007-01".into();
        settings.to_date = "2026-11".into();
        let providers = Providers::new();
        let rings = ghana_rings();
        let filters = Filters::new(&settings, &providers, &rings);

        let pano = Pano {
            links: vec![Link {
                pano: "aaaaaaaaaaaaaaaaaaaaaa".into(),
                ..Default::default()
            }],
            time: vec![TimeEntry {
                pano: "bbbbbbbbbbbbbbbbbbbbbb".into(),
                date: "2020-01".into(),
            }],
            ..Default::default()
        };
        assert_eq!(
            filters.neighbour_starts(&pano),
            vec![("bbbbbbbbbbbbbbbbbbbbbb".to_string(), 1)]
        );
    }

    #[test]
    fn failed_metadata_frees_the_claim() {
        let claimed = Mutex::new(HashSet::from(["stale".to_string()]));
        unclaim_pano(&claimed, "stale");
        assert!(claimed.lock().unwrap().is_empty());
        unclaim_pano(&claimed, "");
        assert!(claimed.lock().unwrap().is_empty());
    }

    /// The fixed look-ahead must cross a few rejected captures; otherwise a
    /// short old-generation stretch still splits the road into two clusters.
    #[tokio::test]
    async fn walk_crosses_several_rejected_captures() {
        let mut settings = Settings::default();
        settings.check_links = true;
        settings.from_date = "2007-01".into();
        settings.to_date = "2026-11".into();
        let providers = Providers::new();
        let rings = ghana_rings();
        let filters = Filters::new(&settings, &providers, &rings);

        let rejected = ["aaaaaaaaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbbbbbbbb", "cccccccccccccccccccccc"];
        let accepted = "dddddddddddddddddddddd";
        let pano = |id: &str, links: &[&str], world_height: u32| Pano {
            pano_id: id.into(),
            provider: "google".into(),
            image_date: Some("2020-01".into()),
            world_height,
            lat: 5.0,
            lng: -2.65,
            links: links
                .iter()
                .map(|next| Link {
                    pano: (*next).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        providers.cache_pano(pano(rejected[0], &[rejected[1]], 0));
        providers.cache_pano(pano(rejected[1], &[rejected[2]], 0));
        providers.cache_pano(pano(rejected[2], &[accepted], 0));
        providers.cache_pano(pano(accepted, &[], 8192));

        let claimed = Mutex::new(HashSet::new());
        let found = filters
            .collect(vec![(rejected[0].to_string(), 1)], &claimed, |_| async { true })
            .await;
        let ids: Vec<&str> = found.iter().map(|pano| pano.pano_id.as_str()).collect();
        assert_eq!(ids, vec![accepted]);
    }
}
