// Reads the OneSimulator board and hands it to background.js.
// Runs in the page, so `window` is fine here.

const CARD_ID = /\b((?:OP|ST|EB|PRB|P)-\d{2}-\d{3}[A-Z]?)\b/i;
const CARD_SRC = /\/cards\/(?:full|thumbnail)\/([^/.]+)\.webp/i;

function cardId(el) {
  const img = el.querySelector("img");
  const fromSrc = img?.src?.match(CARD_SRC);
  if (fromSrc) return fromSrc[1].replace(/_/g, "-").toUpperCase();
  const fromAlt = img?.alt?.match(CARD_ID);
  if (fromAlt) return fromAlt[1].toUpperCase();
  const fromText = (el.textContent || "").match(CARD_ID);
  return fromText ? fromText[1].toUpperCase() : null;
}

function playerIds() {
  const ids = new Set();
  document.querySelectorAll("[data-card-player-id]").forEach((el) => {
    const id = el.getAttribute("data-card-player-id");
    if (id) ids.add(id);
  });
  document.querySelectorAll("[data-zone-anchor]").forEach((el) => {
    const id = el.getAttribute("data-zone-anchor")?.split(":")[0];
    if (id) ids.add(id);
  });
  return [...ids].sort();
}

function selfId(ids) {
  let best = ids[0] || "0";
  let maxTop = -1;
  document.querySelectorAll('[data-zone-anchor$=":hand"]').forEach((el) => {
    const id = el.getAttribute("data-zone-anchor")?.split(":")[0];
    const top = el.getBoundingClientRect().top;
    if (id && top >= maxTop) {
      maxTop = top;
      best = id;
    }
  });
  return best;
}

function life(playerId) {
  const n = document.querySelectorAll(
    `[data-card-zone="life"][data-card-player-id="${playerId}"]`,
  ).length;
  if (n > 0) return n;
  const anchor = document.querySelector(`[data-zone-anchor="${playerId}:life"]`);
  const m = anchor?.textContent?.match(/Life:\s*(\d+)/i);
  return m ? Number(m[1]) : null;
}

function handCount(playerId) {
  const n = document.querySelectorAll(
    `[data-card-zone="hand"][data-card-player-id="${playerId}"]`,
  ).length;
  return n > 0 ? n : null;
}

function don(playerId) {
  const field = document.querySelector(`[data-zone-anchor="${playerId}:donField"]`);
  if (!field) return { active: null, rested: null };
  const cards = field.querySelectorAll("[data-don-iid], [data-card-zone='donField']");
  let active = 0;
  let rested = 0;
  cards.forEach((el) => {
    const cls = `${el.className || ""}`;
    if (cls.includes("rotate-90")) rested += 1;
    else active += 1;
  });
  if (active + rested === 0) return { active: null, rested: null };
  return { active, rested };
}

function board(playerId) {
  const cards = [];
  for (const zone of ["leader", "character", "stage"]) {
    document
      .querySelectorAll(`[data-card-zone="${zone}"][data-card-player-id="${playerId}"]`)
      .forEach((el) => {
        cards.push({
          card_id: cardId(el),
          name: el.querySelector("img")?.alt || null,
          rested: `${el.className || ""}`.includes("rotate-90"),
        });
      });
  }
  return cards;
}

function known(playerId, isSelf) {
  const ids = new Set();
  const zones = isSelf
    ? ["leader", "character", "stage", "trash", "hand"]
    : ["leader", "character", "stage", "trash"];
  for (const zone of zones) {
    document
      .querySelectorAll(`[data-card-zone="${zone}"][data-card-player-id="${playerId}"]`)
      .forEach((el) => {
        const id = cardId(el);
        if (id) ids.add(id);
      });
  }
  return [...ids];
}

function cleanName(raw) {
  const n = String(raw || "").replace(/\s+/g, " ").trim();
  if (n.length < 2 || n.length > 32) return null;
  if (
    /^(you|me|opponent|player\s*[12]?|life|hand|deck|don|phase|turn|vs|queue)$/i.test(
      n,
    )
  ) {
    return null;
  }
  if (CARD_ID.test(n)) return null;
  return n;
}

function pageState() {
  const inMatch = Boolean(
    document.querySelector(".game-board-shell") &&
      document.querySelector("[data-zone-anchor]"),
  );
  if (inMatch) return "match";

  const href = String(location.href || "").toLowerCase();
  const text = String(document.body?.innerText || "").slice(0, 12000);
  const queued =
    /queue|matchmaking/.test(href) ||
    /\b(in queue|queued)\b/i.test(text) ||
    /searching for (an? )?(opponent|match|player)/i.test(text) ||
    /finding (an? )?(opponent|match|player)/i.test(text) ||
    /looking for (an? )?(opponent|match)/i.test(text) ||
    /waiting for (an? )?opponent/i.test(text) ||
    /matchmaking/i.test(text);
  return queued ? "queue" : "lobby";
}

