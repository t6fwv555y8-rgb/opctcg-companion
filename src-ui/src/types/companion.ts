export type Phase = "Draw" | "Don" | "Main" | "End" | "Combat";

export interface CardInstance {
  instance_id: string;
  card_id: string;
  owner: number;
  zone: string;
  state: string;
  attached_don: number;
}

export interface PlayerState {
  player_index: number;
  life: number;
  don_active: number;
  don_rested: number;
  hand_count: number;
  deck_count: number;
  leader_id: string;
  leader_power: number;
  characters: CardInstance[];
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

export interface ConnectionState {
  websocket_connected: boolean;
  file_monitor_active: boolean;
  latency_ms: number;
  events_processed: number;
}

export interface GameState {
  active_player: number;
  turn_number: number;
  phase: Phase;
  players: [PlayerState, PlayerState];
  combat: CombatState;
  connection: ConnectionState;
}

export interface LegalAction {
  action_type: string;
  card_id: string | null;
  target_id: string | null;
  description: string;
  priority: number;
}

export interface ScoredAction {
  action: LegalAction;
  score: number;
  sequence: string[];
}

export interface MctsResult {
  best_action: LegalAction;
  win_rate: number;
  visits: number;
  alternatives: [LegalAction, number][];
}

export interface RecommendationsPayload {
  beam: ScoredAction[];
  mcts: MctsResult | null;
}

export interface CombatAnalysis {
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

export interface ConnectionStatus {
  websocket_connected: boolean;
  file_monitor_active: boolean;
  latency_ms: number;
  events_processed: number;
}

export interface CompanionBridge {
  gameState: GameState | null;
  recommendations: RecommendationsPayload | null;
  combatAnalysis: CombatAnalysis | null;
  connectionStatus: ConnectionStatus | null;
  legalActions: LegalAction[];
  loading: boolean;
  error: string | null;
  clickThrough: boolean;
  setClickThrough: (enabled: boolean) => void;
}
