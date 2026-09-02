use crate::dto::{AdapterInfoDto, ObservationStatusDto, SourceSelectionDto, SyncStateDto};
use optcg_observation::{
    AdapterManager, AnalysisEligibility, ObservationPipeline, SourceSelection, SyncStatus,
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

use optcg_observation::ObservationPipelineConfig;

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

fn sync_status_to_dto(state: SyncStatus) -> SyncStateDto {
    match state {
        SyncStatus::Synced => SyncStateDto::Synced,
        SyncStatus::Partial => SyncStateDto::Partial,
        SyncStatus::Recovering => SyncStateDto::Recovering,
        SyncStatus::Degraded => SyncStateDto::Degraded,
        SyncStatus::Desynced => SyncStateDto::Desynced,
    }
}

fn hud_state_from(searching: bool, live: bool, sync: SyncStatus) -> crate::dto::HudOperatingState {
    use crate::dto::HudOperatingState;
    if searching {
        return HudOperatingState::Searching;
    }
    if !live {
        return HudOperatingState::Connecting;
    }
    match sync {
        SyncStatus::Synced => HudOperatingState::Live,
        SyncStatus::Partial | SyncStatus::Degraded => HudOperatingState::Partial,
        SyncStatus::Recovering => HudOperatingState::Syncing,
        SyncStatus::Desynced => HudOperatingState::Lost,
    }
}

fn analysis_to_dto(a: AnalysisEligibility) -> crate::dto::AnalysisEligibilityDto {
    let hud_label = a.hud_label().map(|s| s.to_string());
    crate::dto::AnalysisEligibilityDto {
        eligible: a.eligible,
        confidence: a.confidence,
        reasons: a.reasons,
        mode: format!("{:?}", a.mode).to_lowercase(),
        hud_label,
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
    let sync = pipeline.sync_status();
    let sync_state = sync_status_to_dto(sync);
    let live = adapters.iter().any(|a| a.live);
    let searching = active.is_none();

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
        searching,
        sync_state,
        hud_state: hud_state_from(searching, live, sync),
        analysis: analysis_to_dto(pipeline.analysis_eligibility()),
    }
}
