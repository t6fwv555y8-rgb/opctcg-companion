import type { BrowserCombatSnapshot, ObservedCard } from "../../types.js";
import { CARD_ID_RE, CARD_SRC_RE, SELECTORS, ZONES } from "./selectors.js";
import { normalizeCardId } from "./extract.js";

export interface CombatObservation {
  attacker: ObservedCard | null;
  attackerPower: number | null;
  target: ObservedCard | null;
  defenderPower: number | null;
  timestamp: number;
  confidence: number;
}

function extractCardFromEl(el: Element | null): ObservedCard | null {
  if (!el) return null;
  const img = el.querySelector("img");
  let cardId: string | null = null;
  if (img?.src) {
    const m = img.src.match(CARD_SRC_RE);
    if (m) cardId = normalizeCardId(m[1]);
  }
  const iid = el.getAttribute("data-card-iid");
  const zone = el.getAttribute("data-card-zone");
  const slot = el.getAttribute("data-card-slot") ?? "0";
  const pid = el.getAttribute("data-card-player-id") ?? "?";
  return {
    card_id: cardId,
    instance_key: `${pid}:${zone}:${slot}:${cardId ?? "?"}:${iid ?? "?"}`,
    rested: null,
    power: null,
    name: img?.alt ?? null,
  };
}

/** Dedicated combat DOM observer — inspects visible battle UI structure. */
export function observeCombat(
  doc: Document = document,
  options: { activePlayerId?: string | null } = {},
): CombatObservation | null {
  const board = doc.querySelector(SELECTORS.gameBoard);
  if (!board) return null;

  // Battle-target phase cards have data-card-zone="character" with selection ring classes
  const selected = board.querySelector(
    '[data-card-zone="character"][class*="ring-"], [data-card-zone="leader"][class*="ring-"], [data-card-zone="character"][class*="border-yellow"], [data-card-zone="leader"][class*="border-yellow"]',
  );

  const attackBanner = board.querySelector(
    '[class*="battle"], [class*="attack"], [data-combat-active="true"]',
  );

  if (!selected && !attackBanner) {
    const text = board.textContent ?? "";
    if (!/battle|attack|counter|block/i.test(text)) return null;
  }

  // Marked cards (selection ring) can be on either side: the attacker belongs
  // to whoever's turn it is and the target to the other player. Never assume
  // the attacker is ours, or the opponent's attacks are read as our own.
  const marked = [
    ...board.querySelectorAll(
      '[data-card-zone="character"][class*="ring-"], [data-card-zone="leader"][class*="ring-"], [data-card-zone="character"][class*="border-yellow"], [data-card-zone="leader"][class*="border-yellow"]',
    ),
  ];
  const owner = (el: Element) => el.getAttribute("data-card-player-id");
  const activePid = options.activePlayerId ?? null;
  let attackerEl: Element | null = null;
  let targetEl: Element | null = null;
  if (activePid) {
    attackerEl = marked.find((el) => owner(el) === activePid) ?? null;
    targetEl = marked.find((el) => owner(el) !== activePid) ?? null;
  } else if (marked.length >= 2) {
    attackerEl = marked[0];
    targetEl = marked.find((el) => owner(el) !== owner(marked[0])) ?? null;
  } else {
    attackerEl = marked[0] ?? null;
  }

  const text = board.textContent ?? "";
  const powerMatches = [...text.matchAll(/(\d{4,5})\s*(?:→|->|power)/gi)];
  const attackerPower = powerMatches[0]
    ? Number(powerMatches[0][1])
    : null;
  const defenderPower = powerMatches[1]
    ? Number(powerMatches[1][1])
    : null;

  const hasSignal = attackerEl || targetEl || attackerPower;
  if (!hasSignal) return null;

  return {
    attacker: extractCardFromEl(attackerEl),
    attackerPower,
    target: extractCardFromEl(targetEl),
    defenderPower,
    timestamp: Date.now(),
    confidence: selected ? 0.92 : 0.75,
  };
}

function ownerFromKey(
  card: ObservedCard | null,
  selfId: string | null,
): "self" | "opponent" | null {
  const pid = card?.instance_key?.split(":")[0];
  if (!pid || pid === "?" || !selfId) return null;
  return pid === selfId ? "self" : "opponent";
}

export function combatToSnapshot(
  combat: CombatObservation,
  selfId: string | null = null,
): BrowserCombatSnapshot {
  const targetZone = combat.target?.instance_key?.split(":")[1];
  return {
    attacker: combat.attacker,
    target: combat.target,
    displayed_power: combat.attackerPower,
    attacker_owner: ownerFromKey(combat.attacker, selfId),
    target_owner: ownerFromKey(combat.target, selfId),
    target_is_leader: combat.target ? targetZone === "leader" : null,
  };
}

/** Selector health for combat-related DOM probes. */
export function combatSelectorHealth(doc: Document = document): Record<string, boolean> {
  const board = doc.querySelector(SELECTORS.gameBoard);
  return {
    game_board: !!board,
    character_cards: !!doc.querySelector(`[data-card-zone="${ZONES.character}"]`),
    leader_cards: !!doc.querySelector(`[data-card-zone="${ZONES.leader}"]`),
    combat_text: board ? /battle|attack|phase/i.test(board.textContent ?? "") : false,
  };
}
