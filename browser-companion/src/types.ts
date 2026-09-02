/** Normalized browser-visible game snapshot — never includes hidden info. */
export interface ObservedCard {
  card_id?: string | null;
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

export interface BrowserGameSnapshot {
  timestamp: number;
  turn?: number | null;
  phase?: string | null;
  active_player?: string | null;
  self?: BrowserPlayerSnapshot | null;
  opponent?: BrowserPlayerSnapshot | null;
  combat?: BrowserCombatSnapshot | null;
}

export interface ObservableGameSnapshot {
  detected: boolean;
  snapshot: BrowserGameSnapshot | null;
}
