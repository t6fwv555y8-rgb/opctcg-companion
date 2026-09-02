export type Phase = "Draw" | "Don" | "Main" | "End" | "Combat";

export type ConnectionStatusKind =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error";

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

export interface StateUpdatePayload {
  game_state: GameStateDto;
  connection: ConnectionStatusDto;
  combat_analysis: CombatAnalysis | null;
  strategy: StrategyRecommendation | null;
  latency_ms: number;
}

export interface OverlaySettings {
  click_through: boolean;
  opacity: number;
}

export interface CompanionBridge {
  snapshot: StateUpdatePayload | null;
  loading: boolean;
  error: string | null;
  overlay: OverlaySettings;
  toggleOverlay: (enabled?: boolean) => Promise<void>;
  setOpacity: (opacity: number) => Promise<void>;
}
