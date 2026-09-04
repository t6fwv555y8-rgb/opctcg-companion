#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod runtime;
mod state;

use optcg_database::{AssetParser, Database};
use optcg_observation::{ObservationPipeline, SourceSelection};
use parking_lot::RwLock;
use runtime::{default_pipeline_config, spawn_pipeline_listener, RuntimeHandles};
use state::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let data_dir = dirs_data_path();
    // Must create the companion data directory itself (not only its parent),
    // otherwise SQLite open panics on first launch on macOS/Windows.
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("failed to create data dir {}: {e}", data_dir.display());
    }
    let _ = std::fs::create_dir_all(data_dir.join("sessions"));
    let _ = std::fs::create_dir_all(data_dir.join("logs"));
    let _ = std::fs::create_dir_all(data_dir.join("calibration"));

    let db_path = data_dir.join("optcg_companion.db");
    let database = match Database::open(db_path.to_str().unwrap_or("optcg_companion.db")) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open database at {}: {e}", db_path.display());
            eprintln!("falling back to in-memory database");
            Database::open_in_memory().expect("in-memory database failed")
        }
    };
    let _ = AssetParser::seed_defaults(&database);

    let game_state = Arc::new(RwLock::new(optcg_core::GameState::new()));
    let pipeline = Arc::new(ObservationPipeline::new(
        Arc::clone(&game_state),
        default_pipeline_config(data_dir),
    ));
    let manager = pipeline.manager();

    let app_state = AppState::new(database, Arc::clone(&game_state));

    tauri::Builder::default()
        .manage(app_state)
        .manage(RuntimeHandles {
            pipeline: Arc::clone(&pipeline),
            manager: Arc::clone(&manager),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_game_state,
            commands::get_connection_status,
            commands::get_last_event,
            commands::get_event_sequence,
            commands::get_recommendations,
            commands::get_combat_analysis,
            commands::get_legal_actions,
            commands::get_state_snapshot,
            commands::get_observation_status,
            commands::set_observation_source,
            commands::toggle_overlay,
            commands::set_overlay_opacity,
            commands::set_click_through,
            commands::inject_event,
            commands::get_debug_status,
            commands::get_calibration_profile,
            commands::save_calibration_profile,
            commands::capture_debug_snapshot,
            commands::set_replay_speed,
            commands::replay_step_forward,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let gs = Arc::clone(&game_state);
            let mgr = Arc::clone(&manager);
            let pipe = Arc::clone(&pipeline);

            let (result_tx, result_rx) = tokio::sync::mpsc::channel(512);
            spawn_pipeline_listener(result_rx, handle, gs, mgr, pipe);

            // Default to OneSimulator so the browser extension can connect on :9003.
            // Override with OPTCG_SOURCE=mock|auto|optcgsim if needed.
            let start_pipeline = Arc::clone(&pipeline);
            let initial_source = match std::env::var("OPTCG_SOURCE")
                .unwrap_or_else(|_| "onesimulator".into())
                .to_lowercase()
                .as_str()
            {
                "mock" => SourceSelection::Mock,
                "optcgsim" | "desktop" => SourceSelection::OptcgSim,
                "auto" => SourceSelection::Auto,
                _ => SourceSelection::OneSimulator,
            };
            tauri::async_runtime::spawn(async move {
                if let Err(e) = start_pipeline.start(initial_source, result_tx).await {
                    tracing::warn!(error = %e, "pipeline start failed");
                    let (tx, _rx) = tokio::sync::mpsc::channel(8);
                    if let Err(e2) = start_pipeline.start(SourceSelection::Mock, tx).await {
                        tracing::error!(error = %e2, "mock fallback also failed");
                    }
                }
            });

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                #[cfg(target_os = "linux")]
                {
                    use tauri::LogicalSize;
                    let _ = window.set_size(LogicalSize::new(420.0, 720.0));
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn dirs_data_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("optcg-companion")
}
