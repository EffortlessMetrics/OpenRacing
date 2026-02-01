//! OpenRacing Tauri Application Entry Point
//!
//! This is the main entry point for the OpenRacing desktop application.
//! It initializes the Tauri runtime and sets up IPC communication with
//! the wheeld service.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]
// Allow unused for now as we're scaffolding
#![allow(dead_code)]

mod commands;

use commands::AppState;
use std::sync::Arc;
use tokio::sync::RwLock;

fn main() {
    // Initialize tracing for debug builds
    #[cfg(debug_assertions)]
    {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    // Create the application state
    let app_state = AppState::new();

    // Build and run the Tauri application
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(RwLock::new(app_state)))
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::get_device_status,
            commands::apply_profile,
            commands::get_telemetry,
            commands::get_service_status,
            commands::emergency_stop,
            commands::connect_service,
            commands::disconnect_service,
        ])
        .setup(|_app| {
            tracing::info!("OpenRacing UI starting up");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
