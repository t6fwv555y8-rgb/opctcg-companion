import { diagnoseGameUi, type AdapterDiagnostics } from "./diagnostics.js";

export type HealthLevel = "healthy" | "degraded" | "missing";

export interface SelectorHealth {
  leader: HealthLevel;
  life: HealthLevel;
  don: HealthLevel;
  board: HealthLevel;
  combat: HealthLevel;
  hand: HealthLevel;
}

function level(boardOk: boolean, present: boolean): HealthLevel {
  if (!boardOk) return "missing";
  if (present) return "healthy";
  return "degraded";
}

export function evaluateSelectorHealth(doc: Document = document): SelectorHealth {
  const diag = diagnoseGameUi(doc);
  const found = diag.found ?? {};
  const boardOk = !!found.game_board;

  return {
    leader: level(boardOk, !!doc.querySelector('[data-card-zone="leader"]')),
    life: level(boardOk, !!doc.querySelector('[data-zone-anchor$=":life"]')),
    don: level(boardOk, !!doc.querySelector('[data-zone-anchor$=":donField"]')),
    board: level(boardOk, !!doc.querySelector('[data-card-zone="character"]')),
    combat: diag.game_detected ? "healthy" : level(boardOk, false),
    hand: level(boardOk, !!doc.querySelector('[data-zone-anchor$=":hand"]')),
  };
}

export function selectorHealthMessage(health: SelectorHealth): string {
  const degraded = Object.entries(health).filter(([, v]) => v === "degraded");
  const missing = Object.entries(health).filter(([, v]) => v === "missing");
  if (missing.length > 0) {
    return `Missing selectors: ${missing.map(([k]) => k).join(", ")}`;
  }
  if (degraded.length > 0) {
    return `Degraded selectors: ${degraded.map(([k]) => k).join(", ")}`;
  }
  return "All selectors healthy";
}

export function diagnosticsWithHealth(doc: Document = document): AdapterDiagnostics {
  const base = diagnoseGameUi(doc);
  const health = evaluateSelectorHealth(doc);
  const allHealthy = Object.values(health).every((v) => v === "healthy");
  return {
    ...base,
    message: selectorHealthMessage(health),
    found: {
      ...base.found,
      selector_health: allHealthy,
    },
  };
}
