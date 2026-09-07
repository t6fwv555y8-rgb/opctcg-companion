import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("the service worker does not touch window", () => {
  const bundle = readFileSync("dist/background.bundle.js", "utf8");
  assert.equal(
    bundle.includes("window.setTimeout"),
    false,
    "window.setTimeout crashes the MV3 service worker the first time the HUD is down"
  );
  assert.match(bundle, /globalThis\.setTimeout|setTimeout\(/);
});
