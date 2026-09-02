import type { BrowserGameSnapshot } from "../../types.js";
import { extractOneSimulatorSnapshot } from "./extract.js";

const SETTLE_MS = 120;
const DEBOUNCE_MS = 80;

let debounceTimer: number | null = null;
let settleTimer: number | null = null;
let observer: MutationObserver | null = null;
let lastSnapshot: BrowserGameSnapshot | null = null;
let onUpdate: ((snap: BrowserGameSnapshot) => void) | null = null;
let pendingMutations = 0;

function gameRoot(): Element | null {
  return document.querySelector(".game-board-shell") ?? document.body;
}

function scheduleExtract(): void {
  pendingMutations += 1;
  if (debounceTimer !== null) window.clearTimeout(debounceTimer);
  if (settleTimer !== null) window.clearTimeout(settleTimer);

  debounceTimer = window.setTimeout(() => {
    debounceTimer = null;
    // Animation settling — wait for DOM to stabilize before extraction
    settleTimer = window.setTimeout(() => {
      settleTimer = null;
      pendingMutations = 0;
      const snap = extractOneSimulatorSnapshot();
      lastSnapshot = snap;
      onUpdate?.(snap);
    }, SETTLE_MS);
  }, DEBOUNCE_MS);
}

export function startOneSimulatorObserver(
  callback: (snap: BrowserGameSnapshot) => void,
): () => void {
  stopOneSimulatorObserver();
  onUpdate = callback;

  const root = gameRoot();
  if (!root) return () => undefined;

  observer = new MutationObserver((mutations) => {
    const relevant = mutations.some(
      (m) =>
        m.type === "childList" ||
        (m.type === "attributes" &&
          (m.attributeName?.startsWith("data-card") ||
            m.attributeName === "data-zone-anchor" ||
            m.attributeName === "class")),
    );
    if (relevant) scheduleExtract();
  });

  observer.observe(root, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: [
      "data-card-zone",
      "data-card-player-id",
      "data-card-slot",
      "data-card-iid",
      "data-zone-anchor",
      "class",
    ],
  });

  scheduleExtract();

  return stopOneSimulatorObserver;
}

export function stopOneSimulatorObserver(): void {
  if (debounceTimer !== null) {
    window.clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  if (settleTimer !== null) {
    window.clearTimeout(settleTimer);
    settleTimer = null;
  }
  observer?.disconnect();
  observer = null;
  onUpdate = null;
}

export function getLastSnapshot(): BrowserGameSnapshot | null {
  return lastSnapshot;
}

export function getPendingMutations(): number {
  return pendingMutations;
}
