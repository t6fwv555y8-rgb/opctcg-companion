import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("the loadable extension has no build step", () => {
  const manifest = JSON.parse(readFileSync("manifest.json", "utf8"));
  assert.equal(manifest.background.service_worker, "background.js");
  assert.equal(manifest.content_scripts[0].js[0], "content.js");
});

test("the reader prefers the mutation observer over an 800ms poll", () => {
  const src = readFileSync("content.js", "utf8");
  assert.match(src, /MutationObserver/);
  assert.match(src, /HEARTBEAT_MS = 5000/);
  assert.equal(src.includes("setInterval(send, 800)"), false);
});

test("the background worker never mentions window", () => {
  const src = readFileSync("background.js", "utf8");
  assert.equal(src.includes("window"), false);
  assert.match(src, /127\.0\.0\.1:9003\/snapshot/);
});
