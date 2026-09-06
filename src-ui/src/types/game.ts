export type Phase = "Draw" | "Don" | "Main" | "End" | "Combat";

export type ConnectionStatusKind =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error";

export type AdapterStatusKind =
  | "unavailable"
  | "detecting"
  | "connected"
  | "observing"
  | "degraded"
  | "disconnected"
  | "error";

export type SourceSelectionKind =
  | "auto"
  | "one_simulator"
  | "optcgsim"
  | "mock"
  | "replay"
  | "screen_vision";

export type SyncStateKind =
  | "synced"
  | "partial"
  | "recovering"
  | "degraded"
  | "desynced";

export type HudOperatingStateKind =
  | "searching"
  | "connecting"
  | "syncing"
  | "live"
  | "partial"
  | "lost";

export interface AnalysisEligibilityDto {
  eligible: boolean;
  confidence: number;
  reasons: string[];
  mode: string;
  hud_label: string | null;
}

export type SurvivalStatus = "SURVIVES" | "COUNTER_REQUIRED" | "LETHAL";

export interface LastEventInfo {
  sequence: number;
  event_name: string;
  summary: string;
  processed_at: string;
}

export interface PlayerStateDto {
  player_index: number;
  leader_id: string;
  leader_power: number;
  life: number;
  active_don: number;
  rested_don: number;
  hand_count: number;
  deck_count: number;
  trash_count: number;
  board_count: number;
  board?: BoardCardDto[];
  deck_name?: string;
  known_cards?: string[];
}

/// Where a side's deck list came from.
///
/// `observed` is the leader and revealed cards only, `presumed` is a saved list
/// matched to the leader on the table, and `attached` is a list the user
/// supplied for that side.
export type DeckOrigin = "observed" | "presumed" | "attached";

export type Side = "you" | "opponent";

export interface DeckInfoDto {
  name: string;
  leader_id: string;
  leader_name: string;
  leader_color: string;
  known_cards: KnownCardDto[];
  origin: DeckOrigin;
  deck_id?: string | null;
  list_entries?: DeckListEntryDto[];
  list_total_cards?: number;
  list_warnings?: string[];
}

export interface DeckListEntryDto {
  card_id: string;
  name: string;
  quantity: number;
  cost: number;
  card_type: string;
  color: string;
  rush: boolean;
  blocker: boolean;
  counter: number;
}

export interface PastedDeckDto {
  raw: string;
  name: string | null;
  leader_id: string | null;
  entries: DeckListEntryDto[];
  warnings: string[];
  total_cards: number;
}

export interface SavedDeckDto {
  id: string;
  name: string;
  raw: string;
  leader_id: string | null;
  leader_name: string | null;
  leader_color: string | null;
  total_cards: number;
  is_active: boolean;
  updated_at: string;
}

/// What past games say about the opponent's deck.
export interface ScoutingReportDto {
  leader_id: string;
  leader_name: string;
  games: number;
  /// How far the sample goes: "thin", "fair", or "solid".
  reliability: string;
  pace: string;
  /// Copies the map can name, against the fifty in a deck.
  mapped_copies: number;
  cards: ScoutedCardDto[];
  notes: string[];
}

export interface ScoutedCardDto {
  card_id: string;
  name: string;
  games_seen: number;
  /// Share of games this card appeared in, between 0 and 1.
  confidence: number;
  likely_copies: number;
  earliest_turn: number;
}

export interface DeckCollectionDto {
  decks: SavedDeckDto[];
  active_id: string | null;
  opponent_id?: string | null;
  max_decks: number;
}

export interface KnownCardDto {
  card_id: string;
  name: string;
  card_type: string;
  color: string;
}

export interface BoardCardDto {
  card_id: string;
  rested: boolean;
  attached_don: number;
  power: number;
  position: number;
}

export interface CombatState {
  active: boolean;
  attacker_id: string | null;
  target_id: string | null;
  target_is_leader: boolean;
  blocker_offered: boolean;
  blocker_id: string | null;
  damage: number;
  blocked: boolean;
}

