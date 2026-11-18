use crate::engine;
use crate::models::{GoalUpdate, Goals, PlaceResult, PolygonInput, Settings};
use crate::providers::Providers;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, State};

pub struct GenerationState {
    pub stop: Option<Arc<AtomicBool>>,
    pub task: Option<tokio::task::JoinHandle<()>>,
    /// Live goals for the current run, editable while generating.
    pub goals: Goals,
}

impl Default for GenerationState {
    fn default() -> Self {
        Self { stop: None, task: None, goals: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[tauri::command]
pub async fn start_generation(
    app: AppHandle,
    state: State<'_, Mutex<GenerationState>>,
    settings: Settings,
    polygons: Vec<PolygonInput>,
) -> Result<(), String> {
    // Stop any running generation first.
    {
        let mut st = state.lock().unwrap();
        if let Some(stop) = st.stop.take() {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(task) = st.task.take() {
            task.abort();
        }
    }

    let providers = Arc::new(Providers::new());
    if !settings.api_keys.mapycz.is_empty() {
        *providers.mapycz_key.lock().unwrap() = settings.api_keys.mapycz.clone();
    }

    let goals: Goals = Arc::new(Mutex::new(
        polygons
            .iter()
            .map(|p| (p.id.clone(), engine::polygon_needed(p)))
            .collect(),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut st = state.lock().unwrap();
        st.stop = Some(stop.clone());
        st.goals = goals.clone();
    }

    let app_clone = app.clone();
    let settings_clone = settings.clone();
    let polygons_clone = polygons.clone();
    let providers_clone = providers.clone();
    let stop_clone = stop.clone();
    let goals_clone = goals.clone();
    let task = tokio::spawn(async move {
        engine::run(
            app_clone,
            settings_clone,
            polygons_clone,
            providers_clone,
            stop_clone,
            goals_clone,
        )
        .await;
    });

    {
        let mut st = state.lock().unwrap();
        st.task = Some(task);
    }

    Ok(())
}

#[tauri::command]
pub fn stop_generation(state: State<'_, Mutex<GenerationState>>) -> Result<(), String> {
    let mut st = state.lock().unwrap();
    if let Some(stop) = st.stop.take() {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(task) = st.task.take() {
        task.abort();
    }
    Ok(())
}

/// Update goals for the active run without restarting it.
#[tauri::command]
pub fn update_goals(state: State<'_, Mutex<GenerationState>>, goals: Vec<GoalUpdate>) -> Result<(), String> {
    let st = state.lock().unwrap();
    let mut live = st.goals.lock().unwrap();
    for update in goals {
        if let Some(slot) = live.get_mut(&update.id) {
            *slot = if update.nb_needed == 0 { 1 } else { update.nb_needed as usize };
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn search_places(query: String) -> Result<Vec<PlaceResult>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&addressdetails=1&limit=12",
        urlencoding(&query)
    );
    let client = reqwest::Client::builder()
        .user_agent("Map Generator/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let value: Value = response.json().await.map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    if let Some(array) = value.as_array() {
        for item in array {
            results.push(PlaceResult {
                display_name: item
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                osm_id: item.get("osm_id").and_then(|v| v.as_i64()).unwrap_or(0),
                osm_type: item.get("osm_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
                addresstype: item
                    .get("addresstype")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                address: item.get("address").cloned(),
                lat: item.get("lat").and_then(|v| v.as_str()).map(|s| s.to_string()),
                lon: item.get("lon").and_then(|v| v.as_str()).map(|s| s.to_string()),
            });
        }
    }
    Ok(results)
}

#[tauri::command]
pub async fn fetch_text(url: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("Map Generator/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    response.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_text_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, content).map_err(|e| e.to_string())
}

fn urlencoding(value: &str) -> String {
    // Minimal percent-encoding for query values.
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", byte));
            }
        }
    }
    out
}