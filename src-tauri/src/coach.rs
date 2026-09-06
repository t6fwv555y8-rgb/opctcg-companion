use crate::state::AppState;
use optcg_coach::{
    CancelToken, ChatMessage, ChatProvider, CoachError, CoachEvent, CoachSession, CoachStreamEvent,
    DeckContext, EventSink, TurnSummary, SYSTEM_PROMPT,
};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Tauri event channel carrying [`CoachStreamEvent`] frames to the HUD.
///
/// This is the transport the streaming architecture rides on. A Tauri app has
/// no HTTP origin to serve SSE from, and `emit` is already the app's
/// backend-to-webview push channel, so it plays the role SSE would in a
/// browser deployment without adding a second process.
pub const COACH_EVENT: &str = "coach-chat-event";

/// Provider plus conversation state, managed separately from [`AppState`] so a
/// streaming turn can own everything it needs without borrowing app state.
pub struct CoachRuntime {
    provider: Arc<dyn ChatProvider>,
    session: Arc<Mutex<CoachSession>>,
}

impl CoachRuntime {
    pub fn from_env() -> Self {
        let provider = optcg_coach::provider_from_env();
        tracing::info!(
            provider = %provider.label(),
            live = provider.is_live(),
            "coach provider selected"
        );
        Self {
            provider,
            session: Arc::new(Mutex::new(CoachSession::new())),
        }
    }

