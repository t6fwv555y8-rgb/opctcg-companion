import type { BrowserGameSnapshot } from "./types";

const BRIDGE_HTTP = "http://127.0.0.1:9003/snapshot";
const BRIDGE_WS = "ws://127.0.0.1:9003/ws";
const MAX_PAYLOAD = 256 * 1024;

let ws: WebSocket | null = null;
let reconnectTimer: number | null = null;

export function connectBridge(): void {
  if (ws?.readyState === WebSocket.OPEN) return;

  ws = new WebSocket(BRIDGE_WS);
  ws.onopen = () => {
    console.info("[optcg-companion] bridge connected");
  };
  ws.onclose = () => scheduleReconnect();
  ws.onerror = () => scheduleReconnect();
}

function scheduleReconnect(): void {
  if (reconnectTimer !== null) return;
  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    connectBridge();
  }, 3000);
}

export async function sendSnapshot(snapshot: BrowserGameSnapshot): Promise<boolean> {
  const body = JSON.stringify({ type: "snapshot", ...snapshot });
  if (body.length > MAX_PAYLOAD) {
    console.warn("[optcg-companion] snapshot too large, dropped");
    return false;
  }

  if (ws?.readyState === WebSocket.OPEN) {
    ws.send(body);
    return true;
  }

  try {
    const resp = await fetch(BRIDGE_HTTP, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
    });
    return resp.ok;
  } catch {
    return false;
  }
}

export function pingBridge(): void {
  if (ws?.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: "ping" }));
  }
}