function playerName(playerId, isSelf) {
  const scoped = [
    `[data-player-name][data-card-player-id="${playerId}"]`,
    `[data-player-id="${playerId}"][data-player-name]`,
    `[data-username][data-card-player-id="${playerId}"]`,
    `[data-player-id="${playerId}"][data-username]`,
    `[data-zone-anchor^="${playerId}:"] [data-player-name]`,
    `[data-zone-anchor^="${playerId}:"] [data-username]`,
  ];
  for (const sel of scoped) {
    const el = document.querySelector(sel);
    const n = cleanName(
      el?.getAttribute("data-player-name") ||
        el?.getAttribute("data-username") ||
        el?.textContent,
    );
    if (n) return n;
  }

  const zone = document.querySelector(`[data-zone-anchor^="${playerId}:"]`);
  const cluster =
    zone?.closest("section, article, [class*='player']") || zone?.parentElement;
  const labeled = cluster?.querySelector(
    "[data-player-name], [data-username], [class*='username'], [class*='player-name']",
  );
  const near = cleanName(
    labeled?.getAttribute("data-player-name") ||
      labeled?.getAttribute("data-username") ||
      labeled?.textContent,
  );
  if (near) return near;

  if (isSelf) {
    const me = document.querySelector(
      "[data-self-name], [data-own-name], header [data-username], nav [data-username]",
    );
    const n = cleanName(
      me?.getAttribute("data-self-name") ||
        me?.getAttribute("data-username") ||
        me?.textContent,
    );
    if (n) return n;
  }
  return null;
}

function selectedLeaderId() {
  return (
    cardId(document.querySelector("[data-selected-leader]")) ||
    cardId(
      document.querySelector(
        '[aria-pressed="true"] img[src*="/cards/"], [data-selected="true"] img[src*="/cards/"]',
      )?.closest("[data-card-zone], div") || document.createElement("div"),
    )
  );
}

function player(playerId, isSelf) {
  const d = don(playerId);
  const cards = board(playerId);
  const leaderEl = document.querySelector(
    `[data-card-zone="leader"][data-card-player-id="${playerId}"]`,
  );
  return {
    life: life(playerId),
    hand_count: handCount(playerId),
    active_don: d.active,
    rested_don: d.rested,
    leader_id: cardId(leaderEl || document.createElement("div")) || (isSelf ? selectedLeaderId() : null),
    player_name: playerName(playerId, isSelf),
    known_cards: known(playerId, isSelf),
    board: cards,
  };
}

function phaseAndTurn() {
  const text = document.querySelector(".game-board-shell")?.textContent || "";
  const phaseM = text.match(/Phase:\s*([^\n]+)/i);
  const turnM = text.match(/Turn\s+(\d+)/i);
  let phase = null;
  if (phaseM) {
    const raw = phaseM[1].toLowerCase();
    if (raw.includes("draw")) phase = "Draw";
    else if (raw.includes("don")) phase = "Don";
    else if (raw.includes("main")) phase = "Main";
    else if (raw.includes("end")) phase = "End";
    else if (raw.includes("combat") || raw.includes("battle")) phase = "Combat";
  }
  return { phase, turn: turnM ? Number(turnM[1]) : null };
}

function readBoard() {
  const ids = playerIds();
  const you = selfId(ids);
  const them = ids.find((id) => id !== you) || (you === "0" ? "1" : "0");
  const state = pageState();
  const inMatch = state === "match";
  const { phase, turn } = phaseAndTurn();
  const self = inMatch
    ? player(you, true)
    : {
        player_name: playerName(you, true),
        leader_id: selectedLeaderId(),
        known_cards: [],
        board: [],
      };
  const opponent = inMatch ? player(them, false) : { player_name: playerName(them, false) };

  return {
    timestamp: Date.now(),
    source: "onesimulator",
    page_state: state,
    turn,
    phase,
    self,
    opponent: opponent.player_name || inMatch ? opponent : null,
    diagnostics: {
      site_detected: true,
      game_detected: inMatch,
      ui_recognized: inMatch,
      message:
        state === "match"
          ? "Game detected"
          : state === "queue"
            ? "In queue"
            : "In lobby",
      found: { queue: state === "queue", lobby: state === "lobby", match: inMatch },
    },
  };
}

function paintStatus(text, ok) {
  let el = document.getElementById("optcg-companion-status");
  if (!el) {
    el = document.createElement("div");
    el.id = "optcg-companion-status";
    el.style.cssText =
      "position:fixed;z-index:2147483647;right:12px;bottom:12px;padding:8px 10px;" +
      "font:12px/1.3 system-ui,sans-serif;border-radius:8px;color:#fff;" +
      "box-shadow:0 2px 8px rgba(0,0,0,.4)";
    document.documentElement.appendChild(el);
  }
  el.style.background = ok ? "#166534" : "#7f1d1d";
  el.textContent = text;
}

function send() {
  const snapshot = readBoard();
  const state = snapshot.page_state;

  if (!chrome.runtime?.id) {
    paintStatus("Companion extension was reloaded — refresh this tab", false);
    return;
  }

  chrome.runtime.sendMessage({ type: "snapshot", snapshot }, (res) => {
    if (chrome.runtime.lastError || !res) {
      paintStatus("Start the app first:  ./start onesimulator", false);
      return;
    }
    if (!res.ok) {
      paintStatus("HUD is not open. Run ./start onesimulator", false);
      return;
    }
    const label =
      state === "match"
        ? "Companion is reading this match"
        : state === "queue"
          ? "In queue — companion is reading"
          : "Companion ready — in lobby";
    paintStatus(label, true);
  });
}

paintStatus("Companion starting…", false);
send();
setInterval(send, 800);
