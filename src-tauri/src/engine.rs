//! Generation engine.
//!
//! Runs random/grid strategies per polygon, applies the provider-neutral
//! filters, dedupes panoramas globally by pano id and emits progress events
//! to the frontend. Notifications and Discord webhooks are fired from here.
//!
//! Each generator keeps a fixed number of samples in flight instead of waiting
//! for a whole batch, so a slow or empty response never stalls the pipeline.
//! Settings, rings and marker snapshots are shared through `Arc`s rather than
//! cloned per sample.

use crate::filters::Filters;
use crate::geo::{GridGenerator, LatLng, PolyRings};
use crate::land::is_land_near;
use crate::models::{
    Goals, LocationEvent, Marker, PolygonDoneEvent, PolygonInput, ProgressEvent, Settings,
};
use crate::providers::Providers;
use crate::tiles::BlueLineMask;
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Hard ceiling on simultaneous provider requests. The keyless Google endpoint
/// plateaus around 128 in flight (roughly 900 requests/s); beyond that latency
/// and memory grow without adding throughput.
const MAX_CONCURRENCY: usize = 128;

/// How many samples one generator keeps in flight. This is what the "Speed"
/// setting controls, so the number of requests follows it directly.
const MAX_PER_GENERATOR: usize = 128;

/// Give up on the land mask after this many consecutive sea samples. Islands
/// too small for the 0.05° mask would otherwise never produce points.
const LAND_MASK_GIVE_UP: u32 = 4_000;

struct RunContext {
    app: AppHandle,
    settings: Arc<Settings>,
    providers: Arc<Providers>,
    /// Panorama ids already taken (globally unique across polygons).
    seen: Mutex<HashSet<String>>,
    /// Every id visited or claimed by a walk. Seeded with `seen` so a walk
    /// stops when it reaches an existing location instead of walking past it.
    claimed: Mutex<HashSet<String>>,
    /// Markers found per polygon id, this run included.
    counts: Mutex<HashMap<String, usize>>,
    /// Live goals per polygon id, shared with the `update_goals` command.
    goals: Goals,
    start: Instant,
    semaphore: Semaphore,
    stop: Arc<AtomicBool>,
}

struct PolyRuntime {
    id: String,
    name: String,
    rings: Arc<PolyRings>,
    goals: Goals,
    /// Accepted coordinates, only maintained when region spacing is on.
    found_points: Mutex<Vec<LatLng>>,
}

impl PolyRuntime {
    fn needed(&self) -> usize {
        self.goals
            .lock()
            .ok()
            .and_then(|goals| goals.get(&self.id).copied())
            .unwrap_or(1)
    }
}

enum PointCheck {
    Accept,
    Reject,
    /// Inside the polygon but on water.
    Sea,
}

pub(crate) fn polygon_needed(polygon: &PolygonInput) -> usize {
    if polygon.nb_needed == 0 {
        1
    } else {
        polygon.nb_needed as usize
    }
}

