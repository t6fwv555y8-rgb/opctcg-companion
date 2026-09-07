import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { parseHTML } from "linkedom";

const src = readFileSync(new URL("../content.js", import.meta.url), "utf8");
const end = src.indexOf("\nfunction paintStatus");
assert.notEqual(end, -1);

function load(html) {
  const { window, document } = parseHTML(`<!DOCTYPE html><html><body>${html}</body></html>`);
  const fn = new Function(
    "document",
    `${src.slice(0, end)}; return { cardId, cardName, leaderOf, observeCombat, player };`,
  );
  return { document, window, ...fn(document) };
}

test("reads both leaders by set code and printed name", () => {
  const html = `
    <div class="game-board-shell">
      <div data-card-zone="leader" data-card-player-id="0">
        <img src="/cards/full/ST01-001.webp" alt="Monkey.D.Luffy ST01-001" />
      </div>
      <div data-card-zone="leader" data-card-player-id="1">
        <img src="/card-images/OP13-001.png" alt="Silvers Rayleigh" />
      </div>
      <div data-zone-anchor="0:hand"></div>
      <div data-zone-anchor="1:hand"></div>
    </div>
  `;
  const { leaderOf } = load(html);
  const you = leaderOf("0");
  const them = leaderOf("1");
  assert.equal(you.id, "ST01-001");
  assert.equal(you.name, "Monkey.D.Luffy");
  assert.equal(them.id, "OP13-001");
  assert.equal(them.name, "Silvers Rayleigh");
});

test("reads a leader from data-card-id when the image url is unfamiliar", () => {
  const html = `
    <div class="game-board-shell">
      <div data-card-zone="leader" data-card-player-id="0" data-card-id="OP09-061">
        <img alt="Trafalgar Law" src="https://cdn.example/art/law.jpg" />
      </div>
    </div>
  `;
  const { leaderOf } = load(html);
  const leader = leaderOf("0");
  assert.equal(leader.id, "OP09-061");
  assert.equal(leader.name, "Trafalgar Law");
});

test("card text that says attack does not invent a combat", () => {
  const html = `
    <div class="game-board-shell">
      <span class="text-yellow-500">Phase: Main</span>
      <div data-card-zone="character" data-card-player-id="0">
        When this character attacks, draw 1. Counter 2000. Blocker.
      </div>
      <div data-card-zone="leader" data-card-player-id="0">
        <img src="/cards/full/ST01-001.webp" alt="Monkey.D.Luffy" />
      </div>
      <div data-card-zone="leader" data-card-player-id="1">
        <img src="/cards/full/ST01-001.webp" alt="Monkey.D.Luffy" />
      </div>
    </div>
  `;
  const { observeCombat } = load(html);
  assert.equal(observeCombat("0", "1"), null);
});

test("a combat ring still starts a combat read", () => {
  const html = `
    <div class="game-board-shell">
      <span class="text-yellow-500">Phase: Main</span>
      <div data-card-zone="character" data-card-player-id="1" class="rotate-90 ring-2 ring-yellow-400">
        <img src="/cards/full/ST01-012.webp" alt="Sanji" />
      </div>
      <div data-card-zone="leader" data-card-player-id="0">
        <img src="/cards/full/ST01-001.webp" alt="Monkey.D.Luffy" />
      </div>
    </div>
  `;
  const { observeCombat } = load(html);
  const combat = observeCombat("0", "1");
  assert.ok(combat);
  assert.equal(combat.active, true);
  assert.equal(combat.attacker?.card_id, "ST01-012");
});
