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
  board?: ObservedCard[];
}

export interface BrowserCombatSnapshot {
  attacker?: ObservedCard | null;
  target?: ObservedCard | null;
  displayed_power?: number | null;
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
  active_player?: string | null;
  self?: BrowserPlayerSnapshot | null;
  opponent?: BrowserPlayerSnapshot | null;
  combat?: BrowserCombatSnapshot | null;
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
