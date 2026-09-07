import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { parseHTML } from "linkedom";

const src = readFileSync(new URL("../content.js", import.meta.url), "utf8");
const start = src.indexOf("function playerIds");
const end = src.indexOf("\nfunction life(");
assert.notEqual(start, -1);
assert.notEqual(end, -1);

function loadSeat(html) {
  const { window, document } = parseHTML(`<!DOCTYPE html><html><body>${html}</body></html>`);
  const fn = new Function(
    "document",
    `${src.slice(start, end)}; return { playerIds, selfId, seatY };`,
  );
  return { document, window, ...fn(document) };
}

function place(el, top) {
  el.getBoundingClientRect = () => ({
    top,
    bottom: top + 40,
    left: 0,
    right: 80,
    width: 80,
    height: 40,
    x: 0,
    y: top,
  });
}

const board = `
  <div class="game-board-shell">
    <div data-card-zone="leader" data-card-player-id="0" id="p0-leader"></div>
    <div data-card-zone="leader" data-card-player-id="1" id="p1-leader"></div>
    <div data-zone-anchor="0:hand"></div>
    <div data-zone-anchor="1:hand"></div>
  </div>
`;

test("the player at the bottom of the board is you, even when they are id 1", () => {
  const { document, playerIds, selfId } = loadSeat(board);
  place(document.getElementById("p0-leader"), 40);
  place(document.getElementById("p1-leader"), 520);
  assert.equal(selfId(playerIds()), "1");
});

test("the player at the bottom of the board is you when they are id 0", () => {
  const { document, playerIds, selfId } = loadSeat(board);
  place(document.getElementById("p0-leader"), 520);
  place(document.getElementById("p1-leader"), 40);
  assert.equal(selfId(playerIds()), "0");
});

test("a face-up hand picks you when the seats have no size", () => {
  const html = `
    <div class="game-board-shell">
      <div data-card-zone="hand" data-card-player-id="0"></div>
      <div data-card-zone="hand" data-card-player-id="1">
        <img src="/cards/full/ST01-002.webp" />
        <img src="/cards/full/ST01-003.webp" />
      </div>
      <div data-zone-anchor="0:hand"></div>
      <div data-zone-anchor="1:hand"></div>
    </div>
  `;
  const { playerIds, selfId } = loadSeat(html);
  assert.equal(selfId(playerIds()), "1");
});
