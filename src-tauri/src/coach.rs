use crate::dto::DeckOrigin;
use crate::state::AppState;
use optcg_coach::{
    AutoDecision, AutoTrigger, CancelReason, CancelToken, ChatMessage, ChatProvider, CoachError,
    CoachEvent, CoachSession, CoachStreamEvent, CoalescingSink, ContextScope, DeckContext,
    EventSink, FlushTicker, ListStanding, StateFingerprint, TurnKind, TurnSummary,
    DEFAULT_FLUSH_INTERVAL_MS, SYSTEM_PROMPT,
};
use optcg_scouting::{DeckMap, StrategyRead};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Tauri event channel carrying [`CoachStreamEvent`] frames to the HUD.
///
/// This is the transport the streaming architecture rides on. A Tauri app has
/// no HTTP origin to serve SSE from, and `emit` is already the app's
/// backend-to-webview push channel, so it plays the role SSE would in a
/// browser deployment without adding a second process.
pub const COACH_EVENT: &str = "coach://event";

/// Provider plus conversation state, managed separately from [`AppState`] so a
/// streaming turn can own everything it needs without borrowing app state.
pub struct CoachRuntime {
    provider: Arc<dyn ChatProvider>,
    session: Arc<Mutex<CoachSession>>,
    auto: Arc<Mutex<AutoTrigger>>,
    scope: Arc<Mutex<ContextScope>>,
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
            auto: Arc::new(Mutex::new(AutoTrigger::default())),
            scope: Arc::new(Mutex::new(ContextScope::default())),
        }
    }

    pub fn status(&self) -> CoachStatusDto {
        // Each bound before taking the session lock; none are held together.
        let auto_enabled = self.auto.lock().is_enabled();
        let context = *self.scope.lock();
        let session = self.session.lock();
        CoachStatusDto {
            provider: self.provider.label(),
            live: self.provider.is_live(),
            busy: session.is_busy(),
            active_turn: session.active_turn(),
            automatic: session.active_kind() == Some(TurnKind::Auto),
            auto_enabled,
            context,
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
    /// True when the streaming turn was triggered by a board change rather
    /// than asked for.
    pub automatic: bool,
    /// True when board changes trigger reads on their own.
    pub auto_enabled: bool,
    /// What the next turn will send to the model.
    pub context: ContextScope,
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
        your_list: list_lines(&yours),
        your_list_standing: standing(&yours),
        opponent_deck: opponent.name.clone(),
        opponent_leader: leader_label(&opponent),
        opponent_list: list_lines(&opponent),
        opponent_list_standing: standing(&opponent),
        opponent_scouting: scouting_brief(state, &opponent),
        plan: strategy.as_ref().map(|brief| brief.your_plan.clone()),
        vs_opponent: strategy.as_ref().map(|brief| brief.vs_opponent.clone()),
    }
}

/// How many mapped cards the briefing carries.
///
/// The map is sorted most-established first, so this takes the part of it worth
/// planning around and leaves the long tail of one-off sightings out of the
/// prompt.
const SCOUTED_CARDS: usize = 20;

/// What past games say about the deck across the table.
///
/// Returns `None` when their list is already known, since a scouting estimate
/// of a deck we have in full is noise, and when nothing has been seen yet.
fn scouting_brief(
    state: &AppState,
    opponent: &crate::dto::DeckInfoDto,
) -> Option<optcg_coach::ScoutingBrief> {
    if opponent.origin == DeckOrigin::Attached {
        return None;
    }
    let profile = state.scouting_for(&opponent.leader_id)?;
    let map = DeckMap::from_profile(&profile)?;
    let read = StrategyRead::from_profile(&profile);
    let repo = state.repo();

    let likely_cards = map
        .cards
        .iter()
        .take(SCOUTED_CARDS)
        .map(|card| {
            let name = repo
                .get_by_id(&card.card_id)
                .map(|def| def.name)
                .unwrap_or_else(|_| card.card_id.clone());
            format!(
                "{}x {} ({}) — {} of {} games",
                card.likely_copies, name, card.card_id, card.games_seen, map.games
            )
        })
        .collect();

    Some(optcg_coach::ScoutingBrief {
        games: map.games,
        reliability: map.reliability.label().to_string(),
        pace: read
            .as_ref()
            .map(|read| read.pace.label().to_string())
            .unwrap_or_else(|| "not yet established".into()),
        likely_cards,
        notes: read.map(|read| read.notes).unwrap_or_default(),
        mapped_copies: map.mapped_copies(),
    })
}

