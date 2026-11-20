#!/usr/bin/env python3
"""Download recent tournament top-cut decklists and write them where the app
loads them, so the HUD can compare your deck with what is actually winning.

Sources (pick with --source):
  egman      Egman Deck Builder's public tournament API. One request, the newest
             sets (OP17 results are there), official AND unofficial/online
             events. This is the default.
  limitless  onepiece.limitlesstcg.com (their robots.txt allows crawling). Older
             data, scraped page by page, so it is slower.

Be a good guest: everything downloaded is cached, requests are spaced out, and
only what the comparison needs is kept. Player names are NOT stored.

    python3 scripts/sync_meta.py                         # newest set, Egman
    python3 scripts/sync_meta.py --set-group OP16 --top 8
    python3 scripts/sync_meta.py --source limitless --formats OP16

Re-run it whenever you want fresh data, then use "Reload data" in the HUD.
"""
import argparse
import datetime as dt
import html
import json
import os
import re
import sys
import time
import urllib.parse
import urllib.request

LIMITLESS = "https://onepiece.limitlesstcg.com"
EGMAN = "https://deckbuilder.egmanevents.com"
EGMAN_API = EGMAN + "/api/tournaments/optcg"
USER_AGENT = "optcg-companion-meta-sync (personal tool; slow, cached)"


def data_dir() -> str:
    if sys.platform == "darwin":
        root = os.path.expanduser("~/Library/Application Support")
    elif os.name == "nt":
        root = os.environ.get("APPDATA", os.path.expanduser("~"))
    else:
        root = os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share"))
    return os.path.join(root, "optcg-companion")


def http_get(url: str, accept: str = "*/*", timeout: int = 60) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": accept})
    last_error = None
    for attempt in range(1, 4):
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.read()
        except Exception as e:  # transient resets happen; back off and retry
            last_error = e
            time.sleep(attempt * 2)
    raise RuntimeError(f"{url}: {last_error}")


def cached_get(cache_dir: str, name: str, url: str, ttl_hours: float, refresh: bool,
               accept: str = "*/*", delay: float = 0.0) -> str:
    os.makedirs(cache_dir, exist_ok=True)
    path = os.path.join(cache_dir, re.sub(r"[^A-Za-z0-9]+", "_", name).strip("_") + ".cache")
    if not refresh and os.path.exists(path):
        if (time.time() - os.path.getmtime(path)) / 3600 < ttl_hours:
            with open(path, encoding="utf-8") as f:
                return f.read()
    if delay:
        time.sleep(delay)
    body = http_get(url, accept).decode("utf-8", "replace")
    with open(path, "w", encoding="utf-8") as f:
        f.write(body)
    return body


# ------------------------------------------------------- formats and banlist

FORMAT_RE = re.compile(r"(OP\d{1,2}(?:\.5)?|EB\d{1,2}(?:\.5)?|PRB\d{1,2})", re.I)


def normalize_format(label: str):
    """'ENG OP17' / 'OP17' -> ('OP17', 'ENG'); 'JPN OP16+STs' -> ('OP16', 'JPN').

    Events label the same format many ways. The set (and a '.5' marker) is the
    format; JPN lists are kept apart because Japan's card pool runs ahead.
    """
    label = (label or "").strip()
    m = FORMAT_RE.search(label)
    key = m.group(1).upper() if m else (label.upper() or "UNKNOWN")
    region = "JPN" if "JPN" in label.upper() else "ENG"
    return key, region


def write_banlist(cache_dir: str, refresh: bool, out_path: str) -> None:
    """Ban list, pair bans and rotation from Egman's game data."""
    game = json.loads(cached_get(cache_dir, "egman_gamedata", EGMAN + "/api/gamedata/optcg",
                                 6, refresh, accept="application/json"))
    cards = json.loads(cached_get(cache_dir, "egman_cards", EGMAN + "/api/cards/optcg",
                                  24, refresh, accept="application/json", delay=1.0))
    ban = game.get("banList") or {}
    banned, pairs, rotated_blocks = [], {}, []
    for e in ban.get("entries") or []:
        code, status = (e.get("card_code") or "").strip().upper(), e.get("status") or ""
        if status == "banned" and code:
            banned.append(code)
        elif status in ("pair_ban_a", "pair_ban_b") and code:
            group = pairs.setdefault(e.get("pair_group") or code, {"a": [], "b": []})
            group["a" if status.endswith("_a") else "b"].append(code)
        elif status == "rotated" and code.isdigit():
            rotated_blocks.append(int(code))
    rotated_cards = sorted({
        (c.get("card_code") or "").upper()
        for c in cards
        if str(c.get("block")).isdigit() and int(c["block"]) in rotated_blocks and c.get("card_code")
    })
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump({
            "source": EGMAN,
            "title": (ban.get("meta") or {}).get("title", ""),
            "last_updated": (ban.get("meta") or {}).get("last_updated", ""),
            "fetched_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
            "banned": sorted(set(banned)),
            "pair_bans": list(pairs.values()),
            "rotated_blocks": rotated_blocks,
            "rotated_cards": rotated_cards,
        }, f)
    print(f"banlist: {len(set(banned))} banned, {len(pairs)} pair bans, "
          f"{len(rotated_cards)} rotated cards (blocks {rotated_blocks}), "
          f"updated {(ban.get('meta') or {}).get('last_updated', '?')}")


