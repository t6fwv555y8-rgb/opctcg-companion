export type ChatRole = "system" | "user" | "assistant";

export interface ChatMessage {
  role: ChatRole;
  content: string;
}

export interface ToolRun {
  tool: string;
  summary: string;
}

export type FinishReason =
  | "complete"
  /** The user pressed Stop. */
  | "cancelled"
  /** The board moved on, so the answer described a position that is now gone. */
  | "interrupted"
  | "failed";

export interface TurnSummary {
  reason: FinishReason;
  /** Authoritative full text, or null to keep whatever already streamed. */
  text?: string | null;
  error?: string | null;
}

/** The board position an answer is grounded on. */
export interface StateFingerprint {
  /** Short human label, e.g. `turn 4 · Main · life 3-2`. */
  label: string;
  /** Canonical form of the fields that make advice stale when they change. */
  digest: string;
}

/**
 * One frame from the agent's output stream, as emitted on `coach://event`.
 * `turn_id` identifies the question it belongs to; frames from an older turn
 * are dropped rather than mixed into the current answer.
 */
export type CoachStreamEvent =
  | { turn_id: number; type: "state_sync"; data: StateFingerprint }
  | { turn_id: number; type: "status"; data: string }
  | { turn_id: number; type: "tool_run"; data: ToolRun }
  | { turn_id: number; type: "text_delta"; data: string }
  | { turn_id: number; type: "done"; data: TurnSummary };

/** What the coach may send to the model. */
export interface ContextScope {
  /** The live position and everything read off it. */
  board: boolean;
  /** Your saved deck list, leader, and matchup plan. */
  deck: boolean;
}

export interface CoachStatus {
  /** Model name, or `Offline coach` when no API key is configured. */
  provider: string;
  live: boolean;
  busy: boolean;
  active_turn: number | null;
  /** True when the streaming turn was triggered by a board change. */
  automatic: boolean;
  /** True when board changes trigger reads on their own. */
  auto_enabled: boolean;
  /** What the next turn will send. */
  context: ContextScope;
}

export interface CoachHistory {
  messages: ChatMessage[];
  status: CoachStatus;
}

/** A message as rendered in the panel. */
export interface CoachChatMessage {
  role: Exclude<ChatRole, "system">;
  content: string;
  /** True while this assistant message is still streaming. */
  streaming?: boolean;
  /** How the turn ended, once it has. Absent for a clean completion. */
  endedBecause?: Exclude<FinishReason, "complete">;
  /** The board position this answer was grounded on. */
  groundedOn?: StateFingerprint;
  /** True when a board change triggered this read rather than the user. */
  automatic?: boolean;
}

export interface CoachStream {
  messages: CoachChatMessage[];
  status: CoachStatus | null;
  /** Latest progress line for the streaming turn. */
  activity: string | null;
  /** Grounding steps the agent ran for the streaming turn. */
  tools: ToolRun[];
  streaming: boolean;
  error: string | null;
  send: (message: string) => Promise<void>;
  /** Stop the streaming turn, keeping the partial answer on screen. */
  interrupt: () => Promise<void>;
  reset: () => Promise<void>;
  /** Turn unprompted board reads on or off. */
  setAuto: (enabled: boolean) => Promise<void>;
  /** Choose what the coach may send to the model. */
  setContext: (scope: ContextScope) => Promise<void>;
}