fn list_lines(deck: &crate::dto::DeckInfoDto) -> Vec<String> {
    deck.list_entries
        .iter()
        .map(|entry| format!("{}x {} ({})", entry.quantity, entry.name, entry.card_id))
        .collect()
}

fn standing(deck: &crate::dto::DeckInfoDto) -> ListStanding {
    match deck.origin {
        DeckOrigin::Attached if !deck.list_entries.is_empty() => ListStanding::Confirmed,
        DeckOrigin::Presumed if !deck.list_entries.is_empty() => ListStanding::Presumed,
        _ => ListStanding::Unknown,
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

/// The grounded prompt plus the position it was read from.
struct Briefing {
    prompt: String,
    /// Absent when the board was not shared, so the turn has no position to go
    /// stale against.
    fingerprint: Option<StateFingerprint>,
}

/// Run the read-only grounding tools and assemble the system prompt.
///
/// Synchronous on purpose: it reads game state and the card database, so
/// keeping it await-free means no lock is ever held across a suspension point.
fn build_briefing(state: &AppState, scope: ContextScope, sink: &EventSink) -> Briefing {
    // Skip the deck read entirely when it is not being shared, rather than
    // gathering it and dropping it.
    let decks = if scope.deck {
        deck_context(state)
    } else {
        DeckContext::default()
    };
    let game_state = state.game_state.read();
    let repo = state.repo();
    let context = optcg_coach::build_context(&game_state, &repo, &decks, scope, sink);
    Briefing {
        prompt: format!(
            "{SYSTEM_PROMPT}\n\n# MATCH BRIEFING\n{}",
            context.to_prompt()
        ),
        fingerprint: context.fingerprint.clone(),
    }
}

/// Ground a turn off the async runtime.
///
/// Grounding hits SQLite and the rules engine, so it runs on the blocking pool
/// rather than occupying an async worker. Returns `None` only if app state is
/// gone (shutdown) or the blocking task panicked.
async fn ground_turn(app: AppHandle, scope: ContextScope, sink: EventSink) -> Option<Briefing> {
    tokio::task::spawn_blocking(move || {
        let state = app.try_state::<AppState>()?;
        Some(build_briefing(state.inner(), scope, &sink))
    })
    .await
    .inspect_err(|e| tracing::warn!(error = %e, "grounding task failed"))
    .ok()
    .flatten()
}

/// Translate a cancellation into the terminal frame the UI should see.
fn summarize_cancellation(cancel: &CancelToken) -> TurnSummary {
    match cancel.reason() {
        Some(CancelReason::Stale) => TurnSummary::interrupted(),
        _ => TurnSummary::cancelled(),
    }
}

/// Ground, stream, and close out one turn, emitting exactly one terminal frame.
///
/// Grounding runs first so `tool_run` frames reach the HUD before any text,
/// and so the position the answer is based on is known before it is written.
async fn run_turn<G, F>(
    ground: G,
    provider: Arc<dyn ChatProvider>,
    session: Arc<Mutex<CoachSession>>,
    coalescing: Arc<CoalescingSink>,
    sink: EventSink,
    turn_id: u64,
    cancel: CancelToken,
) where
    G: FnOnce(EventSink) -> F,
    F: std::future::Future<Output = Option<Briefing>>,
{
    // Keeps buffered text moving even if the model stalls mid-answer. Dropped
    // on every exit path below, which stops the task.
    let _ticker = FlushTicker::spawn(
        Arc::clone(&coalescing),
        Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
    );

    let Some(briefing) = ground(Arc::clone(&sink)).await else {
        session.lock().abandon_turn(turn_id);
        sink(CoachEvent::Done(TurnSummary::failed(
            "could not read the game state",
        )));
        return;
    };

    // Grounding is not instant, so the turn may have been cancelled or replaced
    // while it ran.
    let messages = {
        let mut guard = session.lock();
        if guard.is_superseded(turn_id) {
            tracing::debug!(turn_id, "dropping superseded coach turn");
            return;
        }
        if guard.active_turn() != Some(turn_id) {
            drop(guard);
            sink(CoachEvent::Done(summarize_cancellation(&cancel)));
            return;
        }
        if let Some(position) = briefing.fingerprint {
            guard.record_grounding(turn_id, position);
        }
        guard.prompt_with(briefing.prompt)
    };

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
                CoachError::Cancelled => summarize_cancellation(&cancel),
                e => {
                    tracing::warn!(error = %e, turn_id, "coach turn failed");
                    TurnSummary::failed(e.to_string())
                }
            }
        }
    };

    sink(CoachEvent::Done(summary));
}

