use optcg_core::{CombatState, ConnectionStatus, GameState, LastEventInfo, Phase, PlayerState};
use optcg_observation::{AdapterInfo, AdapterStatus, LatencySnapshot};
use optcg_rules::{
    CombatAnalysis, CombatDoThis, DeckListEntry, DeckStrategyBrief, PastedDeckList,
    StrategyRecommendation,
};
use serde::{Deserialize, Serialize};

/// Serializable game state snapshot for the frontend (authoritative DTO).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateDto {
    pub game_id: String,
    pub turn_number: u32,
    pub active_player: u8,
    pub phase: Phase,
    pub player_one: PlayerStateDto,
    pub player_two: PlayerStateDto,
    pub combat: CombatState,
    pub event_sequence: u64,
    pub last_event: Option<LastEventInfo>,
    pub timestamp: String,
    #[serde(default)]
    pub page_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownCardDto {
    pub card_id: String,
    pub name: String,
    pub card_type: String,
    pub color: String,
}

/// Where a side's deck list came from, which decides how much the coach may
/// take for granted about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckOrigin {
    /// Read from play: the leader plus whatever cards the table has revealed.
    /// No full list is known.
    Observed,
    /// Read from play, then matched to a saved list by leader. The list is a
    /// good guess about a deck nobody has confirmed.
    Presumed,
    /// A list attached to this side by hand. Trustworthy for your own side.
    Attached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckInfoDto {
    /// Simulator deck label when visible; otherwise leader-based fallback.
    pub name: String,
    pub leader_id: String,
    pub leader_name: String,
    pub leader_color: String,
    pub known_cards: Vec<KnownCardDto>,
    /// How this side's deck was arrived at.
    pub origin: DeckOrigin,
    /// The saved list backing this side, when one does.
    #[serde(default)]
    pub deck_id: Option<String>,
    /// Exact list entries, present when `origin` carries a list.
    #[serde(default)]
    pub list_entries: Vec<DeckListEntry>,
    #[serde(default)]
    pub list_total_cards: u32,
    #[serde(default)]
    pub list_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedDeckDto {
    pub raw: String,
    pub name: Option<String>,
    pub leader_id: Option<String>,
    pub entries: Vec<DeckListEntry>,
    pub warnings: Vec<String>,
    pub total_cards: u32,
}

/// One deck in the user's saved collection, with its leader resolved for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDeckDto {
    pub id: String,
    pub name: String,
    /// Original paste, so the UI can reopen a saved deck for editing.
    pub raw: String,
    pub leader_id: Option<String>,
    pub leader_name: Option<String>,
    pub leader_color: Option<String>,
    pub total_cards: u32,
    pub is_active: bool,
    pub updated_at: String,
}

/// What past games say about the opponent's deck, for the HUD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutingReportDto {
    pub leader_id: String,
    pub leader_name: String,
    pub games: u32,
    /// How far the sample goes: `thin`, `fair`, or `solid`.
    pub reliability: String,
    pub pace: String,
    /// Copies the map can name, against the fifty in a deck.
    pub mapped_copies: u32,
    pub cards: Vec<ScoutedCardDto>,
    pub notes: Vec<String>,
}

/// How your deck has actually fared against this leader, for the HUD.
///
/// Finished games only. A game in progress has no result to report, and the
/// current one is exactly the game the player is trying to win.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchupReportDto {
    pub their_leader_id: String,
    pub their_leader_name: String,
    pub wins: u32,
    pub losses: u32,
    /// Games that ended without a readable result.
    pub unfinished: u32,
    /// `too_early`, `favourable`, `even`, or `rough`.
    pub standing: String,
    /// Share of finished games won, absent until a game has finished.
    pub win_rate: Option<f32>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutedCardDto {
    pub card_id: String,
    pub name: String,
    pub games_seen: u32,
    /// Share of games this card appeared in, between 0 and 1.
    pub confidence: f32,
    pub likely_copies: u32,
    pub earliest_turn: u32,
}

/// The saved deck library shipped to the HUD on every state update.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeckCollectionDto {
    pub decks: Vec<SavedDeckDto>,
    /// The list attached to your side, or `None` when it is read from play.
    pub active_id: Option<String>,
    /// The list attached to the opponent, or `None` when read from play.
    #[serde(default)]
    pub opponent_id: Option<String>,
    pub max_decks: usize,
}

