pub mod commands;
pub mod dates;
pub mod engine;
pub mod filters;
pub mod geo;
pub mod models;
pub mod protobuf;
pub mod providers;
pub mod tiles;

use commands::GenerationState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(std::sync::Mutex::new(GenerationState::default()))
        .invoke_handler(tauri::generate_handler![
            commands::start_generation,
            commands::stop_generation,
            commands::search_places,
            commands::fetch_text,
            commands::read_text_file,
            commands::write_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}