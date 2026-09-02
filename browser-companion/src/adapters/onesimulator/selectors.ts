/**
 * Centralized OneSimulator DOM selectors — patch here when the site updates.
 * Derived from runtime inspection of GameBoard-DWdEQgnY.js (2026-03).
 *
 * Priority: data-* attributes > aria > semantic structure > stable classes
 */
export const ONE_SIMULATOR_HOST = "onesimulator.slidingcodes.com";

export const SELECTORS = {
  /** Root game board container */
  gameBoard: ".game-board-shell",
  /** Phase label row: "Phase: <value>" */
  phaseLabel: "text-yellow-500, .text-yellow-500",
  phaseRow: '[class*="text-yellow-500"]',
  /** Turn indicator: "Turn N" */
  turnText: 'span.text-yellow-500, span[class*="text-yellow-500"]',
  /** Card elements with stable data attributes */
  card: "[data-card-zone][data-card-player-id]",
  cardZone: "[data-card-zone]",
  cardPlayer: "[data-card-player-id]",
  cardSlot: "[data-card-slot]",
  cardIid: "[data-card-iid]",
  /** Zone anchors: `${playerId}:${zone}` e.g. "0:life", "1:hand" */
  zoneAnchor: "[data-zone-anchor]",
  donIid: "[data-don-iid]",
} as const;

/** Known zone names from OneSimulator runtime */
export const ZONES = {
  leader: "leader",
  character: "character",
  stage: "stage",
  hand: "hand",
  life: "life",
  donField: "donField",
  donDeck: "donDeck",
  deck: "deck",
  trash: "trash",
  leaderDon: "leaderDon",
  characterDon: "characterDon",
} as const;

/** Minimum selectors that must match for "game UI recognized" */
export const REQUIRED_FOR_GAME = [
  SELECTORS.gameBoard,
  SELECTORS.zoneAnchor,
] as const;

/** Card ID pattern: OP01-001, ST01-001, EB01-001, P-001, DON, etc. */
export const CARD_ID_RE =
  /\b((?:OP|ST|EB|P|PRB|DP|UC|CP|L|SEC|SR|R|UC|C|P)-?\d{2,3}-?\d{3}[a-zA-Z]?|DON-?\d*)\b/i;

/** Extract card id from image src like /cards/full/OP01-001.webp */
export const CARD_SRC_RE = /\/cards\/(?:full|thumbnail)\/([^/.]+)\.webp/i;