/// Start streaming a turn that the session has already registered.
fn spawn_turn(app: AppHandle, coach: &CoachRuntime, turn_id: u64, cancel: CancelToken) {
    let coalescing = Arc::new(CoalescingSink::new(emit_sink(app.clone(), turn_id)));
    let sink = coalescing.as_event_sink();

    // Captured at turn start, so toggling sharing mid-answer cannot change
    // what was already sent.
    let scope = *coach.scope.lock();
    let provider = Arc::clone(&coach.provider);
    let session = Arc::clone(&coach.session);
    tauri::async_runtime::spawn(async move {
        run_turn(
            |sink| ground_turn(app, scope, sink),
            provider,
            session,
            coalescing,
            sink,
            turn_id,
            cancel,
        )
        .await;
    });
}

/// Ask the coach a question. Returns as soon as the turn is registered; the
/// answer arrives as [`COACH_EVENT`] frames.
#[tauri::command]
pub fn coach_send_message(
    app: AppHandle,
    coach: tauri::State<'_, CoachRuntime>,
    message: String,
) -> Result<CoachTurnDto, String> {
    let (turn_id, cancel) = coach.session.lock().begin_turn(&message)?;
    spawn_turn(app, coach.inner(), turn_id, cancel);
    Ok(CoachTurnDto { turn_id })
}

/// Turn unprompted board reads on or off.
#[tauri::command]
pub fn coach_set_auto(coach: tauri::State<'_, CoachRuntime>, enabled: bool) -> CoachStatusDto {
    coach.auto.lock().set_enabled(enabled);
    tracing::info!(enabled, "automatic coach reads toggled");
    coach.status()
}

/// Choose what the coach may send to the model.
///
/// Withdrawing the board also stops automatic reads, which exist to answer
/// board changes and have nothing to say without it.
#[tauri::command]
pub fn coach_set_context(
    coach: tauri::State<'_, CoachRuntime>,
    board: bool,
    deck: bool,
) -> CoachStatusDto {
    *coach.scope.lock() = ContextScope { board, deck };
    if !board {
        coach.auto.lock().set_enabled(false);
    }
    tracing::info!(board, deck, "coach context sharing changed");
    coach.status()
}

/// Read the board unprompted when it settles on a new position worth advice.
///
/// Called on a fixed cadence rather than only on state updates, because the
/// settle window has to be able to expire after the last change arrives.
/// Cheap when idle: it returns before touching game state unless automatic
/// reads are on and nothing is already streaming.
pub fn poll_auto_trigger(app: &AppHandle) {
    let Some(coach) = app.try_state::<CoachRuntime>() else {
        return;
    };
    if !coach.auto.lock().is_enabled() {
        return;
    }
    // An automatic read answers a board change, so it is pointless once the
    // board is being withheld.
    if !coach.scope.lock().board {
        return;
    }
    // Never talk over a turn already in flight, the user's least of all.
    if coach.session.lock().is_busy() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let (position, at_decision_point) = {
        let game_state = state.game_state.read();
        (
            optcg_coach::fingerprint(&game_state),
            optcg_coach::is_decision_point(&game_state),
        )
    };

    let decision = coach
        .auto
        .lock()
        .observe(&position, at_decision_point, Instant::now());
    if decision != AutoDecision::Fire {
        return;
    }

    let started = coach
        .session
        .lock()
        .begin_auto_turn(optcg_coach::AUTO_QUESTION);
    match started {
        Ok((turn_id, cancel)) => {
            tracing::debug!(turn_id, position = %position.label, "reading the board unprompted");
            spawn_turn(app.clone(), coach.inner(), turn_id, cancel);
        }
        Err(e) => tracing::warn!(error = %e, "could not start an automatic read"),
    }
}

