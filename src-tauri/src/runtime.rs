use crate::dto::{AdapterInfoDto, ObservationStatusDto, SourceSelectionDto, SyncStateDto};
use optcg_observation::{
    AdapterManager, ObservationPipeline, ObservationPipelineConfig, SourceSelection, SyncState,
};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::mpsc;

pub struct RuntimeHandles {
    pub pipeline: Arc<ObservationPipeline>,
    pub manager: Arc<AdapterManager>,
}

pub fn default_pipeline_config(data_dir: PathBuf) -> ObservationPipelineConfig {
    ObservationPipelineConfig {
        desktop_log_path: data_dir.join("logs"),
        sessions_dir: data_dir.join("sessions"),
        mock_port: 9002,
        browser_port: 9003,
        recording_enabled: std::env::var("OPTCG_RECORD_OBSERVATIONS").is_ok(),
    }
}

pub fn spawn_pipeline_listener(
    mut result_rx: mpsc::Receiver<optcg_observation::PipelineResult>,
    app_handle: tauri::AppHandle,
    game_state: Arc<RwLock<optcg_core::GameState>>,
    manager: Arc<AdapterManager>,
    pipeline: Arc<ObservationPipeline>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(result) = result_rx.recv().await {
            if result.error.is_none() || result.applied {
                let handle = app_handle.clone();
                let gs = Arc::clone(&game_state);
                let mgr = Arc::clone(&manager);
                let pipe = Arc::clone(&pipeline);
                let _ = app_handle.run_on_main_thread(move || {
                    if let Some(app_state) = handle.try_state::<crate::state::AppState>() {
                        let observation = Some(build_observation_status(&mgr, &pipe));
                        crate::commands::emit_state_update(&handle, app_state.inner(), observation);
                    }
                    let _ = gs;
                });
            }
            if let Some(err) = result.error {
                tracing::warn!(error = %err, "pipeline result error");
            }
        }
    });
}

pub fn selection_from_dto(dto: SourceSelectionDto) -> SourceSelection {
    match dto {
        SourceSelectionDto::Auto => SourceSelection::Auto,
        SourceSelectionDto::OneSimulator => SourceSelection::OneSimulator,
        SourceSelectionDto::OptcgSim => SourceSelection::OptcgSim,
        SourceSelectionDto::Mock => SourceSelection::Mock,
        SourceSelectionDto::Replay => SourceSelection::Replay,
        SourceSelectionDto::ScreenVision => SourceSelection::ScreenVision,
    }
}

pub fn selection_to_dto(sel: SourceSelection) -> SourceSelectionDto {
    match sel {
        SourceSelection::Auto => SourceSelectionDto::Auto,
        SourceSelection::OneSimulator => SourceSelectionDto::OneSimulator,
        SourceSelection::OptcgSim => SourceSelectionDto::OptcgSim,
        SourceSelection::Mock => SourceSelectionDto::Mock,
        SourceSelection::Replay => SourceSelectionDto::Replay,
        SourceSelection::ScreenVision => SourceSelectionDto::ScreenVision,
    }
}

fn sync_state_to_dto(state: SyncState) -> SyncStateDto {
    match state {
        SyncState::Synced => SyncStateDto::Synced,
        SyncState::Partial => SyncStateDto::Partial,
        SyncState::Degraded => SyncStateDto::Degraded,
    }
}

pub fn build_observation_status(
    manager: &AdapterManager,
    pipeline: &ObservationPipeline,
) -> ObservationStatusDto {
    let active = manager.active_source();
    let adapters: Vec<AdapterInfoDto> = manager
        .all_statuses()
        .into_iter()
        .map(AdapterInfoDto::from)
        .collect();
    let latency = pipeline.latency();
    let sync_state = sync_state_to_dto(pipeline.sync_state());

    let active_label = active.map(|s| {
        if s == optcg_observation::ObservationSource::DesktopSimulator {
            manager.optcgsim_status().label
        } else {
            s.label().to_string()
        }
    });

    ObservationStatusDto {
        selection: selection_to_dto(manager.selection()),
        active_source: active_label,
        adapters,
        latency,
        searching: active.is_none(),
        sync_state,
    }
}
