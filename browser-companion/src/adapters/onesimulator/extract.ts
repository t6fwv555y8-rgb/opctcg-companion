import type {
  BrowserCombatSnapshot,
  BrowserGameSnapshot,
  BrowserPlayerSnapshot,
  ObservedCard,
} from "../../types.js";
import { diagnoseGameUi } from "./diagnostics.js";
import {
  buildInstanceKey,
  toObservedCard,
  type CardInstanceKey,
} from "./instances.js";
import {
  CARD_ID_RE,
  CARD_SRC_RE,
  SELECTORS,
  ZONES,
} from "./selectors.js";

/** Normalize OneSimulator phase strings to canonical labels */
export function normalizePhase(raw: string | null): string | null {
  if (!raw) return null;
  const lower = raw.trim().toLowerCase();
  if (lower.includes("draw") || lower === "refresh") return "Draw";
  if (lower.includes("don")) return "Don";
  if (lower.includes("main") || lower === "battle-target") return "Main";
  if (lower.includes("end")) return "End";
  if (lower.includes("combat") || lower.includes("battle") || lower.includes("counter") || lower.includes("block")) return "Combat";
  if (lower.includes("idle") || lower.includes("game-over")) return null;
  return raw.trim();
}

function extractCardId(el: Element): string | null {
  const img = el.querySelector("img");
  if (img?.src) {
    const m = img.src.match(CARD_SRC_RE);
    if (m) return normalizeCardId(m[1]);
  }
  const alt = img?.alt?.trim();
  if (alt && CARD_ID_RE.test(alt)) {
    const m = alt.match(CARD_ID_RE);
    if (m) return normalizeCardId(m[1]);
  }
  const text = el.textContent ?? "";
  const m = text.match(CARD_ID_RE);
  return m ? normalizeCardId(m[1]) : null;
}

export function normalizeCardId(raw: string): string {
  let id = raw.replace(/\.webp$/i, "").replace(/_/g, "-").toUpperCase();
  const m = id.match(CARD_ID_RE);
  if (m) return m[1].toUpperCase();
  return id;
}

function isRested(el: Element): boolean | null {
  if (el.classList?.contains("rotate-90")) return true;
  const img = el.querySelector("img");
  if (!img) return null;
  const transform = getComputedStyle(img).transform;
  if (transform && transform !== "none") {
    // OneSimulator rotates rested cards ~90deg
    if (transform.includes("matrix") || transform.includes("rotate(90")) return true;
  }
  const cls = `${el.className?.toString() ?? ""} ${el.getAttribute("class") ?? ""}`;
  if (cls.includes("rotate-90")) return true;
  return false;
}

function resolveSelfPlayerId(doc: Document): string | null {
  const handAnchors = doc.querySelectorAll(`[data-zone-anchor$=":${ZONES.hand}"]`);
  let selfId: string | null = null;
  let maxTop = -1;
  for (const anchor of handAnchors) {
    const pid = anchor.getAttribute("data-zone-anchor")?.split(":")[0];
    if (!pid) continue;
    const parent = anchor.closest('[class*="items-end"], [class*="items-start"]');
    const isSelf =
      parent?.className?.toString().includes("items-end") ?? false;
    const top = anchor.getBoundingClientRect().top;
    if (isSelf || top > maxTop) {
      maxTop = top;
      selfId = pid;
    }
  }
  return selfId;
}

function countLife(playerId: string, doc: Document): number | null {
  const anchor = doc.querySelector(
    `[data-zone-anchor="${playerId}:${ZONES.life}"]`,
  );
  if (!anchor) return null;
  const cards = anchor.querySelectorAll(
    `[data-card-zone="${ZONES.life}"][data-card-player-id="${playerId}"]`,
  );
  if (cards.length > 0) return cards.length;
  const countEl = anchor.querySelector("span.text-yellow-300, span[class*='text-yellow']");
  const text = countEl?.textContent?.trim();
  if (text && /^\d+$/.test(text)) return Number(text);
  const lifeLabel = anchor.textContent?.match(/Life:\s*(\d+)/i);
  if (lifeLabel) return Number(lifeLabel[1]);
  return null;
}

function countHand(playerId: string, doc: Document): number | null {
  const cards = doc.querySelectorAll(
    `[data-card-zone="${ZONES.hand}"][data-card-player-id="${playerId}"]`,
  );
  if (cards.length > 0) return cards.length;
  const anchor = doc.querySelector(
    `[data-zone-anchor="${playerId}:${ZONES.hand}"]`,
  );
  const m = anchor?.textContent?.match(/Hand:\s*(\d+)/i);
  return m ? Number(m[1]) : null;
}