/// Stop the streaming turn. The partial answer stays on screen.
#[tauri::command]
pub fn coach_cancel(coach: tauri::State<'_, CoachRuntime>) -> Option<u64> {
    coach.session.lock().cancel_active()
}

/// Interrupt the streaming turn if the board has moved away from the position
/// it was grounded on, because that answer is now about a position that no
/// longer exists.
///
/// Called from the central state broadcast, so it runs on every observed
/// change. Cheap when idle: it returns before reading game state if no turn is
/// streaming. Cancelling rather than emitting here means the streaming task
/// still emits the one terminal frame, keeping it ordered after any buffered
/// text.
pub fn interrupt_if_board_changed(app: &AppHandle) {
    let Some(coach) = app.try_state::<CoachRuntime>() else {
        return;
    };
    if !coach.session.lock().is_busy() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let current = {
        let game_state = state.game_state.read();
        optcg_coach::fingerprint(&game_state)
    };

    // Bound in its own statement so the guard is released here rather than
    // being held for the body of the `if let`.
    let interrupted = coach.session.lock().interrupt_if_stale(&current);
    if let Some(turn_id) = interrupted {
        tracing::debug!(turn_id, position = %current.label, "board moved; interrupting coach turn");
    }
}

/// Clear the conversation and let the current board be read again.
#[tauri::command]
pub fn coach_reset(coach: tauri::State<'_, CoachRuntime>) -> CoachHistoryDto {
    coach.session.lock().reset();
    coach.auto.lock().reset();
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
        AppState::new(
            database,
            Arc::new(RwLock::new(optcg_core::GameState::new())),
            isolated_data_dir(),
        )
    }

    /// An app state whose board the test can drive.
    fn app_state_with_board() -> (AppState, Arc<parking_lot::RwLock<optcg_core::GameState>>) {
        let database = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&database).unwrap();
        let board = Arc::new(RwLock::new(optcg_core::GameState::new()));
        let state = AppState::new(database, Arc::clone(&board), isolated_data_dir());
        (state, board)
    }

    /// Watch `games` games in which the opponent plays `card`.
    fn scout_games(
        state: &AppState,
        board: &Arc<parking_lot::RwLock<optcg_core::GameState>>,
        games: u128,
        card: &str,
    ) {
        for game in 1..=games {
            {
                let mut gs = board.write();
                gs.game_id = uuid::Uuid::from_u128(game);
                gs.turn_number = 3;
                gs.player_two_mut().leader.card_id = "OP17-079".into();
                gs.player_two_mut().characters = vec![optcg_core::CardInstance::new(
                    card,
                    1,
                    optcg_core::Zone::Character,
                )];
            }
            state.scout_position();
        }
    }

    #[test]
    fn what_we_learned_about_them_reaches_the_deck_context() {
        let (state, board) = app_state_with_board();
        scout_games(&state, &board, 4, "OP17-080");

        let scouting = deck_context(&state)
            .opponent_scouting
            .expect("four games against this leader should reach the coach");

        assert_eq!(scouting.games, 4);
        assert_eq!(scouting.reliability, "fair");
        assert!(
            scouting
                .likely_cards
                .iter()
                .any(|line| line.contains("Usopp") && line.contains("4 of 4 games")),
            "the card and its rate should both travel: {:?}",
            scouting.likely_cards
        );
    }

    #[test]
    fn an_unscouted_opponent_adds_nothing_to_the_context() {
        let state = app_state();

        assert!(
            deck_context(&state).opponent_scouting.is_none(),
            "with nothing seen there is nothing to report"
        );
    }

    #[test]
    fn scouting_is_dropped_once_we_hold_their_real_list() {
        let (state, board) = app_state_with_board();
        scout_games(&state, &board, 4, "OP17-080");
        assert!(deck_context(&state).opponent_scouting.is_some());

        let theirs = state
            .save_deck(
                optcg_rules::Side::Opponent,
                None,
                None,
                "Deck: Black Elbaph\nLeader: OP17-079\n4x OP17-080",
            )
            .unwrap();
        state
            .set_deck_source(optcg_rules::Side::Opponent, Some(&theirs.id))
            .unwrap();

        assert!(
            deck_context(&state).opponent_scouting.is_none(),
            "an estimate of a deck we hold in full is noise"
        );
    }

    /// A directory of its own per call. Tests in a crate share one process and
    /// run concurrently, so a directory keyed only on the process id would let
    /// them collide over the same `deck_collection.json`.
    fn isolated_data_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("optcg-coach-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn withholding_the_board_leaves_the_briefing_without_a_position() {
        let state = app_state();
        let (sink, _) = recording_sink();

        let briefing = build_briefing(
            &state,
            ContextScope {
                board: false,
                deck: true,
            },
            &sink,
        );

        assert!(
            briefing.fingerprint.is_none(),
            "no position means a board change cannot interrupt the answer"
        );
        assert!(!briefing.prompt.contains("## Board"));
        assert!(
            briefing.prompt.contains("## Withheld"),
            "the model must be told what it cannot see"
        );
    }

    #[test]
    fn briefing_includes_the_system_prompt_and_live_board() {
        let state = app_state();
        let (sink, recorder) = recording_sink();

        let briefing = build_briefing(&state, ContextScope::default(), &sink);

        assert!(briefing.prompt.starts_with("You are the in-game coach"));
        assert!(briefing.prompt.contains("# MATCH BRIEFING"));
        assert!(briefing.prompt.contains("## Board"));
        assert!(briefing.prompt.contains("## Opponent counter range"));
        assert!(
            briefing
                .fingerprint
                .is_some_and(|position| !position.digest.is_empty()),
            "the briefing must record the position it read"
        );
        assert!(
            !recorder.events().is_empty(),
            "grounding should report its steps"
        );
    }

    #[test]
    fn tool_frames_all_precede_the_first_text_delta() {
        let state = app_state();
        let (sink, recorder) = recording_sink();

        build_briefing(&state, ContextScope::default(), &sink);
        let events = recorder.events();

        assert!(
            !events.iter().any(|e| matches!(e, CoachEvent::TextDelta(_))),
            "grounding must finish before any text is produced"
        );
        assert!(
            events.iter().any(|e| matches!(e, CoachEvent::ToolRun(_))),
            "grounding should report tools: {events:?}"
        );
        assert!(
            matches!(events.last(), Some(CoachEvent::StateSync(_))),
            "state_sync should close grounding: {events:?}"
        );
    }

    #[test]
    fn deck_context_reflects_the_active_saved_deck() {
        let state = app_state();
        state
            .save_deck(
                optcg_rules::Side::You,
                None,
                None,
                "Deck: Red Luffy Aggro\nLeader: ST01-001\n4x ST01-002",
            )
            .unwrap();

        let context = deck_context(&state);
        assert_eq!(context.your_deck, "Red Luffy Aggro");
        assert!(context.your_leader.contains("ST01-001"));
        assert!(
            context
                .your_list
                .iter()
                .any(|line| line.contains("4x Usopp")),
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

    const BRIEFING: &str = "## Phase guidance\nAttack the leader.";

    fn test_briefing() -> Briefing {
        Briefing {
            prompt: BRIEFING.into(),
            fingerprint: Some(StateFingerprint {
                label: "turn 4".into(),
                digest: "turn-4".into(),
            }),
        }
    }

    /// Drive a turn with grounding stubbed out, returning the frames the HUD
    /// would have seen.
    async fn drive_turn(
        session: &Arc<Mutex<CoachSession>>,
        turn_id: u64,
        cancel: CancelToken,
    ) -> optcg_coach::provider::test_support::Recorder {
        let (emit, recorder) = recording_sink();
        let coalescing = Arc::new(CoalescingSink::new(emit));
        let sink = coalescing.as_event_sink();

        run_turn(
            |_sink| async { Some(test_briefing()) },
            Arc::new(OfflineProvider::instant()),
            Arc::clone(session),
            coalescing,
            sink,
            turn_id,
            cancel,
        )
        .await;

        recorder
    }

    #[tokio::test]
    async fn a_turn_streams_then_emits_one_terminal_frame() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();

        let recorder = drive_turn(&session, turn_id, cancel).await;

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
        assert!(
            events.last().is_some_and(CoachEvent::is_terminal),
            "the terminal frame must come last, after all batched text"
        );
        assert!(
            !recorder.text().is_empty(),
            "the answer should have streamed"
        );
        assert!(!session.lock().is_busy(), "the turn should be closed out");
        assert_eq!(session.lock().history().len(), 2);
    }

    #[tokio::test]
    async fn grounding_records_the_position_for_staleness_checks() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();

        // Interrupt after grounding but before the turn is driven to completion
        // by holding the position the stub reports.
        let (emit, _recorder) = recording_sink();
        let coalescing = Arc::new(CoalescingSink::new(emit));
        let sink = coalescing.as_event_sink();
        let watcher = Arc::clone(&session);

        run_turn(
            move |_sink| async move {
                let briefing = test_briefing();
                let position = briefing.fingerprint.clone().expect("stub has a position");
                // Nothing recorded yet, so the turn cannot be stale.
                assert_eq!(watcher.lock().interrupt_if_stale(&position), None);
                Some(briefing)
            },
            Arc::new(OfflineProvider::instant()),
            Arc::clone(&session),
            coalescing,
            sink,
            turn_id,
            cancel,
        )
        .await;

        assert_eq!(session.lock().history().len(), 2, "the turn completed");
    }

    #[tokio::test]
    async fn an_auto_turn_streams_without_joining_the_conversation() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session
            .lock()
            .begin_auto_turn(optcg_coach::AUTO_QUESTION)
            .unwrap();

        let recorder = drive_turn(&session, turn_id, cancel).await;

        assert!(
            !recorder.text().is_empty(),
            "an automatic read should stream like any other turn"
        );
        assert!(
            matches!(
                recorder.events().last(),
                Some(CoachEvent::Done(TurnSummary {
                    reason: optcg_coach::FinishReason::Complete,
                    ..
                }))
            ),
            "got {:?}",
            recorder.events()
        );
        assert!(
            session.lock().history().is_empty(),
            "unprompted reads must not accumulate in the conversation"
        );
        assert!(!session.lock().is_busy());
    }

    #[tokio::test]
    async fn a_cancelled_turn_reports_cancelled_and_keeps_no_answer() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();
        session.lock().cancel_active();

        let recorder = drive_turn(&session, turn_id, cancel).await;

        assert!(
            matches!(
                recorder.events().last(),
                Some(CoachEvent::Done(TurnSummary {
                    reason: optcg_coach::FinishReason::Cancelled,
                    ..
                }))
            ),
            "got {:?}",
            recorder.events()
        );
        assert_eq!(
            session.lock().history().len(),
            1,
            "only the question survives a cancelled turn"
        );
    }

    #[tokio::test]
    async fn a_turn_interrupted_by_the_board_is_reported_as_interrupted() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();
        // The board moved while the answer was in flight.
        cancel.cancel_with(CancelReason::Stale);

        let recorder = drive_turn(&session, turn_id, cancel).await;

        assert!(
            matches!(
                recorder.events().last(),
                Some(CoachEvent::Done(TurnSummary {
                    reason: optcg_coach::FinishReason::Interrupted,
                    ..
                }))
            ),
            "a board change must be distinguishable from the user pressing Stop: {:?}",
            recorder.events()
        );
    }

    #[tokio::test]
    async fn a_superseded_turn_emits_nothing() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (first_turn, first_cancel) = session.lock().begin_turn("q1").unwrap();

        // The user asks again before the first answer lands.
        session.lock().begin_turn("q2").unwrap();

        let recorder = drive_turn(&session, first_turn, first_cancel).await;

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

    #[tokio::test]
    async fn a_turn_that_cannot_be_grounded_fails_cleanly() {
        let session = Arc::new(Mutex::new(CoachSession::new()));
        let (turn_id, cancel) = session.lock().begin_turn("what now?").unwrap();
        let (emit, recorder) = recording_sink();
        let coalescing = Arc::new(CoalescingSink::new(emit));
        let sink = coalescing.as_event_sink();

        run_turn(
            |_sink| async { None },
            Arc::new(OfflineProvider::instant()),
            Arc::clone(&session),
            coalescing,
            sink,
            turn_id,
            cancel,
        )
        .await;

        assert!(
            matches!(
                recorder.events().last(),
                Some(CoachEvent::Done(TurnSummary {
                    reason: optcg_coach::FinishReason::Failed,
                    ..
                }))
            ),
            "got {:?}",
            recorder.events()
        );
        assert!(
            !session.lock().is_busy(),
            "the turn must not stay in flight"
        );
    }
}
