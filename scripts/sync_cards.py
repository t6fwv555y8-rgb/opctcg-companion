#!/usr/bin/env python3
"""Download the full card list (names, costs, types, effect and trigger text)
and write it where the app loads it.

Primary source: OneSimulator's public card data. Cards it does not have yet (the
newest sets: OP17, OP18, EB05) are filled in from Egman Deck Builder's public
card list.

The app reads <data dir>/cards_full.json at startup, after its built-in
starter set, so every card the coach sees comes with its real rules text.
Re-run this after new sets come out, then restart the app.

    python3 scripts/sync_cards.py            # default location
    python3 scripts/sync_cards.py --out cards_full.json
"""
import argparse
import json
import os
import re
import sys
import time
import urllib.request

BASE = "https://cdn.slidingcodes.com/english/data"
EGMAN_CARDS = "https://deckbuilder.egmanevents.com/api/cards/optcg"


def default_out() -> str:
    if sys.platform == "darwin":
        root = os.path.expanduser("~/Library/Application Support")
    elif os.name == "nt":
        root = os.environ.get("APPDATA", os.path.expanduser("~"))
    else:
        root = os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share"))
    return os.path.join(root, "optcg-companion", "cards_full.json")


def fetch(url: str, attempts: int = 4):
    req = urllib.request.Request(url, headers={"User-Agent": "optcg-companion-card-sync"})
    for attempt in range(1, attempts + 1):
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.load(resp)
        except Exception:
            if attempt == attempts:
                raise
            time.sleep(attempt)  # transient resets happen; back off and retry


def clean(text: str) -> str:
    """Card text sometimes carries HTML line breaks; keep it as plain text."""
    text = re.sub(r"<br\s*/?>", " ", text or "", flags=re.I)
    return re.sub(r"<[^>]+>", "", text).strip()


def keyword(effect: str, name: str) -> bool:
    return f"[{name}]" in effect


def to_entry(card: dict):
    raw_id = card.get("id") or ""
    # Alternate-art printings ("OP01-016_p3") share the base card's rules.
    if re.search(r"_p\d+$", raw_id):
        return None
    effect = clean(card.get("effect"))
    trigger = clean(card.get("trigger"))
    types = card.get("types") or []
    attrs = card.get("attributes") or []
    # Types and trigger live in the text so the coach can see card synergies
    # ("Straw Hat Crew" payoffs, [Trigger] effects) without a schema change.
    parts = []
    if types:
        parts.append("Type: " + " / ".join(types))
    if attrs:
        parts.append("Attribute: " + " / ".join(attrs))
    if effect:
        parts.append(effect)
    if trigger:
        parts.append(trigger)
    return {
        "card_id": raw_id,
        "name": card.get("name") or raw_id,
        "card_type": (card.get("category") or "character").lower(),
        "cost": card.get("cost") or 0,
        "power": card.get("power") or 0,
        "counter": card.get("counter") or 0,
        "color": "/".join(card.get("colors") or []),
        "rush": keyword(effect, "Rush"),
        "blocker": keyword(effect, "Blocker"),
        "double_attack": keyword(effect, "Double Attack"),
        "banish": keyword(effect, "Banish"),
        "text": "\n".join(parts),
    }


def egman_entry(card: dict):
    card_id = (card.get("card_code") or "").strip().upper()
    if not card_id or re.search(r"_p\d+$", card_id):
        return None
    effect = clean(card.get("effect"))
    trigger = clean(card.get("trigger"))
    types = card.get("type") or []
    attribute = card.get("attribute") or ""
    parts = []
    if types:
        parts.append("Type: " + " / ".join(types))
    if attribute:
        parts.append("Attribute: " + str(attribute))
    if effect:
        parts.append(effect)
    if trigger:
        parts.append(trigger)
    colors = card.get("color") or []
    if isinstance(colors, str):
        colors = [colors]
    return {
        "card_id": card_id,
        "name": card.get("name") or card_id,
        "card_type": (card.get("category") or "character").lower(),
        "cost": card.get("cost") or 0,
        "power": card.get("power") or 0,
        "counter": card.get("counter") or 0,
        "color": "/".join(colors),
        "rush": keyword(effect, "Rush"),
        "blocker": keyword(effect, "Blocker"),
        "double_attack": keyword(effect, "Double Attack"),
        "banish": keyword(effect, "Banish"),
        "text": "\n".join(parts),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=default_out())
    args = ap.parse_args()

    packs = fetch(f"{BASE}/packs.json")
    cards = {}
    for pack_id in packs:
        try:
            data = fetch(f"{BASE}/{pack_id}.json")
        except Exception as e:  # a missing pack should not sink the rest
            print(f"skip {pack_id}: {e}", file=sys.stderr)
            continue
        for card in data:
            entry = to_entry(card)
            if entry and entry["card_id"] not in cards:
                cards[entry["card_id"]] = entry
        time.sleep(0.15)  # be polite to the host

    # Newest sets are missing from the primary source; fill them from Egman.
    added = 0
    try:
        for card in fetch(EGMAN_CARDS):
            entry = egman_entry(card)
            if entry and entry["card_id"] not in cards:
                cards[entry["card_id"]] = entry
                added += 1
    except Exception as e:  # the primary list is still usable without it
        print(f"skipped Egman card list: {e}", file=sys.stderr)

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump({"cards": list(cards.values())}, f, ensure_ascii=False)
    print(f"wrote {len(cards)} cards to {args.out} ({added} filled in from Egman)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