function countDon(playerId: string, doc: Document): { active: number; rested: number } | null {
  const field = doc.querySelector(
    `[data-zone-anchor="${playerId}:${ZONES.donField}"]`,
  );
  if (!field) return null;
  const donCards = field.querySelectorAll("[data-don-iid], [data-card-zone='donField']");
  let active = 0;
  let rested = 0;
  for (const d of donCards) {
    if (isRested(d)) rested += 1;
    else active += 1;
  }
  if (active + rested === 0) return null;
  return { active, rested };
}

function extractBoardCards(playerId: string, doc: Document): ObservedCard[] {
  const boardZones = [ZONES.leader, ZONES.character, ZONES.stage];
  const cards: ObservedCard[] = [];
  for (const zone of boardZones) {
    const els = doc.querySelectorAll(
      `[data-card-zone="${zone}"][data-card-player-id="${playerId}"]`,
    );
    for (const el of els) {
      const slot = el.getAttribute("data-card-slot") ?? "0";
      const iid = el.getAttribute("data-card-iid");
      const key: CardInstanceKey = {
        playerId,
        zone,
        slot,
        cardId: extractCardId(el),
        domIid: iid,
      };
      cards.push(toObservedCard(key, isRested(el), null));
    }
  }
  return cards;
}

function extractPhase(doc: Document): string | null {
  const board = doc.querySelector(SELECTORS.gameBoard);
  if (!board) return null;
  const text = board.textContent ?? "";
  const m = text.match(/Phase:\s*([^\n]+)/i);
  if (m) return normalizePhase(m[1].trim());
  return null;
}

function extractTurn(doc: Document): number | null {
  const board = doc.querySelector(SELECTORS.gameBoard);
  if (!board) return null;
  const m = board.textContent?.match(/Turn\s+(\d+)/i);
  return m ? Number(m[1]) : null;
}

function extractCombat(doc: Document): BrowserCombatSnapshot | null {
  const board = doc.querySelector(SELECTORS.gameBoard);
  if (!board) return null;
  const text = board.textContent ?? "";
  if (!/battle|attack|counter|block/i.test(text)) return null;
  const powerM = text.match(/(\d{4,5})\s*(?:power|→|->)/i);
  return {
    attacker: null,
    target: null,
    displayed_power: powerM ? Number(powerM[1]) : null,
  };
}

function buildPlayerSnapshot(
  playerId: string,
  doc: Document,
): BrowserPlayerSnapshot {
  const don = countDon(playerId, doc);
  return {
    life: countLife(playerId, doc),
    hand_count: countHand(playerId, doc),
    active_don: don?.active ?? null,
    rested_don: don?.rested ?? null,
    board: extractBoardCards(playerId, doc),
  };
}

/** Extract normalized player-visible snapshot from OneSimulator DOM */
export function extractOneSimulatorSnapshot(
  doc: Document = document,
): BrowserGameSnapshot {
  const diag = diagnoseGameUi(doc);
  const selfPid = resolveSelfPlayerId(doc);
  const playerIds = new Set<string>();
  doc.querySelectorAll(SELECTORS.cardPlayer).forEach((el) => {
    const pid = el.getAttribute("data-card-player-id");
    if (pid) playerIds.add(pid);
  });
  doc.querySelectorAll(SELECTORS.zoneAnchor).forEach((el) => {
    const anchor = el.getAttribute("data-zone-anchor");
    const pid = anchor?.split(":")[0];
    if (pid) playerIds.add(pid);
  });

  const ids = [...playerIds].sort();
  const selfId = selfPid ?? ids[0] ?? "0";
  const oppId = ids.find((id) => id !== selfId) ?? (selfId === "0" ? "1" : "0");

  return {
    timestamp: Date.now(),
    source: "onesimulator",
    turn: extractTurn(doc),
    phase: extractPhase(doc),
    active_player: null,
    self: buildPlayerSnapshot(selfId, doc),
    opponent: buildPlayerSnapshot(oppId, doc),
    combat: extractCombat(doc),
    diagnostics: diag,
  };
}

export function listCardInstances(doc: Document): CardInstanceKey[] {
  const keys: CardInstanceKey[] = [];
  doc.querySelectorAll(SELECTORS.card).forEach((el) => {
    const zone = el.getAttribute("data-card-zone");
    const playerId = el.getAttribute("data-card-player-id");
    if (!zone || !playerId) return;
    if (zone === ZONES.hand) return; // never expose opponent hand identities
    keys.push({
      playerId,
      zone,
      slot: el.getAttribute("data-card-slot") ?? "0",
      cardId: extractCardId(el),
      domIid: el.getAttribute("data-card-iid"),
    });
  });
  return keys;
}

export { buildInstanceKey };
