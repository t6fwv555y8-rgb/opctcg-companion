import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("the loadable extension has no build step", () => {
  const manifest = JSON.parse(readFileSync("manifest.json", "utf8"));
  assert.equal(manifest.background.service_worker, "background.js");
  assert.equal(manifest.content_scripts[0].js[0], "content.js");
});

test("the background worker never mentions window", () => {
  const src = readFileSync("background.js", "utf8");
  assert.equal(src.includes("window"), false);
  assert.match(src, /127\.0\.0\.1:9003\/snapshot/);
});
