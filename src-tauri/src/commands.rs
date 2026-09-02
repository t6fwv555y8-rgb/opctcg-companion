use crate::state::AppState;
use optcg_core::GameState;
use optcg_rules::{CombatAnalysis, LegalAction, MctsResult, ScoredAction};
use serde::{Deserialize, Serialize};
use tauri::{command, State, WebviewWindow};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub websocket_connected: bool,
    pub file_monitor_active: bool,
    pub latency_ms: u64,
    pub events_processed: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecommendationsPayload {
    pub beam: Vec<ScoredAction>,
    pub mcts: Option<MctsResult>,
}

#[command]
pub fn get_game_state(state: State<'_, AppState>) -> GameState {
    state.inner().game_state.read().clone()
}

#[command]
pub fn get_recommendations(state: State<'_, AppState>) -> RecommendationsPayload {
    let app = state.inner();
    let gs = app.game_state.read().clone();
    let repo = app.repo();

    let beam = app.beam.recommend(&gs, &repo).unwrap_or_default();
    let mcts = app.mcts.search(&gs, &repo).ok();

    RecommendationsPayload { beam, mcts }
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
pub fn get_connection_status(state: State<'_, AppState>) -> ConnectionStatus {
    let gs = state.inner().game_state.read();
    ConnectionStatus {
        websocket_connected: gs.connection.websocket_connected,
        file_monitor_active: gs.connection.file_monitor_active,
        latency_ms: gs.connection.latency_ms,
        events_processed: gs.connection.events_processed,
    }
}

#[command]
pub fn set_click_through(window: WebviewWindow, enabled: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| e.to_string())
}

#[command]
pub fn inject_event(state: State<'_, AppState>, payload: String) -> Result<(), String> {
    let mut gs = state.inner().game_state.write();
    optcg_core::Normalizer::apply_log_line(&mut gs, &payload).map_err(|e| e.to_string())
}
