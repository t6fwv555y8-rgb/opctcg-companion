import type { BrowserGameSnapshot } from "../../types.js";
import { extractOneSimulatorSnapshot } from "./extract.js";

const DEBOUNCE_MS = 80;

let debounceTimer: number | null = null;
let observer: MutationObserver | null = null;
let lastSnapshot: BrowserGameSnapshot | null = null;
let onUpdate: ((snap: BrowserGameSnapshot) => void) | null = null;

function gameRoot(): Element | null {
  return document.querySelector(".game-board-shell") ?? document.body;
}

function scheduleExtract(): void {
  if (debounceTimer !== null) window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => {
    debounceTimer = null;
    const snap = extractOneSimulatorSnapshot();
    lastSnapshot = snap;
    onUpdate?.(snap);
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
  observer?.disconnect();
  observer = null;
  onUpdate = null;
}

export function getLastSnapshot(): BrowserGameSnapshot | null {
  return lastSnapshot;
}