# --------------------------------------------------------------------- Egman

def parse_egman_deck(result: dict):
    """One placement -> {leader, archetype, cards} from its deck-builder link."""
    urls = result.get("deck_list_url") or []
    if isinstance(urls, str):
        urls = [urls]
    if not urls:
        return None
    query = urllib.parse.parse_qs(urllib.parse.urlparse(urls[0]).query)
    raw = (query.get("deck") or [""])[0]
    cards, order = {}, []
    for part in raw.split(","):
        card_id, _, qty = part.partition(":")
        card_id = card_id.strip().upper()
        if not card_id or not qty.strip().isdigit():
            continue
        cards[card_id] = cards.get(card_id, 0) + int(qty)
        order.append(card_id)
    if not cards:
        return None
    declared = (result.get("deck_type") or "").strip().upper()
    leader = declared if declared in cards else order[0]
    cards.pop(leader, None)
    if sum(cards.values()) < 40:
        return None  # incomplete list; skip rather than learn from it
    return {
        "leader": leader,
        "archetype": "/".join(result.get("deck_colors") or []),
        "cards": cards,
    }


def egman_decks(args, cache_dir: str):
    body = cached_get(cache_dir, "egman_tournaments", EGMAN_API, 6, args.refresh,
                      accept="application/json")
    tournaments = json.loads(body)

    group = args.set_group.upper() or max(
        tournaments, key=lambda t: t.get("start_date") or ""
    ).get("set_group", "")
    picked = [
        t for t in tournaments
        if (t.get("set_group") or "").upper() == group
        and (t.get("player_count") or 0) >= args.min_players
    ]
    picked.sort(key=lambda t: t.get("start_date") or "", reverse=True)
    picked = picked[: args.max_tournaments]
    print(f"{len(picked)} tournaments in {group} with {args.min_players}+ players")

    decks, used = [], []
    # The same pilot posting the same 50 cards at two events is one signal, not
    # two: keep the newest. Different pilots with identical lists are kept,
    # since copying a winning list is a real signal.
    seen_pilot_lists, skipped_repeats = set(), 0
    for t in picked:
        found = 0
        results = sorted(t.get("tournament_results") or [], key=lambda r: r.get("placement") or 999)
        for r in results:
            if (r.get("placement") or 999) > args.top:
                continue
            deck = parse_egman_deck(r)
            if not deck:
                continue
            pilot = (r.get("player_name") or "").strip().casefold()
            if pilot and not args.keep_repeats:
                key = (pilot, deck["leader"], tuple(sorted(deck["cards"].items())))
                if key in seen_pilot_lists:
                    skipped_repeats += 1
                    continue
                seen_pilot_lists.add(key)
            fmt, region = normalize_format(t.get("format") or t.get("set_group") or "")
            deck.update(
                tournament_id=str(t["id"]),
                placement=str(r.get("placement")),
                event_type=t.get("event_type") or "",
                format=fmt,
                region=region,
            )
            decks.append(deck)
            found += 1
        if found:
            fmt, region = normalize_format(t.get("format") or t.get("set_group") or "")
            used.append({
                "id": str(t["id"]),
                "name": t.get("tournament_name") or "",
                "date": t.get("start_date") or "",
                "format": fmt,
                "region": region,
                "players": t.get("player_count") or 0,
                "event_type": t.get("event_type") or "",
            })
        print(f"  {t.get('start_date')} {t.get('tournament_name')} "
              f"({t.get('player_count')}, {t.get('event_type')}): {found} lists")
    if skipped_repeats:
        print(f"  skipped {skipped_repeats} repeat lists (same pilot, same cards, older event)")
    return sorted({t["format"] for t in used}), used, decks, EGMAN


# ------------------------------------------------------------------ Limitless

ROW_RE = re.compile(
    r'<tr data-date="(?P<date>[\d-]+)"[^>]*?data-name="(?P<name>[^"]*)"\s+data-format="(?P<format>[^"]*)"\s+'
    r'data-players="(?P<players>\d+)".*?href="/tournaments/(?P<id>\d+)"',
    re.S,
)
DECK_SPLIT = '<div class="tournament-decklist">'
PLACE_RE = re.compile(r'class="decklist-toggle"[^>]*>\s*([^<]*?)\s*<', re.S)
HEADING_RE = re.compile(r'class="decklist-column-heading">\s*([^<]*?)\s*<')
CARD_RE = re.compile(r'class="decklist-card" data-count="(\d+)" data-id="([A-Za-z0-9\-]+)"')


