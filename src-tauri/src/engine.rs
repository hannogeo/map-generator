//! Generation engine.
//!
//! Runs random/grid strategies per polygon, applies the provider-neutral
//! filters, dedupes panoramas globally by pano id and emits progress events
//! to the frontend. Notifications and Discord webhooks are fired from here.

use crate::filters::Filters;
use crate::geo::{LatLng, PolyRings};
use crate::models::{
    LocationEvent, Marker, PolygonDoneEvent, PolygonInput, ProgressEvent, Settings,
};
use crate::providers::{Providers};
use crate::tiles::BlueLineMask;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::Semaphore;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

pub async fn run(
    app: AppHandle,
    settings: Settings,
    polygons: Vec<PolygonInput>,
    providers: Arc<Providers>,
    stop: Arc<AtomicBool>,
) {
    // Global dedupe set, seeded with any markers the frontend already had.
    let global_seen: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let found_map: Arc<Mutex<HashMap<String, Vec<Marker>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    for polygon in &polygons {
        found_map
            .lock()
            .unwrap()
            .insert(polygon.id.clone(), polygon.found.clone());
        for marker in &polygon.found {
            global_seen.lock().unwrap().insert(marker.pano_id.clone());
        }
    }

    let start_time = Instant::now();
    let grid = settings.strategy == "grid";
    let concurrency = Arc::new(Semaphore::new(16));

    // Per-polygon goals, used to decide when everything is complete.
    let needs: Arc<HashMap<String, usize>> = Arc::new(
        polygons
            .iter()
            .map(|p| (p.id.clone(), if p.nb_needed == 0 { 1 } else { p.nb_needed as usize }))
            .collect(),
    );

    // Build the list of generator tasks.
    let mut specs: Vec<PolygonInput> = Vec::new();
    if settings.one_country_at_a_time {
        for polygon in &polygons {
            specs.push(polygon.clone());
        }
    } else {
        for polygon in &polygons {
            let count = if grid {
                1
            } else {
                settings.num_of_generators.clamp(1, 10)
            };
            for _ in 0..count {
                specs.push(polygon.clone());
            }
        }
    }

    if settings.one_country_at_a_time {
        for spec in specs {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            run_one(
                app.clone(),
                settings.clone(),
                providers.clone(),
                spec,
                global_seen.clone(),
                found_map.clone(),
                stop.clone(),
                start_time,
                needs.clone(),
                concurrency.clone(),
            )
            .await;
        }
    } else {
        let mut handles = Vec::new();
        for spec in specs {
            let app = app.clone();
            let settings = settings.clone();
            let providers = providers.clone();
            let global_seen = global_seen.clone();
            let found_map = found_map.clone();
            let stop = stop.clone();
            let needs = needs.clone();
            let concurrency = concurrency.clone();
            handles.push(tokio::spawn(async move {
                run_one(
                    app,
                    settings,
                    providers,
                    spec,
                    global_seen,
                    found_map,
                    stop,
                    start_time,
                    needs,
                    concurrency,
                )
                .await;
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
    }

    let _ = app.emit("generation:finished", serde_json::json!({}));
}

async fn run_one(
    app: AppHandle,
    settings: Settings,
    providers: Arc<Providers>,
    polygon: PolygonInput,
    global_seen: Arc<Mutex<HashSet<String>>>,
    found_map: Arc<Mutex<HashMap<String, Vec<Marker>>>>,
    stop: Arc<AtomicBool>,
    start_time: Instant,
    needs: Arc<HashMap<String, usize>>,
    concurrency: Arc<Semaphore>,
) {
    let rings = match PolyRings::from_geometry(&polygon.geometry) {
        Some(rings) => rings,
        None => return,
    };
    let polygon_id = polygon.id.clone();
    let needed = if polygon.nb_needed == 0 {
        1
    } else {
        polygon.nb_needed as usize
    };

    let blue_mask = if settings.only_check_blue_lines {
        Some(
            BlueLineMask::build(
                &providers,
                crate::geo::LatLng::new(rings.bounds.north, rings.bounds.west),
                crate::geo::LatLng::new(rings.bounds.south, rings.bounds.east),
            )
            .await,
        )
    } else {
        None
    };

    let chunk_size = if settings.find_regions { 1usize } else { 75usize };

    if settings.strategy == "grid" {
        let mut grid = match crate::geo::GridGenerator::new(&rings, settings.effective_radius()) {
            Ok(grid) => grid,
            Err(_) => return,
        };

        'outer: loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            {
                let fm = found_map.lock().unwrap();
                if fm.get(&polygon_id).map(|v| v.len()).unwrap_or(0) >= needed {
                    break;
                }
            }

            let batch_size = (chunk_size * 2).max(150);
            let mut batch: Vec<crate::geo::LatLng> = Vec::with_capacity(batch_size);
            for _ in 0..batch_size {
                match grid.next_point() {
                    Some(pt) => {
                        if let Some(mask) = &blue_mask {
                            if !mask.contains(pt.lat, pt.lng, settings.radius) {
                                continue;
                            }
                        }
                        if !rings.contains(pt.lat, pt.lng) {
                            continue;
                        }
                        batch.push(pt);
                    }
                    None => break,
                }
            }

            if batch.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }

            for chunk in batch.chunks(chunk_size) {
                if stop.load(Ordering::Relaxed) {
                    break 'outer;
                }
                {
                    let fm = found_map.lock().unwrap();
                    if fm.get(&polygon_id).map(|v| v.len()).unwrap_or(0) >= needed {
                        break 'outer;
                    }
                }

                let found_lat_lngs: Vec<LatLng> = found_map
                    .lock()
                    .unwrap()
                    .get(&polygon_id)
                    .map(|v| v.iter().map(|m| LatLng::new(m.lat, m.lng)).collect())
                    .unwrap_or_default();

                let mut futs = Vec::with_capacity(chunk.len());
                for pt in chunk {
                    let settings = settings.clone();
                    let providers = providers.clone();
                    let rings = rings.clone();
                    let global_seen = global_seen.clone();
                    let stop = stop.clone();
                    let concurrency = concurrency.clone();
                    let pt = *pt;
                    let found_lat_lngs = found_lat_lngs.clone();
                    futs.push(tokio::spawn(async move {
                        let _permit = concurrency.acquire().await.ok();
                        if stop.load(Ordering::Relaxed) {
                            return Vec::new();
                        }
                        process_sample(&settings, &providers, &rings, pt.lat, pt.lng, &global_seen, &found_lat_lngs, &stop).await
                    }));
                }

                for fut in futs {
                    if stop.load(Ordering::Relaxed) {
                        fut.abort();
                        continue;
                    }
                    let Ok(markers) = fut.await else { continue };
                    for marker in markers {
                        add_marker(
                            &app,
                            &settings,
                            &polygon_id,
                            marker,
                            &found_map,
                            needed,
                            &global_seen,
                            start_time,
                            &needs,
                        )
                        .await;
                    }
                }

                emit_progress(&app, &polygon_id, &found_map, needed);
            }
        }
    } else {
        'outer: loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            {
                let fm = found_map.lock().unwrap();
                if fm.get(&polygon_id).map(|v| v.len()).unwrap_or(0) >= needed {
                    break;
                }
            }

            let n = (needed * 100).min(settings.speed).max(1);
            let mut random_coords: Vec<LatLng> = Vec::with_capacity(n);
            while random_coords.len() < n {
                if stop.load(Ordering::Relaxed) {
                    break 'outer;
                }
                let pt = rings.random_point();
                if !rings.contains(pt.lat, pt.lng) {
                    continue;
                }
                if let Some(mask) = &blue_mask {
                    if !mask.contains(pt.lat, pt.lng, settings.radius) {
                        continue;
                    }
                }
                random_coords.push(pt);
            }

            for chunk in random_coords.chunks(chunk_size) {
                if stop.load(Ordering::Relaxed) {
                    break 'outer;
                }
                {
                    let fm = found_map.lock().unwrap();
                    if fm.get(&polygon_id).map(|v| v.len()).unwrap_or(0) >= needed {
                        break 'outer;
                    }
                }

                let found_lat_lngs: Vec<LatLng> = found_map
                    .lock()
                    .unwrap()
                    .get(&polygon_id)
                    .map(|v| v.iter().map(|m| LatLng::new(m.lat, m.lng)).collect())
                    .unwrap_or_default();

                let mut futs = Vec::with_capacity(chunk.len());
                for pt in chunk {
                    let settings = settings.clone();
                    let providers = providers.clone();
                    let rings = rings.clone();
                    let global_seen = global_seen.clone();
                    let stop = stop.clone();
                    let concurrency = concurrency.clone();
                    let pt = *pt;
                    let found_lat_lngs = found_lat_lngs.clone();
                    futs.push(tokio::spawn(async move {
                        let _permit = concurrency.acquire().await.ok();
                        if stop.load(Ordering::Relaxed) {
                            return Vec::new();
                        }
                        process_sample(&settings, &providers, &rings, pt.lat, pt.lng, &global_seen, &found_lat_lngs, &stop).await
                    }));
                }

                for fut in futs {
                    if stop.load(Ordering::Relaxed) {
                        fut.abort();
                        continue;
                    }
                    let Ok(markers) = fut.await else { continue };
                    for marker in markers {
                        add_marker(
                            &app,
                            &settings,
                            &polygon_id,
                            marker,
                            &found_map,
                            needed,
                            &global_seen,
                            start_time,
                            &needs,
                        )
                        .await;
                    }
                }

                emit_progress(&app, &polygon_id, &found_map, needed);
            }
        }
    }

    // Polygon done event.
    let found_len = found_map
        .lock()
        .unwrap()
        .get(&polygon_id)
        .map(|v| v.len())
        .unwrap_or(0);
    if found_len >= needed {
        let _ = app.emit(
            "generation:polygon-done",
            PolygonDoneEvent {
                polygon_id: polygon_id.clone(),
                found: found_len,
                needed,
            },
        );
    }
}

