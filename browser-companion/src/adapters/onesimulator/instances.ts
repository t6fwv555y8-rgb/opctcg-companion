import type { ObservedCard } from "../../types.js";

/** Stable observation-level instance key */
export interface CardInstanceKey {
  playerId: string;
  zone: string;
  slot: string;
  cardId: string | null;
  domIid: string | null;
}

export function buildInstanceKey(
  playerId: string,
  zone: string,
  slot: string,
  cardId: string | null,
  domIid: string | null,
): string {
  return `${playerId}:${zone}:${slot}:${cardId ?? "?"}:${domIid ?? "?"}`;
}

export function parseInstanceKey(key: string): CardInstanceKey | null {
  const parts = key.split(":");
  if (parts.length < 5) return null;
  const [playerId, zone, slot, cardId, domIid] = parts;
  return {
    playerId,
    zone,
    slot,
    cardId: cardId === "?" ? null : cardId,
    domIid: domIid === "?" ? null : domIid,
  };
}

/** Track board instances across snapshots to detect moves/rests without duplicating */
export class BoardInstanceTracker {
  private lastKeys = new Set<string>();

  diff(current: CardInstanceKey[]): {
    appeared: CardInstanceKey[];
    removed: CardInstanceKey[];
    unchanged: CardInstanceKey[];
  } {
    const currentSet = new Set(current.map((c) => buildInstanceKey(
      c.playerId, c.zone, c.slot, c.cardId, c.domIid,
    )));
    const appeared = current.filter(
      (c) =>
        !this.lastKeys.has(
          buildInstanceKey(c.playerId, c.zone, c.slot, c.cardId, c.domIid),
        ),
    );
    const removed: CardInstanceKey[] = [];
    for (const key of this.lastKeys) {
      if (!currentSet.has(key)) {
        const parsed = parseInstanceKey(key);
        if (parsed) removed.push(parsed);
      }
    }
    const unchanged = current.filter((c) =>
      this.lastKeys.has(
        buildInstanceKey(c.playerId, c.zone, c.slot, c.cardId, c.domIid),
      ),
    );
    this.lastKeys = currentSet;
    return { appeared, removed, unchanged };
  }
}

export function toObservedCard(
  key: CardInstanceKey,
  rested: boolean | null,
  power: number | null,
): ObservedCard {
  return {
    card_id: key.cardId,
    instance_key: buildInstanceKey(
      key.playerId,
      key.zone,
      key.slot,
      key.cardId,
      key.domIid,
    ),
    rested,
    power,
    name: null,
  };
}
