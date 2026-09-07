import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../content.js", import.meta.url), "utf8");

test("the page reader can see a queue and a match", () => {
  assert.match(src, /page_state/);
  assert.match(src, /player_name/);
  assert.match(src, /In queue — companion is reading/);
  assert.match(src, /searching for/);
});