    pub fn status(&self) -> CoachStatusDto {
        let session = self.session.lock();
        CoachStatusDto {
            provider: self.provider.label(),
            live: self.provider.is_live(),
            busy: session.is_busy(),
            active_turn: session.active_turn(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoachStatusDto {
    /// Model name, or `Offline coach` when no API key is configured.
    pub provider: String,
    /// True when answers come from a real model API.
    pub live: bool,
    pub busy: bool,
    pub active_turn: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoachTurnDto {
    pub turn_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoachHistoryDto {
    pub messages: Vec<ChatMessage>,
    pub status: CoachStatusDto,
}

/// A sink that forwards every frame for one turn to the HUD.
fn emit_sink(app: AppHandle, turn_id: u64) -> EventSink {
    Arc::new(move |event: CoachEvent| {
        if let Err(e) = app.emit(COACH_EVENT, CoachStreamEvent::new(turn_id, event)) {
            tracing::warn!(error = %e, "could not emit coach event");
        }
    })
}

/// Map the app's deck DTOs onto the coach's deck context.
fn deck_context(state: &AppState) -> DeckContext {
    let (yours, opponent) = state.deck_infos();
    let strategy = state.cached_deck_strategy();

    DeckContext {
        your_deck: yours.name.clone(),
        your_leader: leader_label(&yours),
        your_list: yours
            .list_entries
            .iter()
            .map(|entry| format!("{}x {} ({})", entry.quantity, entry.name, entry.card_id))
            .collect(),
        opponent_deck: opponent.name.clone(),
        opponent_leader: leader_label(&opponent),
        plan: strategy.as_ref().map(|brief| brief.your_plan.clone()),
        vs_opponent: strategy.as_ref().map(|brief| brief.vs_opponent.clone()),
    }
}

fn leader_label(deck: &crate::dto::DeckInfoDto) -> String {
    if deck.leader_name.is_empty() {
        deck.leader_id.clone()
    } else if deck.leader_id.is_empty() {
        deck.leader_name.clone()
    } else {
        format!("{} · {}", deck.leader_name, deck.leader_id)
    }
}

/// Run the read-only grounding tools and assemble the system prompt.
///
/// Synchronous on purpose: it reads game state and the card database, so
/// keeping it await-free means no lock is ever held across a suspension point.
fn build_briefing(state: &AppState, sink: &EventSink) -> String {
    let decks = deck_context(state);
    let game_state = state.game_state.read();
    let repo = state.repo();
    let context = optcg_coach::build_context(&game_state, &repo, &decks, sink);
    format!(
        "{SYSTEM_PROMPT}\n\n# MATCH BRIEFING\n{}",
        context.to_prompt()
    )
}

/// Stream one turn, then record it and emit exactly one terminal frame.
async fn run_turn(
    provider: Arc<dyn ChatProvider>,
    session: Arc<Mutex<CoachSession>>,
    messages: Vec<ChatMessage>,
    sink: EventSink,
    turn_id: u64,
    cancel: CancelToken,
) {
    let summary = match provider.stream_chat(&messages, &sink, &cancel).await {
        Ok(answer) => {
            // A turn the user has already superseded must not append its answer
            // or emit a terminal frame over the newer one.
            if !session.lock().finish_turn(turn_id, &answer) {
                tracing::debug!(turn_id, "dropping superseded coach turn");
                return;
            }
            TurnSummary::complete(answer)
        }
        Err(error) => {
            let mut guard = session.lock();
            if guard.is_superseded(turn_id) {
                tracing::debug!(turn_id, "dropping superseded coach turn");
                return;
            }
            guard.abandon_turn(turn_id);
            drop(guard);

            match error {
                CoachError::Cancelled => TurnSummary::cancelled(),
                e => {
                    tracing::warn!(error = %e, turn_id, "coach turn failed");
                    TurnSummary::failed(e.to_string())
                }
            }
        }
    };

    sink(CoachEvent::Done(summary));
}

/// Ask the coach a question. Returns as soon as the turn is registered; the
/// answer arrives as [`COACH_EVENT`] frames.
#[tauri::command]
pub fn coach_send_message(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    coach: tauri::State<'_, CoachRuntime>,
    message: String,
) -> Result<CoachTurnDto, String> {
    let (turn_id, cancel) = coach.session.lock().begin_turn(&message)?;
    let sink = emit_sink(app, turn_id);

    let briefing = build_briefing(state.inner(), &sink);
    let messages = coach.session.lock().prompt_with(briefing);

    let provider = Arc::clone(&coach.provider);
    let session = Arc::clone(&coach.session);
    tauri::async_runtime::spawn(async move {
        run_turn(provider, session, messages, sink, turn_id, cancel).await;
    });

    Ok(CoachTurnDto { turn_id })
}

/// Stop the streaming turn. The partial answer stays on screen.
#[tauri::command]
pub fn coach_cancel(coach: tauri::State<'_, CoachRuntime>) -> Option<u64> {
    coach.session.lock().cancel_active()
}

/// Clear the conversation.
#[tauri::command]
pub fn coach_reset(coach: tauri::State<'_, CoachRuntime>) -> CoachHistoryDto {
    coach.session.lock().reset();
    coach_history(coach)
}

#[tauri::command]
pub fn coach_history(coach: tauri::State<'_, CoachRuntime>) -> CoachHistoryDto {
    let messages = coach.session.lock().history().to_vec();
    CoachHistoryDto {
        messages,
        status: coach.status(),
    }
}

#[tauri::command]
pub fn coach_status(coach: tauri::State<'_, CoachRuntime>) -> CoachStatusDto {
    coach.status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_coach::{provider::test_support::recording_sink, OfflineProvider};
    use optcg_database::{AssetParser, Database};
    use parking_lot::RwLock;

    fn app_state() -> AppState {
        let database = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&database).unwrap();
        let dir = std::env::temp_dir().join(format!("optcg-coach-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        AppState::new(
            database,
            Arc::new(RwLock::new(optcg_core::GameState::new())),
            dir,
        )
    }

    #[test]
    fn briefing_includes_the_system_prompt_and_live_board() {
        let state = app_state();
        let (sink, recorder) = recording_sink();

        let briefing = build_briefing(&state, &sink);

        assert!(briefing.starts_with("You are the in-game coach"));
        assert!(briefing.contains("# MATCH BRIEFING"));
        assert!(briefing.contains("## Board"));
        assert!(
            !recorder.events().is_empty(),
            "grounding should report its steps"
        );
    }

    #[test]
    fn deck_context_reflects_the_active_saved_deck() {
        let state = app_state();
        state
            .save_deck(
                None,
                None,
                "Deck: Red Luffy Aggro\nLeader: ST01-001\n4x ST01-002",
            )
            .unwrap();

        let context = deck_context(&state);
        assert_eq!(context.your_deck, "Red Luffy Aggro");
        assert!(context.your_leader.contains("ST01-001"));
        assert!(
            context.your_list.iter().any(|line| line.contains("4x Usopp")),
            "exact list should reach the coach: {:?}",
            context.your_list
        );
    }

    #[test]
    fn deck_context_is_empty_without_a_saved_deck() {
        let state = app_state();
        let context = deck_context(&state);
        assert!(
            context.your_list.is_empty(),
            "no list should be claimed when none is saved"
        );
    }

    #[tokio::test]
    async fn a_turn_streams_then_emits_one_terminal_frame() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();
        let messages = session.lock().prompt_with("## Phase guidance\nAttack.".into());
        let (sink, recorder) = recording_sink();

        run_turn(
            Arc::new(OfflineProvider::instant()),
            Arc::clone(&session),
            messages,
            sink,
            turn_id,
            cancel,
        )
        .await;

        let events = recorder.events();
        let terminals: Vec<_> = events.iter().filter(|e| e.is_terminal()).collect();
        assert_eq!(terminals.len(), 1, "expected exactly one Done frame");
        assert!(matches!(
            terminals[0],
            CoachEvent::Done(TurnSummary {
                reason: optcg_coach::FinishReason::Complete,
                ..
            })
        ));
        assert!(!recorder.text().is_empty(), "the answer should have streamed");
        assert!(!session.lock().is_busy(), "the turn should be closed out");
        assert_eq!(session.lock().history().len(), 2);
    }

    #[tokio::test]
    async fn a_cancelled_turn_reports_cancelled_and_keeps_no_answer() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();
        let messages = session.lock().prompt_with("## Phase guidance\nAttack.".into());
        let (sink, recorder) = recording_sink();
        cancel.cancel();

        run_turn(
            Arc::new(OfflineProvider::instant()),
            Arc::clone(&session),
            messages,
            sink,
            turn_id,
            cancel,
        )
        .await;

        let events = recorder.events();
        assert!(matches!(
            events.last(),
            Some(CoachEvent::Done(TurnSummary {
                reason: optcg_coach::FinishReason::Cancelled,
                ..
            }))
        ));
        assert_eq!(
            session.lock().history().len(),
            1,
            "only the question survives a cancelled turn"
        );
    }

    #[tokio::test]
    async fn a_superseded_turn_emits_nothing() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (first_turn, first_cancel) = session.lock().begin_turn("q1").unwrap();
        let messages = session.lock().prompt_with("## Phase guidance\nAttack.".into());

        // The user asks again before the first answer lands.
        session.lock().begin_turn("q2").unwrap();

        let (sink, recorder) = recording_sink();
        run_turn(
            Arc::new(OfflineProvider::instant()),
            Arc::clone(&session),
            messages,
            sink,
            first_turn,
            first_cancel,
        )
        .await;

        assert!(
            !recorder.events().iter().any(CoachEvent::is_terminal),
            "a superseded turn must not emit a terminal frame"
        );
        assert_eq!(
            session.lock().active_turn(),
            Some(first_turn + 1),
            "the newer turn stays in control"
        );
    }
}
