import { REQUIRED_FOR_GAME, SELECTORS } from "./selectors.js";

export interface AdapterDiagnostics {
  site_detected: boolean;
  game_detected: boolean;
  ui_recognized: boolean;
  message: string;
  found: Record<string, boolean>;
}

export function diagnoseGameUi(doc: Document = document): AdapterDiagnostics {
  const found: Record<string, boolean> = {
    game_board: !!doc.querySelector(SELECTORS.gameBoard),
    zone_anchors: doc.querySelectorAll(SELECTORS.zoneAnchor).length > 0,
    cards: doc.querySelectorAll(SELECTORS.card).length > 0,
    life_zone: doc.querySelector('[data-zone-anchor$=":life"]') !== null,
    hand_zone: doc.querySelector('[data-zone-anchor$=":hand"]') !== null,
  };

  const uiRecognized = REQUIRED_FOR_GAME.every((sel) => !!doc.querySelector(sel));
  const gameDetected = found.game_board && found.zone_anchors;

  let message = "Disconnected";
  if (gameDetected && uiRecognized) {
    message = "Game detected";
  } else if (gameDetected && !uiRecognized) {
    message = "OneSimulator detected — Game UI not recognized — Adapter compatibility may require update";
  } else if (found.game_board) {
    message = "OneSimulator loaded — waiting for match";
  }

  return {
    site_detected: true,
    game_detected: gameDetected,
    ui_recognized: uiRecognized,
    message,
    found,
  };
}
