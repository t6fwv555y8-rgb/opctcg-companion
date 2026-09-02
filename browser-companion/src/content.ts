import { detectGame, resolveAdapter } from "./detector";
import { connectBridge, sendSnapshot } from "./bridge";

const POLL_MS = 1000;

function tick(): void {
  const adapter = resolveAdapter(window.location);
  if (!adapter || !detectGame(window.location)) return;

  const observed = adapter.observe();
  if (observed.detected && observed.snapshot) {
    void sendSnapshot(observed.snapshot);
  }
}

connectBridge();
setInterval(tick, POLL_MS);