pub async fn run(
    app: AppHandle,
    settings: Settings,
    polygons: Vec<PolygonInput>,
    providers: Arc<Providers>,
    stop: Arc<AtomicBool>,
    goals: Goals,
) {
    let settings = Arc::new(settings);
    let start = Instant::now();

    let mut counts = HashMap::new();
    let mut seen = HashSet::new();
    let mut runtimes: Vec<Arc<PolyRuntime>> = Vec::new();

    for polygon in &polygons {
        let Some(rings) = PolyRings::from_geometry(&polygon.geometry) else {
            continue;
        };
        counts.insert(polygon.id.clone(), polygon.found.len());
        for marker in &polygon.found {
            seen.insert(marker.pano_id.clone());
        }
        let found_points = if settings.find_regions {
            polygon
                .found
                .iter()
                .map(|marker| LatLng::new(marker.lat, marker.lng))
                .collect()
        } else {
            Vec::new()
        };
        runtimes.push(Arc::new(PolyRuntime {
            id: polygon.id.clone(),
            name: polygon.name.clone(),
            rings: Arc::new(rings),
            goals: goals.clone(),
            found_points: Mutex::new(found_points),
        }));
    }

    let per_polygon = if settings.one_country_at_a_time || settings.strategy == "grid" {
        1
    } else {
        settings.num_of_generators.clamp(1, 10)
    };
    let limit = if settings.find_regions {
        // Region spacing is inherently sequential: one probe at a time.
        per_polygon
    } else {
        settings
            .speed
            .max(1)
            .saturating_mul(per_polygon)
            .min(MAX_CONCURRENCY)
    }
    .max(1);

    let claimed = Mutex::new(seen.clone());
    let ctx = Arc::new(RunContext {
        app,
        settings,
        providers,
        seen: Mutex::new(seen),
        claimed,
        counts: Mutex::new(counts),
        goals,
        start,
        semaphore: Semaphore::new(limit),
        stop,
    });

    if ctx.settings.one_country_at_a_time {
        for poly in runtimes {
            if ctx.stop.load(Ordering::Relaxed) {
                break;
            }
            run_one(ctx.clone(), poly).await;
        }
    } else {
        let mut running = FuturesUnordered::new();
        for poly in runtimes {
            for _ in 0..per_polygon {
                running.push(run_one(ctx.clone(), poly.clone()));
            }
        }
        while running.next().await.is_some() {}
    }

    let _ = ctx.app.emit("generation:finished", serde_json::json!({}));
}

