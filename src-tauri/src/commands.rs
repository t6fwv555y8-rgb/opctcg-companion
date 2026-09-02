use crate::dto::{ConnectionStatusDto, GameStateDto, OverlaySettings, StateUpdatePayload};
use crate::runtime::{build_observation_status, RuntimeHandles};
use crate::state::AppState;
use optcg_observation::SourceSelection;
use optcg_rules::{CombatAnalysis, LegalAction, MctsResult, ScoredAction, StrategyRecommendation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::dto::{ObservationStatusDto, SourceSelectionDto};
use optcg_rules::RulesEngine;

#[derive(Debug, Serialize, Deserialize)]
pub struct RecommendationsPayload {
    pub beam: Vec<ScoredAction>,
    pub mcts: Option<MctsResult>,
    pub strategy: Option<StrategyRecommendation>,
}

#[command]
pub fn get_game_state(state: State<'_, AppState>) -> GameStateDto {
    GameStateDto::from(&*state.inner().game_state.read())
}

#[command]
pub fn get_connection_status(state: State<'_, AppState>) -> ConnectionStatusDto {
    ConnectionStatusDto::from_state(&*state.inner().game_state.read())
}

#[command]
pub fn get_last_event(state: State<'_, AppState>) -> Option<optcg_core::LastEventInfo> {
    state.inner().game_state.read().last_event.clone()
}

#[command]
pub fn get_event_sequence(state: State<'_, AppState>) -> u64 {
    state.inner().game_state.read().event_sequence
}

#[command]
pub fn get_recommendations(state: State<'_, AppState>) -> RecommendationsPayload {
    let app = state.inner();
    let gs = app.game_state.read().clone();
    let repo = app.repo();

    RecommendationsPayload {
        beam: app.beam.recommend(&gs, &repo).unwrap_or_default(),
        mcts: app.mcts.search(&gs, &repo).ok(),
        strategy: RulesEngine::recommend(&gs, &repo).ok().flatten(),
    }
}

#[command]
pub fn get_combat_analysis(state: State<'_, AppState>) -> Option<CombatAnalysis> {
    let app = state.inner();
    let gs = app.game_state.read().clone();
    let repo = app.repo();
    optcg_rules::CombatMath::analyze_current_combat(&gs, &repo)
}

#[command]
pub fn get_legal_actions(state: State<'_, AppState>) -> Vec<LegalAction> {
    let app = state.inner();
    let gs = app.game_state.read().clone();
    let repo = app.repo();
    optcg_rules::RulesEngine::legal_actions(&gs, &repo).unwrap_or_default()
}

#[command]
pub fn get_state_snapshot(
    state: State<'_, AppState>,
    runtime: State<'_, RuntimeHandles>,
) -> StateUpdatePayload {
    state
        .inner()
        .build_update_payload(Some(build_observation_status(
            &runtime.manager,
            &runtime.pipeline,
        )))
}

#[command]
pub fn get_observation_status(runtime: State<'_, RuntimeHandles>) -> ObservationStatusDto {
    build_observation_status(&runtime.manager, &runtime.pipeline)
}

#[command]
pub async fn set_observation_source(
    app: AppHandle,
    state: State<'_, AppState>,
    runtime: State<'_, RuntimeHandles>,
    selection: SourceSelectionDto,
    replay_path: Option<String>,
) -> Result<ObservationStatusDto, String> {
    if let Some(path) = replay_path {
        runtime.pipeline.set_replay_path(path.into());
    }

    let sel = crate::runtime::selection_from_dto(selection);
    let (result_tx, result_rx) = tokio::sync::mpsc::channel(128);
    crate::runtime::spawn_pipeline_listener(
        result_rx,
        app.clone(),
        Arc::clone(&state.inner().game_state),
        Arc::clone(&runtime.manager),
        Arc::clone(&runtime.pipeline),
    );

    runtime
        .pipeline
        .start(sel, result_tx)
        .await
        .map_err(|e| e.to_string())?;

    Ok(build_observation_status(
        &runtime.manager,
        &runtime.pipeline,
    ))
}

#[command]
pub fn toggle_overlay(
    window: WebviewWindow,
    state: State<'_, AppState>,
    enabled: Option<bool>,
) -> Result<OverlaySettings, String> {
    let mut overlay = state.inner().overlay.write();
    overlay.click_through = enabled.unwrap_or(!overlay.click_through);
    window
        .set_ignore_cursor_events(overlay.click_through)
        .map_err(|e| e.to_string())?;
    Ok(overlay.clone())
}

#[command]
pub fn set_overlay_opacity(state: State<'_, AppState>, opacity: f64) -> OverlaySettings {
    let mut overlay = state.inner().overlay.write();
    overlay.opacity = opacity.clamp(0.3, 1.0);
    overlay.clone()
}

#[command]
pub fn set_click_through(window: WebviewWindow, enabled: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| e.to_string())
}

#[command]
pub fn inject_event(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: String,
) -> Result<(), String> {
    use chrono::Utc;
    use optcg_observation::{ObservationEnvelope, ObservationEvent, ObservationSource};

    let runtime = app
        .try_state::<RuntimeHandles>()
        .ok_or_else(|| "runtime not initialized".to_string())?;

    let envelope = ObservationEnvelope {
        sequence: state.inner().game_state.read().event_sequence + 1,
        timestamp_ms: Utc::now().timestamp_millis(),
        source: ObservationSource::Mock,
        event: ObservationEvent::StructuredRaw {
            raw: payload,
            source: ObservationSource::Mock,
            confidence: 1.0,
        },
    };

    // Process synchronously through reconciler path
    let mut session = optcg_observation::GameSession::new(ObservationSource::Mock);
    {
        let gs = state.inner().game_state.read().clone();
        session.state = gs;
    }
    let mut reconciler = optcg_observation::ObservationReconciler::default();
    reconciler
        .reconcile(&mut session, &envelope.event)
        .map_err(|e| e.to_string())?;
    *state.inner().game_state.write() = session.state;

    emit_state_update(
        &app,
        state.inner(),
        Some(build_observation_status(
            &runtime.manager,
            &runtime.pipeline,
        )),
    );
    Ok(())
}

pub fn emit_state_update(
    app: &AppHandle,
    app_state: &AppState,
    observation: Option<ObservationStatusDto>,
) {
    let payload = app_state.build_update_payload(observation);
    let _ = app.emit("game-state-updated", payload);
}
