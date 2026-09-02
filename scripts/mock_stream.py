#!/usr/bin/env python3
"""Event-driven local WebSocket simulator for OPTCG Companion development."""

from __future__ import annotations

import argparse
import asyncio
import json
import random
import signal
import sys
from datetime import datetime, timezone

try:
    import websockets
except ImportError:
    print("Install websockets: pip install websockets", file=sys.stderr)
    sys.exit(1)

HOST = "127.0.0.1"
PORT = 9002

# Realistic full-turn sequence (pipe-delimited, Milestone 2 format)
DETERMINISTIC_SEQUENCE = [
    "GAME_STARTED",
    "TURN_STARTED|PLAYER_1",
    "PHASE_CHANGED|DRAW",
    "PHASE_CHANGED|DON",
    "DON_ATTACHED|PLAYER_1|LEADER|1",
    "DON_ATTACHED|PLAYER_1|ST01-002|1",
    "PHASE_CHANGED|MAIN",
    "CARD_PLAYED|PLAYER_1|ST01-002|character",
    "ATTACK_DECLARED|PLAYER_1|ST01-002|LEADER|PLAYER_2|6000",
    "BLOCKER_OFFERED|PLAYER_2|ST01-010",
    "COMBAT_RESOLVED|5000|false",
    "LIFE_CHANGED|PLAYER_2|-1",
    "PHASE_CHANGED|END",
    "TURN_ENDED|PLAYER_2",
    "TURN_STARTED|PLAYER_2",
    "PHASE_CHANGED|DRAW",
    "PHASE_CHANGED|DON",
    "DON_ATTACHED|PLAYER_2|LEADER|2",
    "PHASE_CHANGED|MAIN",
    "DRAW_CARD|PLAYER_2|1",
]

RANDOM_EVENTS = [
    "PHASE_CHANGED|MAIN",
    "PHASE_CHANGED|DON",
    "DON_ATTACHED|PLAYER_1|LEADER|{n}",
    "CARD_PLAYED|PLAYER_1|ST01-003|character",
    "ATTACK_DECLARED|PLAYER_1|ST01-002|LEADER|PLAYER_2|{power}",
    "COMBAT_RESOLVED|{power}|false",
    "LIFE_CHANGED|PLAYER_2|-1",
    "TURN_ENDED|PLAYER_2",
]


def ts() -> str:
    return datetime.now(timezone.utc).isoformat()


def wrap_event(raw: str) -> str:
    """Send JSON envelope compatible with legacy parser."""
    kind = raw.split("|")[0]
    return json.dumps({"type": kind, "raw": raw, "timestamp": ts()})


class MockStream:
    def __init__(
        self,
        interval: float = 1.0,
        deterministic: bool = True,
        uri: str | None = None,
    ) -> None:
        self.interval = interval
        self.deterministic = deterministic
        self.uri = uri or f"ws://{HOST}:{PORT}"
        self._stop = asyncio.Event()
        self._idx = 0

    def stop(self) -> None:
        self._stop.set()

    def next_event(self) -> str:
        if self.deterministic:
            raw = DETERMINISTIC_SEQUENCE[self._idx % len(DETERMINISTIC_SEQUENCE)]
        else:
            template = random.choice(RANDOM_EVENTS)
            raw = template.format(n=random.randint(1, 2), power=random.choice([4000, 5000, 6000, 7000]))
        self._idx += 1
        return wrap_event(raw)

    async def run(self) -> None:
        print(f"[mock_stream] Target: {self.uri}")
        print(f"[mock_stream] Mode: {'deterministic' if self.deterministic else 'random'}")
        print(f"[mock_stream] Interval: {self.interval}s")

        while not self._stop.is_set():
            try:
                async with websockets.connect(self.uri, ping_interval=20, ping_timeout=20) as ws:
                    print(f"[mock_stream] Connected to {self.uri}")
                    while not self._stop.is_set():
                        payload = self.next_event()
                        await ws.send(payload)
                        print(f"[mock_stream] Sent: {payload}")
                        try:
                            ack = await asyncio.wait_for(ws.recv(), timeout=2.0)
                            print(f"[mock_stream] Ack: {ack}")
                        except asyncio.TimeoutError:
                            pass
                        await asyncio.sleep(self.interval)
            except (ConnectionRefusedError, OSError, ConnectionError) as exc:
                print(f"[mock_stream] Waiting for server on {self.uri}: {exc}")
                await asyncio.sleep(2.0)
            except websockets.exceptions.ConnectionClosed:
                print("[mock_stream] Connection closed, reconnecting...")
                await asyncio.sleep(1.0)


async def main_async(args: argparse.Namespace) -> None:
    stream = MockStream(
        interval=args.interval,
        deterministic=not args.random,
        uri=args.uri,
    )

    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stream.stop)

    await stream.run()


def main() -> None:
    parser = argparse.ArgumentParser(description="OPTCG mock event stream")
    parser.add_argument("--interval", type=float, default=1.0, help="Seconds between events")
    parser.add_argument("--random", action="store_true", help="Randomized event mode")
    parser.add_argument("--uri", type=str, default=None, help="WebSocket URI")
    args = parser.parse_args()

    random.seed(42)
    try:
        asyncio.run(main_async(args))
    except KeyboardInterrupt:
        print("\n[mock_stream] Stopped.")


if __name__ == "__main__":
    main()
