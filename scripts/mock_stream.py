#!/usr/bin/env python3
"""Event-driven local WebSocket simulator for OPTCG Companion development."""

from __future__ import annotations

import asyncio
import json
import random
import sys
from datetime import datetime, timezone

try:
    import websockets
except ImportError:
    print("Install websockets: pip install websockets", file=sys.stderr)
    sys.exit(1)

HOST = "127.0.0.1"
PORT = 9002

EVENT_SEQUENCE = [
    {"type": "PHASE_CHANGED", "phase": "Draw", "active_player": 0},
    {"type": "PHASE_CHANGED", "phase": "Don", "active_player": 0},
    {"type": "DON_ATTACHED", "player": 0, "card_id": "ST01-001", "amount": 1},
    {"type": "CARD_PLAYED", "player": 0, "card_id": "ST01-002", "zone": "character"},
    {"type": "PHASE_CHANGED", "phase": "Main", "active_player": 0},
    {"type": "COMBAT_DECLARED", "attacker": "ST01-002", "target": "leader", "target_player": 1},
    {"type": "BLOCKER_OFFERED", "player": 1, "blocker_id": "ST01-010"},
    {"type": "COMBAT_RESOLVED", "attacker": "ST01-002", "damage": 5000, "blocked": False},
    {"type": "PHASE_CHANGED", "phase": "End", "active_player": 0},
    {"type": "TURN_END", "next_player": 1},
]


def ts() -> str:
    return datetime.now(timezone.utc).isoformat()


async def stream_events() -> None:
    uri = f"ws://{HOST}:{PORT}"
    idx = 0
    while True:
        try:
            async with websockets.connect(uri) as ws:
                print(f"[mock_stream] Connected to {uri}")
                while True:
                    event = dict(EVENT_SEQUENCE[idx % len(EVENT_SEQUENCE)])
                    event["timestamp"] = ts()
                    payload = json.dumps(event)
                    await ws.send(payload)
                    print(f"[mock_stream] Sent: {payload}")
                    idx += 1
                    await asyncio.sleep(1.5)
        except (ConnectionRefusedError, OSError) as exc:
            print(f"[mock_stream] Waiting for server on {uri}: {exc}")
            await asyncio.sleep(2.0)
        except websockets.exceptions.ConnectionClosed:
            print("[mock_stream] Connection closed, reconnecting...")
            await asyncio.sleep(1.0)


async def run_server_and_client() -> None:
    """Standalone mode: also accept connections if no Tauri server is running."""

    async def handler(websocket: websockets.WebSocketServerProtocol) -> None:
        print(f"[mock_stream] Client connected: {websocket.remote_address}")
        idx = 0
        try:
            async for _ in websocket:
                pass
        except websockets.exceptions.ConnectionClosed:
            pass
        finally:
            idx_local = idx
            while websocket.open:
                event = dict(EVENT_SEQUENCE[idx_local % len(EVENT_SEQUENCE)])
                event["timestamp"] = ts()
                await websocket.send(json.dumps(event))
                idx_local += 1
                await asyncio.sleep(1.5)

    async def client_loop() -> None:
        await stream_events()

    server = await websockets.serve(
        lambda ws: broadcast_loop(ws),
        HOST,
        PORT,
        ping_interval=20,
        ping_timeout=20,
    )
    print(f"[mock_stream] Fallback server listening on ws://{HOST}:{PORT}")
    await asyncio.gather(server.wait_closed(), client_loop())


connected: set = set()


async def broadcast_loop(websocket: websockets.WebSocketServerProtocol) -> None:
    connected.add(websocket)
    print(f"[mock_stream] Client joined ({len(connected)} total)")
    try:
        async for _ in websocket:
            pass
    finally:
        connected.discard(websocket)


async def main() -> None:
    """Push events to existing server; if unavailable, start embedded broadcaster."""
    idx = 0
    server = await websockets.serve(
        broadcast_loop,
        HOST,
        PORT,
        ping_interval=20,
        ping_timeout=20,
    )
    print(f"[mock_stream] Broadcasting on ws://{HOST}:{PORT}")

    try:
        while True:
            event = dict(EVENT_SEQUENCE[idx % len(EVENT_SEQUENCE)])
            event["timestamp"] = ts()
            payload = json.dumps(event)
            if connected:
                await asyncio.gather(
                    *[ws.send(payload) for ws in list(connected)],
                    return_exceptions=True,
                )
                print(f"[mock_stream] Broadcast: {payload}")
            else:
                print(f"[mock_stream] No clients, queued: {event['type']}")
            idx += 1
            await asyncio.sleep(1.5)
    finally:
        server.close()
        await server.wait_closed()


if __name__ == "__main__":
    random.seed(42)
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n[mock_stream] Stopped.")
