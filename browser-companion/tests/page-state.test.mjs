import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../content.js", import.meta.url), "utf8");

test("the page reader can see a queue and a match", () => {
  assert.match(src, /page_state/);
  assert.match(src, /player_name/);
  assert.match(src, /In queue — companion is reading/);
  assert.match(src, /searching/);
});

test("a reloaded extension stops the old reader instead of throwing", () => {
  assert.match(src, /extension context/i);
  assert.match(src, /refresh this tab/);
});

test("cardId survives a missing node so the queue screen cannot crash the reader", () => {
  const start = src.indexOf("const CARD_ID");
  const end = src.indexOf("\nfunction playerIds");
  assert.notEqual(start, -1);
  const cardId = new Function(`${src.slice(start, end)}; return cardId;`)();
  assert.equal(cardId(null), null);
  assert.equal(cardId(undefined), null);
});
