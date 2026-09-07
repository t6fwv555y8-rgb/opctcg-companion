import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("the loadable worker posts snapshots and never opens a WebSocket", () => {
  const src = readFileSync("background.js", "utf8");
  assert.equal(src.includes("new WebSocket"), false);
  assert.equal(src.includes("ws://127.0.0.1:9003/ws"), false);
  assert.equal(src.includes("window"), false);
  assert.match(src, /127\.0\.0\.1:9003\/snapshot/);
});
