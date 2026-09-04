import { sendSnapshot, connectBridge, onBridgeStatus } from "./bridge.js";
import { getActiveAdapterId, resolveAdapter } from "./detector.js";
import { ONE_SIMULATOR_HOST } from "./adapters/onesimulator/index.js";
import {
  startOneSimulatorObserver,
  stopOneSimulatorObserver,
} from "./adapters/onesimulator/observer.js";
import type { BridgeStatus } from "./types.js";

let stopObserver: (() => void) | null = null;
let status: BridgeStatus = {
  connected: false,
  game_detected: false,
  message: "Initializing",
};

function updateBadge(): void {
  // Content scripts cannot reliably set action badges in MV3 — ask background.
  try {
    chrome.runtime?.sendMessage?.({
      type: "badge",
      text: status.connected ? (status.game_detected ? "ON" : "…") : "!",
      color: status.connected ? "#22c55e" : "#ef4444",
      title: `OPTCG Companion — ${status.message}`,
    });
  } catch {
    // ignore when extension context is invalidated
  }
}

function bootstrap(): void {
  if (window.location.hostname !== ONE_SIMULATOR_HOST) {
    return;
  }

  connectBridge();
  onBridgeStatus(({ connected, message }) => {
    status = { ...status, connected, message: `Bridge: ${message}` };
    updateBadge();
  });

  const adapter = resolveAdapter(window.location);
  if (!adapter || adapter.id !== "onesimulator") return;

  stopObserver = startOneSimulatorObserver((snap) => {
    status = {
      connected: status.connected,
      game_detected: true,
      message: snap.diagnostics?.message ?? "Game detected",
    };
    updateBadge();
    void sendSnapshot(snap);
  });

  // Initial observe
  const observed = adapter.observe();
  status.game_detected = observed.detected;
  status.message = observed.diagnostics?.message ?? status.message;
  updateBadge();
  if (observed.detected && observed.snapshot) {
    void sendSnapshot(observed.snapshot);
  } else if (observed.snapshot) {
    // diagnostics-only snapshot for unrecognized UI
    void sendSnapshot(observed.snapshot);
  }
}

window.addEventListener("beforeunload", () => {
  stopOneSimulatorObserver();
  stopObserver?.();
});

bootstrap();

export { getActiveAdapterId };
