export type ChatRole = "system" | "user" | "assistant";

export interface ChatMessage {
  role: ChatRole;
  content: string;
}

export interface ToolRun {
  tool: string;
  summary: string;
}

export type FinishReason = "complete" | "cancelled" | "failed";

export interface TurnSummary {
  reason: FinishReason;
  /** Authoritative full text, or null to keep whatever already streamed. */
  text?: string | null;
  error?: string | null;
}

/**
 * One frame from the agent's output stream, as emitted on `coach-chat-event`.
 * `turn_id` identifies the question it belongs to; frames from an older turn
 * are dropped rather than mixed into the current answer.
 */
export type CoachStreamEvent =
  | { turn_id: number; type: "status"; data: string }
  | { turn_id: number; type: "tool_run"; data: ToolRun }
  | { turn_id: number; type: "text_delta"; data: string }
  | { turn_id: number; type: "done"; data: TurnSummary };

export interface CoachStatus {
  /** Model name, or `Offline coach` when no API key is configured. */
  provider: string;
  live: boolean;
  busy: boolean;
  active_turn: number | null;
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
}

export interface CoachChat {
  messages: CoachChatMessage[];
  status: CoachStatus | null;
  /** Latest progress line for the streaming turn. */
  activity: string | null;
  /** Grounding steps the agent ran for the streaming turn. */
  tools: ToolRun[];
  streaming: boolean;
  error: string | null;
  send: (message: string) => Promise<void>;
  cancel: () => Promise<void>;
  reset: () => Promise<void>;
}