async fn process_sample(
    settings: &Settings,
    providers: &Providers,
    rings: &PolyRings,
    lat: f64,
    lng: f64,
    global_seen: &Mutex<HashSet<String>>,
    found_lat_lngs: &[LatLng],
    stop: &AtomicBool,
) -> Vec<Marker> {
    if stop.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let filters = Filters::new(settings, providers, rings);
    let candidates = filters.candidates(lat, lng, found_lat_lngs).await;
    let mut out = Vec::new();
    for pano in candidates {
        {
            let mut seen = global_seen.lock().unwrap();
            if !seen.insert(pano.pano_id.clone()) {
                continue;
            }
        }
        let marker = filters.build_marker(&pano).await;
        out.push(marker);
    }
    out
}

async fn add_marker(
    app: &AppHandle,
    settings: &Settings,
    polygon_id: &str,
    marker: Marker,
    found_map: &Arc<Mutex<HashMap<String, Vec<Marker>>>>,
    needed: usize,
    _global_seen: &Arc<Mutex<HashSet<String>>>,
    start_time: Instant,
    needs: &Arc<HashMap<String, usize>>,
) {
    let (found_len, all_complete) = {
        let mut fm = found_map.lock().unwrap();
        let vec = fm.entry(polygon_id.to_string()).or_default();
        if vec.len() >= needed {
            return;
        }
        if vec.iter().any(|m| m.pano_id == marker.pano_id) {
            return;
        }
        vec.push(marker.clone());
        let all_complete = needs
            .iter()
            .all(|(id, need)| fm.get(id).map(|v| v.len()).unwrap_or(0) >= *need);
        (fm.get(polygon_id).map(|v| v.len()).unwrap_or(0), all_complete)
    };

    let _ = app.emit(
        "generation:location",
        LocationEvent {
            polygon_id: polygon_id.to_string(),
            location: marker.clone(),
        },
    );
    let _ = app.emit(
        "generation:progress",
        ProgressEvent {
            polygon_id: polygon_id.to_string(),
            found: found_len,
            needed,
        },
    );

    if settings.notification.enabled {
        let elapsed = start_time.elapsed().as_secs_f64();
        let elapsed_str = format!("{:.1}", elapsed);
        let is_no_blue = marker.update_type.as_deref() == Some("noblueline");
        if settings.notification.any_location && found_len == 1 {
            send_notification(
                app,
                settings,
                "Location Found",
                &format!("Found first location ({}s)", elapsed_str),
                &marker,
                settings.notification.send_to_discord && settings.provider.contains("google"),
            )
            .await;
        }
        if settings.notification.any_no_blue_line && is_no_blue && found_len == 1 {
            send_notification(
                app,
                settings,
                "No blue line Location Found",
                &format!("No blue line location ({}s)", elapsed_str),
                &marker,
                settings.notification.send_to_discord && settings.provider.contains("google"),
            )
            .await;
        }
        if settings.notification.one_polygon_complete && found_len >= needed {
            send_notification(
                app,
                settings,
                "Polygon Completed",
                &format!("Polygon has reached its goal ({}s)", elapsed_str),
                &marker,
                settings.notification.send_to_discord && settings.provider.contains("google"),
            )
            .await;
        }
        if settings.notification.all_polygons_complete && all_complete {
            send_notification(
                app,
                settings,
                "Generation Completed",
                &format!("All polygons have reached their goals ({}s)", elapsed_str),
                &marker,
                settings.notification.send_to_discord && settings.provider.contains("google"),
            )
            .await;
        }
    }
}

async fn send_notification(
    app: &AppHandle,
    settings: &Settings,
    title: &str,
    body: &str,
    _marker: &Marker,
    discord: bool,
) {
    let _ = app.notification().builder().title(title).body(body).show();
    if discord {
        if let Some(url) = settings.notification.discord_webhook.clone() {
            let client = reqwest::Client::new();
            let _ = client
                .post(url)
                .json(&serde_json::json!({"content": format!("**{}**\n{}", title, body)}))
                .send()
                .await;
        }
    }
}

fn emit_progress(app: &AppHandle, polygon_id: &str, found_map: &Arc<Mutex<HashMap<String, Vec<Marker>>>>, needed: usize) {
    let found = found_map
        .lock()
        .unwrap()
        .get(polygon_id)
        .map(|v| v.len())
        .unwrap_or(0);
    let _ = app.emit(
        "generation:progress",
        ProgressEvent {
            polygon_id: polygon_id.to_string(),
            found,
            needed,
        },
    );
}