def parse_limitless_decks(page: str):
    for chunk in page.split(DECK_SPLIT)[1:]:
        place = PLACE_RE.search(chunk)
        leader, cards = None, {}
        headings = [(m.start(), m.group(1)) for m in HEADING_RE.finditer(chunk)]
        for m in CARD_RE.finditer(chunk):
            section = ""
            for pos, name in headings:
                if pos < m.start():
                    section = name
            card_id, count = m.group(2).upper(), int(m.group(1))
            if section.lower().startswith("leader"):
                leader = card_id
            else:
                cards[card_id] = cards.get(card_id, 0) + count
        if not leader or sum(cards.values()) < 40:
            continue
        title = re.search(r'class="decklist-title">\s*([^<]*?)\s*<', chunk, re.S)
        yield {
            "placement": (place.group(1).strip() if place else "").split("\n")[0],
            "archetype": html.unescape(title.group(1)) if title else "",
            "leader": leader,
            "cards": cards,
        }


def limitless_decks(args, cache_dir: str):
    def page(path: str, ttl: float) -> str:
        return cached_get(cache_dir, "limitless" + path, LIMITLESS + path, ttl,
                          args.refresh, delay=args.delay)

    tournaments = []
    for m in ROW_RE.finditer(page("/tournaments", 6)):
        tournaments.append({
            "id": m.group("id"), "name": html.unescape(m.group("name")),
            "date": m.group("date"), "format": m.group("format"),
            "players": int(m.group("players")), "event_type": "",
        })
    if not tournaments:
        raise RuntimeError("could not read the tournament list (site layout changed?)")

    wanted = [f.strip().upper() for f in args.formats.split(",") if f.strip()]
    if not wanted:
        wanted = [max(tournaments, key=lambda t: t["date"])["format"].upper()]
    picked = [t for t in tournaments
              if t["format"].upper() in wanted and t["players"] >= args.min_players]
    picked.sort(key=lambda t: t["date"], reverse=True)
    picked = picked[: args.max_tournaments]
    print(f"{len(picked)} tournaments in {','.join(wanted)} with {args.min_players}+ players")

    decks, used = [], []
    for t in picked:
        try:
            body = page(f"/tournaments/{t['id']}/decklists", 24 * 30)
        except RuntimeError as e:
            print(f"skip {t['id']}: {e}", file=sys.stderr)
            continue
        found = list(parse_limitless_decks(body))[: args.top]
        for d in found:
            d["tournament_id"] = t["id"]
            d["event_type"] = ""
            decks.append(d)
        if found:
            used.append(t)
        print(f"  {t['date']} {t['name']} ({t['players']}): {len(found)} lists")
    return wanted, used, decks, LIMITLESS


# ----------------------------------------------------------------------- main

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--source", choices=["egman", "limitless"], default="egman")
    ap.add_argument("--set-group", default="", help="egman: set such as OP17 (default: the newest)")
    ap.add_argument("--formats", default="", help="limitless: comma list such as OP16 (default: the newest)")
    ap.add_argument("--min-players", type=int, default=32)
    ap.add_argument("--max-tournaments", type=int, default=40)
    ap.add_argument("--top", type=int, default=8, help="keep the best N placements per tournament")
    ap.add_argument("--delay", type=float, default=2.0, help="limitless: seconds between downloads")
    ap.add_argument("--keep-repeats", action="store_true", help="egman: keep a pilot's repeated identical lists")
    ap.add_argument("--refresh", action="store_true", help="ignore the cache")
    ap.add_argument("--out", default=os.path.join(data_dir(), "meta_decks.json"))
    args = ap.parse_args()

    cache_dir = os.path.join(data_dir(), "meta_cache")
    try:
        fetch = egman_decks if args.source == "egman" else limitless_decks
        wanted, used, decks, source = fetch(args, cache_dir)
    except RuntimeError as e:
        print(e, file=sys.stderr)
        return 1

    try:
        write_banlist(cache_dir, args.refresh, os.path.join(os.path.dirname(os.path.abspath(args.out)), "banlist.json"))
    except RuntimeError as e:  # the lists are still useful without it
        print(f"skipped the banlist: {e}", file=sys.stderr)

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(
            {
                "source": source,
                "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
                "formats": wanted,
                "top_per_tournament": args.top,
                "tournaments": used,
                "decks": decks,
            },
            f,
            ensure_ascii=False,
        )
    leaders = {d["leader"] for d in decks}
    print(f"wrote {len(decks)} decklists ({len(leaders)} leaders) to {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
