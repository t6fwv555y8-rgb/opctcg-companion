/** Normalized browser-visible game snapshot — never includes hidden info. */
export interface ObservedCard {
  card_id?: string | null;
  /** Stable observation instance key (player:zone:slot:cardId:iid) */
  instance_key?: string | null;
  name?: string | null;
  power?: number | null;
  rested?: boolean | null;
}

export interface BrowserPlayerSnapshot {
  life?: number | null;
  hand_count?: number | null;
  active_don?: number | null;
  rested_don?: number | null;
  leader_id?: string | null;
  deck_name?: string | null;
  known_cards?: string[];
  /** Your own hand, in on-screen order, duplicates kept. Never set for the opponent. */
  hand?: string[];
  board?: ObservedCard[];
}

export interface BrowserCombatSnapshot {
  attacker?: ObservedCard | null;
  target?: ObservedCard | null;
  displayed_power?: number | null;
  /** "self" or "opponent": whose card is attacking / being attacked. */
  attacker_owner?: "self" | "opponent" | null;
  target_owner?: "self" | "opponent" | null;
  target_is_leader?: boolean | null;
}

export interface AdapterDiagnostics {
  site_detected?: boolean;
  game_detected?: boolean;
  ui_recognized?: boolean;
  message?: string;
  found?: Record<string, boolean>;
}

export interface BrowserGameSnapshot {
  timestamp: number;
  source?: string | null;
  sequence?: number | null;
  turn?: number | null;
  phase?: string | null;
  /** "self" or "opponent" when the page says whose turn it is, else null. */
  active_player?: "self" | "opponent" | null;
  self?: BrowserPlayerSnapshot | null;
  opponent?: BrowserPlayerSnapshot | null;
  combat?: BrowserCombatSnapshot | null;
  /** True while the page shows the game-over state. */
  game_over?: boolean | null;
  /** How it ended for you, when the page says so; null when it does not. */
  result?: "win" | "loss" | null;
  diagnostics?: AdapterDiagnostics | null;
}

export interface ObservableGameSnapshot {
  detected: boolean;
  snapshot: BrowserGameSnapshot | null;
  diagnostics?: AdapterDiagnostics | null;
}

export interface BridgeStatus {
  connected: boolean;
  game_detected: boolean;
  message: string;
}

/** Site-specific adapter interface — one small module per supported online simulator. */
export interface SimulatorSiteAdapter {
  id: string;
  matches(location: Location): boolean;
  detectGame(): boolean;
  observe(): ObservableGameSnapshot;
}
