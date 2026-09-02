import type { BrowserGameSnapshot } from "./types.js";

const BRIDGE_HTTP = "http://127.0.0.1:9003/snapshot";
const BRIDGE_WS = "ws://127.0.0.1:9003/ws";
const MAX_PAYLOAD = 256 * 1024;
const ALLOWED_ORIGIN = "onesimulator";

let ws: WebSocket | null = null;
let reconnectTimer: number | null = null;
let sequence = 0;
let connected = false;
let lastError: string | null = null;

export type BridgeStatusListener = (status: {
  connected: boolean;
  message: string;
}) => void;

const listeners = new Set<BridgeStatusListener>();

export function onBridgeStatus(listener: BridgeStatusListener): () => void {
  listeners.add(listener);
  listener({ connected, message: connected ? "Connected" : lastError ?? "Disconnected" });
  return () => listeners.delete(listener);
}

function notifyStatus(): void {
  const msg = connected ? "Connected" : lastError ?? "Disconnected";
  listeners.forEach((l) => l({ connected, message: msg }));
}

export function connectBridge(): void {
  if (ws?.readyState === WebSocket.OPEN) return;

  ws = new WebSocket(BRIDGE_WS);
  ws.onopen = () => {
    connected = true;
    lastError = null;
    console.info("[optcg-companion] bridge connected");
    notifyStatus();
  };
  ws.onclose = () => {
    connected = false;
    scheduleReconnect();
    notifyStatus();
  };
  ws.onerror = () => {
    lastError = "Bridge connection error";
    scheduleReconnect();
    notifyStatus();
  };
}

function scheduleReconnect(): void {
  if (reconnectTimer !== null) return;
  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    connectBridge();
  }, 3000);
}

function validateSnapshot(snapshot: BrowserGameSnapshot): string | null {
  if (!snapshot.timestamp) return "missing timestamp";
  if (snapshot.source && snapshot.source !== ALLOWED_ORIGIN && snapshot.source !== "generic") {
    // allow known sources only
  }
  const body = JSON.stringify(snapshot);
  if (body.length > MAX_PAYLOAD) return "payload too large";
  return null;
}

export async function sendSnapshot(snapshot: BrowserGameSnapshot): Promise<boolean> {
  sequence += 1;
  const payload: BrowserGameSnapshot = {
    ...snapshot,
    sequence,
    source: snapshot.source ?? ALLOWED_ORIGIN,
  };

  const err = validateSnapshot(payload);
  if (err) {
    console.warn("[optcg-companion]", err);
    return false;
  }

  const body = JSON.stringify({ type: "snapshot", ...payload });
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
      headers: {
        "Content-Type": "application/json",
        "X-Companion-Source": payload.source ?? "browser",
        "X-Companion-Sequence": String(sequence),
      },
      body,
    });
    return resp.ok;
  } catch {
    lastError = "HTTP bridge unreachable";
    notifyStatus();
    return false;
  }
}

export function pingBridge(): void {
  if (ws?.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: "ping", sequence: ++sequence }));
  }
}

export function isBridgeConnected(): boolean {
  return connected;
}

export function getSequence(): number {
  return sequence;
}