impl From<&PastedDeckList> for PastedDeckDto {
    fn from(p: &PastedDeckList) -> Self {
        Self {
            raw: p.raw.clone(),
            name: p.name.clone(),
            leader_id: p.leader_id.clone(),
            entries: p.entries.clone(),
            warnings: p.warnings.clone(),
            total_cards: p.total_cards,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStateDto {
    pub player_index: u8,
    pub leader_id: String,
    pub leader_power: u32,
    pub life: u32,
    pub active_don: u32,
    pub rested_don: u32,
    pub hand_count: u32,
    pub deck_count: u32,
    pub trash_count: u32,
    pub board_count: u32,
    pub board: Vec<BoardCardDto>,
    #[serde(default)]
    pub deck_name: String,
    #[serde(default)]
    pub player_name: String,
    #[serde(default)]
    pub known_cards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardCardDto {
    pub card_id: String,
    pub rested: bool,
    pub attached_don: u32,
    pub power: u32,
    pub position: u8,
}

impl From<&PlayerState> for PlayerStateDto {
    fn from(p: &PlayerState) -> Self {
        Self {
            player_index: p.player_index,
            leader_id: p.leader.card_id.clone(),
            leader_power: p.leader.effective_power(),
            life: p.life,
            active_don: p.don_active,
            rested_don: p.don_rested,
            hand_count: p.hand_count,
            deck_count: p.deck_count,
            trash_count: p.trash_count,
            board_count: p.characters.len() as u32,
            board: p
                .characters
                .iter()
                .map(|c| BoardCardDto {
                    card_id: c.card_id.clone(),
                    rested: c.rested,
                    attached_don: c.attached_don,
                    power: c.effective_power(5000).max(0) as u32,
                    position: c.position,
                })
                .collect(),
            deck_name: p.deck_name.clone(),
            player_name: p.player_name.clone(),
            known_cards: p.known_cards.clone(),
        }
    }
}

impl From<&GameState> for GameStateDto {
    fn from(state: &GameState) -> Self {
        Self {
            game_id: state.game_id.to_string(),
            turn_number: state.turn_number,
            active_player: state.active_player,
            phase: state.phase,
            player_one: PlayerStateDto::from(&state.players[0]),
            player_two: PlayerStateDto::from(&state.players[1]),
            combat: state.combat.clone(),
            event_sequence: state.event_sequence,
            last_event: state.last_event.clone(),
            timestamp: state.timestamp.to_rfc3339(),
            page_state: state.page_state.clone(),
        }
    }
}

/// Connection status DTO for HUD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusDto {
    pub status: ConnectionStatus,
    pub label: String,
    pub websocket_connected: bool,
    pub file_monitor_active: bool,
    pub latency_ms: u64,
    pub events_processed: u64,
    pub event_sequence: u64,
    pub last_error: Option<String>,
}

impl ConnectionStatusDto {
    pub fn from_state(state: &GameState) -> Self {
        let status = state.connection.status;
        let label = match status {
            ConnectionStatus::Connected => "SIMULATOR CONNECTED",
            ConnectionStatus::Connecting => "CONNECTING...",
            ConnectionStatus::Disconnected => "WAITING FOR SIMULATOR",
            ConnectionStatus::Error => "CONNECTION ERROR",
        }
        .to_string();

        Self {
            status,
            label,
            websocket_connected: state.connection.websocket_connected,
            file_monitor_active: state.connection.file_monitor_active,
            latency_ms: state.connection.latency_ms,
            events_processed: state.connection.events_processed,
            event_sequence: state.event_sequence,
            last_error: state.connection.last_error.clone(),
        }
    }
}

/// Payload emitted to frontend on every meaningful state change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateUpdatePayload {
    pub game_state: GameStateDto,
    pub connection: ConnectionStatusDto,
    pub combat_analysis: Option<CombatAnalysis>,
    /// What to do in the open battle, when one is happening.
    #[serde(default)]
    pub combat_coach: Option<CombatDoThis>,
    pub strategy: Option<StrategyRecommendation>,
    /// Ranked options for the current step (best first).
    pub options: Vec<StrategyRecommendation>,
    /// Phase coaching line ("what to do now").
    pub phase_coach: String,
    /// Detailed deck-vs-deck strategy brief (refreshable).
    pub deck_strategy: Option<DeckStrategyBrief>,
    /// Deck identity for you (player 1 / self).
    pub your_deck: DeckInfoDto,
    /// Deck identity for opponent (player 2).
    pub opponent_deck: DeckInfoDto,
    /// User-pasted exact deck list (if any).
    #[serde(default)]
    pub pasted_deck: Option<PastedDeckDto>,
    /// Saved deck library and which deck is active.
    #[serde(default)]
    pub deck_collection: DeckCollectionDto,
    /// What earlier games say about the opponent's deck, when there were any.
    #[serde(default)]
    pub scouting: Option<ScoutingReportDto>,
    /// How your deck has gone against this leader before, when it has.
    #[serde(default)]
    pub matchup: Option<MatchupReportDto>,
    pub latency_ms: u64,
    pub observation: Option<ObservationStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSelectionDto {
    Auto,
    #[serde(alias = "browser_simulator")]
    OneSimulator,
    #[serde(alias = "desktop_simulator")]
    OptcgSim,
    Mock,
    Replay,
    ScreenVision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStateDto {
    Synced,
    Partial,
    Recovering,
    Degraded,
    Desynced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudOperatingState {
    Searching,
    Connecting,
    Syncing,
    Live,
    Partial,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisEligibilityDto {
    pub eligible: bool,
    pub confidence: f32,
    pub reasons: Vec<String>,
    pub mode: String,
    pub hud_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterValidationDto {
    pub adapter: String,
    pub implementation: String,
    pub fixture_tests: String,
    pub live_validation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugStatusDto {
    pub observation_sequence: u64,
    pub event_sequence: u64,
    pub sync_status: SyncStateDto,
    pub capture_stats: Option<optcg_observation::CaptureStats>,
    pub validation: Vec<AdapterValidationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfoDto {
    pub source: String,
    pub status: AdapterStatus,
    pub detected: bool,
    pub label: String,
    pub live: bool,
}

impl From<AdapterInfo> for AdapterInfoDto {
    fn from(info: AdapterInfo) -> Self {
        Self {
            source: info.source.label().to_lowercase().replace(' ', "_"),
            status: info.status,
            detected: info.detected,
            label: info.label,
            live: info.status.is_live(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationStatusDto {
    pub selection: SourceSelectionDto,
    pub active_source: Option<String>,
    pub adapters: Vec<AdapterInfoDto>,
    pub latency: LatencySnapshot,
    pub searching: bool,
    pub sync_state: SyncStateDto,
    pub hud_state: HudOperatingState,
    pub analysis: AnalysisEligibilityDto,
}

/// Source-aware connection label for HUD.
impl ConnectionStatusDto {
    pub fn with_source_label(mut self, source: Option<&str>) -> Self {
        if let Some(src) = source {
            self.label = format!("{src} · LIVE");
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    pub click_through: bool,
    pub opacity: f64,
}
