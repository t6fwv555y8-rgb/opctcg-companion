import { parseHTML } from "linkedom";
import test from "node:test";
import assert from "node:assert/strict";

const { window } = parseHTML("<!DOCTYPE html><html></html>");
globalThis.document = window.document;
globalThis.window = window;
globalThis.getComputedStyle = () => ({ transform: "none" });

test("combat observer returns null without battle UI", async () => {
  const { observeCombat } = await import("../dist/adapters/onesimulator/combat.js");
  const { document } = parseHTML("<div class='game-board-shell'></div>");
  assert.equal(observeCombat(document), null);
});

test("combat observer extracts power from banner text", async () => {
  const { observeCombat } = await import("../dist/adapters/onesimulator/combat.js");
  const html = `
    <div class="game-board-shell">
      <div data-card-zone="character" data-card-player-id="0" class="ring-2">
        <img src="/cards/OP01-025.png" alt="Zoro" />
      </div>
      <div data-card-zone="leader" data-card-player-id="1" class="ring-2">
        <img src="/cards/ST01-001.png" alt="Leader" />
      </div>
      <span>5000 power → 7000 power</span>
    </div>`;
  const { document } = parseHTML(html);
  const obs = observeCombat(document);
  assert.ok(obs);
  assert.equal(obs.attackerPower, 5000);
  assert.equal(obs.defenderPower, 7000);
  assert.ok(obs.confidence > 0);
});

test("session detects lobby vs active game", async () => {
  const { detectGameSession } = await import("../dist/adapters/onesimulator/session.js");
  const lobby = parseHTML("<div>No active game</div>").document;
  const active = parseHTML(`
    <div class="game-board-shell">
      <div data-zone-anchor="0:life"></div>
      Turn 2
    </div>`).document;
  assert.equal(detectGameSession(lobby).phase, "lobby");
  assert.equal(detectGameSession(active).phase, "active");
});

test("selector health reports missing board", async () => {
  const { evaluateSelectorHealth } = await import("../dist/adapters/onesimulator/health.js");
  const empty = parseHTML("<html></html>").document;
  const health = evaluateSelectorHealth(empty);
  assert.equal(health.board, "missing");
  assert.equal(health.leader, "missing");
});
