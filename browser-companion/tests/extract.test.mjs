import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { parseHTML } from "linkedom";
import test from "node:test";
import assert from "node:assert/strict";

// Minimal DOM globals for compiled extract module
const { window } = parseHTML("<!DOCTYPE html><html></html>");
globalThis.document = window.document;
globalThis.window = window;
globalThis.getComputedStyle = () => ({ transform: "none" });

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fixture = readFileSync(
  join(root, "../fixtures/onesimulator/board_main.html"),
  "utf8",
);

test("OneSimulator hostname adapter match", async () => {
  const { ONE_SIMULATOR_HOST } = await import("../dist/adapters/onesimulator/selectors.js");
  assert.equal(ONE_SIMULATOR_HOST, "onesimulator.slidingcodes.com");
});

test("extractOneSimulatorSnapshot from fixture HTML", async () => {
  const { document } = parseHTML(fixture);
  const { extractOneSimulatorSnapshot } = await import("../dist/adapters/onesimulator/extract.js");
  const snap = extractOneSimulatorSnapshot(document);
  assert.equal(snap.source, "onesimulator");
  assert.equal(snap.phase, "Main");
  assert.equal(snap.turn, 3);
  assert.equal(snap.self?.life, 2);
  assert.equal(snap.self?.hand_count, 3);
  assert.equal(snap.self?.active_don, 1);
  assert.equal(snap.self?.rested_don, 1);
  assert.equal(snap.opponent?.hand_count, 5);
  assert.ok((snap.self?.board?.length ?? 0) >= 3);
});

test("duplicate card IDs use distinct instance keys", async () => {
  const { document } = parseHTML(fixture);
  const { extractOneSimulatorSnapshot } = await import("../dist/adapters/onesimulator/extract.js");
  const snap = extractOneSimulatorSnapshot(document);
  const dupes = snap.self?.board?.filter((c) => c.card_id === "OP01-025") ?? [];
  assert.equal(dupes.length, 2);
  assert.notEqual(dupes[0]?.instance_key, dupes[1]?.instance_key);
});
