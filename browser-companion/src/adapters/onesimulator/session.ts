import { SELECTORS } from "./selectors.js";

export type GameSessionPhase =
  | "lobby"
  | "loading"
  | "active"
  | "ended";

export interface GameSessionState {
  phase: GameSessionPhase;
  sessionKey: string | null;
  turn: number | null;
}

let lastSessionKey: string | null = null;

function deriveSessionKey(doc: Document): string | null {
  const board = doc.querySelector(SELECTORS.gameBoard);
  if (!board) return null;
  const turnM = board.textContent?.match(/Turn\s+(\d+)/i);
  const turn = turnM ? turnM[1] : "0";
  const cards = doc.querySelectorAll("[data-card-iid]");
  const first = cards[0]?.getAttribute("data-card-iid") ?? "none";
  return `turn-${turn}-seed-${first}`;
}

export function detectGameSession(doc: Document = document): GameSessionState {
  const board = doc.querySelector(SELECTORS.gameBoard);
  if (!board) {
    return { phase: "lobby", sessionKey: null, turn: null };
  }

  const text = board.textContent ?? "";
  if (/game over|winner|defeat|victory/i.test(text)) {
    return {
      phase: "ended",
      sessionKey: deriveSessionKey(doc),
      turn: parseTurn(text),
    };
  }

  const anchors = doc.querySelectorAll(SELECTORS.zoneAnchor);
  if (anchors.length === 0) {
    return { phase: "loading", sessionKey: null, turn: null };
  }

  const sessionKey = deriveSessionKey(doc);
  return {
    phase: "active",
    sessionKey,
    turn: parseTurn(text),
  };
}

function parseTurn(text: string): number | null {
  const m = text.match(/Turn\s+(\d+)/i);
  return m ? Number(m[1]) : null;
}

export function isNewMatch(session: GameSessionState): boolean {
  if (!session.sessionKey || session.phase !== "active") return false;
  if (lastSessionKey === null) {
    lastSessionKey = session.sessionKey;
    return true;
  }
  if (lastSessionKey !== session.sessionKey) {
    lastSessionKey = session.sessionKey;
    return true;
  }
  return false;
}

export function resetSessionTracker(): void {
  lastSessionKey = null;
}

export function onMatchEnded(session: GameSessionState): boolean {
  return session.phase === "ended";
}
