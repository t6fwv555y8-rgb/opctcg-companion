#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use optcg_database::{AssetParser, Database};
use optcg_events::{FileMonitor, FileMonitorConfig, WebSocketServer, WebSocketServerConfig};
use parking_lot::RwLock;
use state::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let db_path = dirs_data_path().join("optcg_companion.db");
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let database = Database::open(db_path.to_str().unwrap_or("optcg_companion.db"))
        .expect("failed to open database");
    let _ = AssetParser::seed_defaults(&database);

    let game_state = Arc::new(RwLock::new(optcg_core::GameState::new()));

    let ws_state = Arc::clone(&game_state);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            if let Err(e) = WebSocketServer::new(ws_state, WebSocketServerConfig::default())
                .run()
                .await
            {
                tracing::error!(error = %e, "websocket server failed");
            }
        });
    });

    let app_state = AppState::new(database, game_state);

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_game_state,
            commands::get_recommendations,
            commands::get_combat_analysis,
            commands::get_legal_actions,
            commands::get_connection_status,
            commands::set_click_through,
            commands::inject_event,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let app_state: State<AppState> = handle.state();
            let gs = Arc::clone(&app_state.inner().game_state);

            let watch_path = dirs_data_path().join("logs");
            let monitor = FileMonitor::new(FileMonitorConfig {
                watch_path,
                debounce_ms: 100,
            });

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                rt.block_on(async {
                    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
                    if monitor.start(tx).await.is_ok() {
                        while let Some(line) = rx.recv().await {
                            let mut state = gs.write();
                            state.connection.file_monitor_active = true;
                            let _ = optcg_core::Normalizer::apply_log_line(&mut state, &line);
                        }
                    }
                });
            });

            if let Some(window) = app.get_webview_window("main") {
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
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("optcg-companion")
}