/// Runs one generator until its polygon reaches the goal, the strategy is
/// exhausted or the run is stopped.
async fn run_one(ctx: Arc<RunContext>, poly: Arc<PolyRuntime>) {
    let settings = &ctx.settings;
    let radius = settings.effective_radius();
    let target = if settings.find_regions {
        1
    } else {
        settings.speed.clamp(1, MAX_PER_GENERATOR)
    };

    let blue_mask = if settings.only_check_blue_lines {
        Some(
            BlueLineMask::build(
                &ctx.providers,
                LatLng::new(poly.rings.bounds.north, poly.rings.bounds.west),
                LatLng::new(poly.rings.bounds.south, poly.rings.bounds.east),
            )
            .await,
        )
    } else {
        None
    };

    let mut grid = if settings.strategy == "grid" {
        match GridGenerator::new(&poly.rings, radius) {
            Ok(grid) => Some(grid),
            Err(_) => return,
        }
    } else {
        None
    };

    let mut set: JoinSet<()> = JoinSet::new();
    let mut exhausted = false;
    let mut sea_rejections = 0u32;
    let mut skip_land_mask = false;

    loop {
        // Read the goal every iteration so live goal edits take effect.
        let needed = poly.needed();
        if ctx.stop.load(Ordering::Relaxed) || found_count(&ctx, &poly.id) >= needed {
            break;
        }

        // Keep the pipeline full: top up to `target` samples in flight.
        while set.len() < target && !exhausted {
            if ctx.stop.load(Ordering::Relaxed) {
                break;
            }
            let mut point = None;
            if let Some(generator) = grid.as_mut() {
                while let Some(candidate) = generator.next_point() {
                    if ctx.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    match check_point(&poly, blue_mask.as_ref(), radius, candidate, settings.use_land_mask, skip_land_mask) {
                        PointCheck::Accept => {
                            point = Some(candidate);
                            break;
                        }
                        // The grid is finite, so sea cells are simply skipped.
                        PointCheck::Reject | PointCheck::Sea => continue,
                    }
                }
                if point.is_none() {
                    exhausted = true;
                }
            } else {
                let mut tries = 0u32;
                while tries < 200_000 {
                    tries += 1;
                    if ctx.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let candidate = poly.rings.random_point();
                    match check_point(&poly, blue_mask.as_ref(), radius, candidate, settings.use_land_mask, skip_land_mask) {
                        PointCheck::Accept => {
                            sea_rejections = 0;
                            point = Some(candidate);
                            break;
                        }
                        PointCheck::Sea => {
                            sea_rejections += 1;
                            if sea_rejections >= LAND_MASK_GIVE_UP {
                                skip_land_mask = true;
                            }
                        }
                        PointCheck::Reject => {}
                    }
                }
                if point.is_none() {
                    exhausted = true;
                }
            }

            let Some(point) = point else { break };
            let task_ctx = ctx.clone();
            let task_poly = poly.clone();
            set.spawn(async move {
                let _permit = task_ctx.semaphore.acquire().await.ok();
                if task_ctx.stop.load(Ordering::Relaxed) {
                    return;
                }
                let found = if task_ctx.settings.find_regions {
                    task_poly
                        .found_points
                        .lock()
                        .map(|points| points.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                process_sample(&task_ctx, &task_poly, point, &found).await
            });
        }

        let Some(joined) = set.join_next().await else {
            break;
        };
        // A cancelled or panicked sample must not kill the run.
        let Ok(()) = joined else { continue };
    }

    set.abort_all();
    while set.join_next().await.is_some() {}

    let found = found_count(&ctx, &poly.id);
    let needed = poly.needed();
    if found >= needed {
        let _ = ctx.app.emit(
            "generation:polygon-done",
            PolygonDoneEvent {
                polygon_id: poly.id.clone(),
                found,
                needed,
            },
        );
    }
}

fn found_count(ctx: &RunContext, polygon_id: &str) -> usize {
    ctx.counts
        .lock()
        .map(|counts| counts.get(polygon_id).copied().unwrap_or(0))
        .unwrap_or(0)
}

fn check_point(
    poly: &PolyRuntime,
    blue_mask: Option<&BlueLineMask>,
    radius: f64,
    point: LatLng,
    use_land_mask: bool,
    skip_land_mask: bool,
) -> PointCheck {
    if !poly.rings.contains(point.lat, point.lng) {
        return PointCheck::Reject;
    }
    if let Some(mask) = blue_mask {
        if !mask.contains(point.lat, point.lng, radius) {
            return PointCheck::Reject;
        }
    }
    if use_land_mask && !skip_land_mask && !is_land_near(point.lat, point.lng) {
        return PointCheck::Sea;
    }
    PointCheck::Accept
}

async fn process_sample(ctx: &RunContext, poly: &PolyRuntime, point: LatLng, found: &[LatLng]) {
    if ctx.stop.load(Ordering::Relaxed) {
        return;
    }
    let filters = Filters::new(&ctx.settings, &ctx.providers, &poly.rings);
    let needed = poly.needed();

    // Emit each linked panorama as soon as it passes the filters, rather than
    // returning a batch after the whole link traversal has finished.
    filters
        .candidates(point.lat, point.lng, found, &ctx.claimed, |pano| {
            let filters = &filters;
            async move {
                if ctx.stop.load(Ordering::Relaxed) {
                    return false;
                }
                {
                    let Ok(mut seen) = ctx.seen.lock() else {
                        return true;
                    };
                    if !seen.insert(pano.pano_id.clone()) {
                        return true;
                    }
                }
                let marker = filters.build_marker(&pano).await;
                try_add_marker(ctx, poly, needed, marker).await.is_some()
                    && !ctx.stop.load(Ordering::Relaxed)
            }
        })
        .await;
}

async fn try_add_marker(
    ctx: &RunContext,
    poly: &PolyRuntime,
    needed: usize,
    marker: Marker,
) -> Option<usize> {
    let (found_len, all_complete) = {
        let mut counts = ctx.counts.lock().ok()?;
        let entry = counts.get_mut(poly.id.as_str())?;
        if *entry >= needed {
            return None;
        }
        *entry += 1;
        let found_len = *entry;
        let all_complete = ctx
            .goals
            .lock()
            .map(|goals| {
                goals
                    .iter()
                    .all(|(id, need)| counts.get(id).copied().unwrap_or(0) >= *need)
            })
            .unwrap_or(false);
        (found_len, all_complete)
    };

    if ctx.settings.find_regions {
        if let Ok(mut points) = poly.found_points.lock() {
            points.push(LatLng::new(marker.lat, marker.lng));
        }
    }

    let _ = ctx.app.emit(
        "generation:location",
        LocationEvent {
            polygon_id: poly.id.clone(),
            location: marker.clone(),
        },
    );
    let _ = ctx.app.emit(
        "generation:progress",
        ProgressEvent {
            polygon_id: poly.id.clone(),
            found: found_len,
            needed,
        },
    );

    if ctx.settings.notification.enabled {
        let elapsed = ctx.start.elapsed().as_secs_f64();
        let elapsed_str = format!("{:.1}", elapsed);
        let is_no_blue = marker.update_type.as_deref() == Some("noblueline");
        let notification = &ctx.settings.notification;
        // Discord links are Google-only; other providers always fall back to desktop.
        let discord_available = ctx.settings.provider.contains("google");
        let discord = discord_available && matches!(notification.target.as_str(), "discord" | "both");
        let desktop = !discord_available || notification.target.as_str() != "discord";
        if notification.any_location && found_len == 1 {
            let link = format!(
                "https://www.google.com/maps/@?api=1&map_action=pano&pano={}&heading={}&pitch={}&fov={}",
                marker.pano_id,
                marker.heading,
                marker.pitch,
                180.0 / 2.0_f64.powf(marker.zoom)
            );
            send_notification(
                &ctx.app,
                &ctx.settings,
                &format!("First location found in {} ({}s)", poly.name, elapsed_str),
                "",
                Some(&link),
                desktop,
                discord,
            )
            .await;
        }
        if notification.any_no_blue_line && is_no_blue && found_len == 1 {
            let link = format!(
                "https://www.google.com/maps/@?api=1&map_action=pano&pano={}&heading={}&pitch={}&fov={}",
                marker.pano_id,
                marker.heading,
                marker.pitch,
                180.0 / 2.0_f64.powf(marker.zoom)
            );
            send_notification(
                &ctx.app,
                &ctx.settings,
                &format!("No blue line location found in {} ({}s)", poly.name, elapsed_str),
                "",
                Some(&link),
                desktop,
                discord,
            )
            .await;
        }
        if notification.one_polygon_complete && found_len >= needed {
            send_notification(
                &ctx.app,
                &ctx.settings,
                "Polygon Completed",
                &format!("Polygon has reached its goal ({}s)", elapsed_str),
                None,
                desktop,
                discord,
            )
            .await;
        }
        if notification.all_polygons_complete && all_complete {
            send_notification(
                &ctx.app,
                &ctx.settings,
                "Generation Completed",
                &format!("All polygons have reached their goals ({}s)", elapsed_str),
                None,
                desktop,
                discord,
            )
            .await;
        }
    }

    Some(found_len)
}

async fn send_notification(
    app: &AppHandle,
    settings: &Settings,
    title: &str,
    body: &str,
    link: Option<&str>,
    desktop: bool,
    discord: bool,
) {
    if desktop {
        let _ = app.notification().builder().title(title).body(body).show();
    }
    if discord {
        if let Some(url) = settings.notification.discord_webhook.clone() {
            let client = reqwest::Client::new();
            let content = match link {
                Some(l) if body.is_empty() => format!("**{}**\n{}", title, l),
                Some(l) => format!("**{}**\n{}\n{}", title, body, l),
                None => format!("**{}**\n{}", title, body),
            };
            let _ = client
                .post(url)
                .json(&serde_json::json!({"content": content}))
                .send()
                .await;
        }
    }
}