export interface GameStateDto {
  game_id: string;
  turn_number: number;
  active_player: number;
  phase: Phase;
  player_one: PlayerStateDto;
  player_two: PlayerStateDto;
  combat: CombatState;
  event_sequence: number;
  last_event: LastEventInfo | null;
  timestamp: string;
}

export interface ConnectionStatusDto {
  status: ConnectionStatusKind;
  label: string;
  websocket_connected: boolean;
  file_monitor_active: boolean;
  latency_ms: number;
  events_processed: number;
  event_sequence: number;
  last_error: string | null;
}

export interface LatencySnapshot {
  observation_latency_ms: number;
  analysis_latency_ms: number;
  total_latency_ms: number;
  last_updated: string | null;
}

export interface AdapterInfoDto {
  source: string;
  status: AdapterStatusKind;
  detected: boolean;
  label: string;
  live: boolean;
}

export interface ObservationStatusDto {
  selection: SourceSelectionKind;
  active_source: string | null;
  adapters: AdapterInfoDto[];
  latency: LatencySnapshot;
  searching: boolean;
  sync_state: SyncStateKind;
  hud_state: HudOperatingStateKind;
  analysis: AnalysisEligibilityDto;
}

export interface CombatCalculation {
  attacker_power: number;
  defender_power: number;
  power_delta: number;
  available_counter: number;
  required_counter: number;
  survives: boolean;
}

export interface CombatAnalysis {
  calculation: CombatCalculation;
  survival_status: SurvivalStatus;
  attacker_power: number;
  defender_power: number;
  power_differential: number;
  required_counter: number;
  survives_without_counter: boolean;
  survives_with_base_counter: boolean;
  lethal_to_leader: boolean;
  blocker_available: boolean;
  recommended_block: boolean;
  shield_needed: number;
}

export interface StrategyRecommendation {
  action: {
    action_type: string;
    actor: number;
    card_id: string | null;
    target: string | null;
    description: string;
  };
  score: number;
  confidence: number;
  reasoning: string;
}

export interface DeckStrategyBrief {
  matchup: string;
  your_plan: string;
  vs_opponent: string;
  this_turn: string[];
  threats: string[];
  priorities: string[];
  list_notes?: string[];
  refreshed_at: string;
}

export interface StateUpdatePayload {
  game_state: GameStateDto;
  connection: ConnectionStatusDto;
  combat_analysis: CombatAnalysis | null;
  strategy: StrategyRecommendation | null;
  options?: StrategyRecommendation[];
  phase_coach?: string;
  deck_strategy?: DeckStrategyBrief | null;
  your_deck?: DeckInfoDto;
  opponent_deck?: DeckInfoDto;
  pasted_deck?: PastedDeckDto | null;
  deck_collection?: DeckCollectionDto;
  scouting?: ScoutingReportDto | null;
  latency_ms: number;
  observation: ObservationStatusDto | null;
}

export interface OverlaySettings {
  click_through: boolean;
  opacity: number;
}

export interface CompanionBridge {
  snapshot: StateUpdatePayload | null;
  observation: ObservationStatusDto | null;
  loading: boolean;
  error: string | null;
  overlay: OverlaySettings;
  refreshingStrategy: boolean;
  toggleOverlay: (enabled?: boolean) => Promise<void>;
  setOpacity: (opacity: number) => Promise<void>;
  setObservationSource: (selection: SourceSelectionKind) => Promise<void>;
  refreshDeckStrategy: () => Promise<void>;
  setPastedDeck: (raw: string) => Promise<void>;
  clearPastedDeck: () => Promise<void>;
  saveDeck: (args: {
    raw: string;
    name?: string;
    id?: string;
    side?: Side;
  }) => Promise<void>;
  setDeckSource: (side: Side, deckId: string | null) => Promise<void>;
  activateDeck: (id: string) => Promise<void>;
  deleteDeck: (id: string) => Promise<void>;
  renameDeck: (id: string, name: string) => Promise<void>;
}
