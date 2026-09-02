#!/usr/bin/env python3
"""OpenCV screen capture fallback for OPTCG Companion vision pipeline."""

from __future__ import annotations

import argparse
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

try:
    import cv2
    import numpy as np
except ImportError:
    print("Install opencv-python and numpy: pip install opencv-python numpy", file=sys.stderr)
    sys.exit(1)

try:
    import websockets
    import asyncio
except ImportError:
    websockets = None  # type: ignore


DEFAULT_WS = "ws://127.0.0.1:9002"
DEFAULT_OUTPUT = Path("vision_events.jsonl")


def capture_region(x: int, y: int, w: int, h: int) -> np.ndarray | None:
    """Capture a screen region using OpenCV (requires mss or platform backend)."""
    try:
        import mss

        with mss.mss() as sct:
            monitor = {"top": y, "left": x, "width": w, "height": h}
            shot = sct.grab(monitor)
            frame = np.array(shot)
            return cv2.cvtColor(frame, cv2.COLOR_BGRA2BGR)
    except ImportError:
        cap = cv2.VideoCapture(0)
        if not cap.isOpened():
            return None
        ret, frame = cap.read()
        cap.release()
        return frame if ret else None


def detect_phase_hint(frame: np.ndarray) -> str:
    """Heuristic phase detection from dominant hue regions."""
    hsv = cv2.cvtColor(frame, cv2.COLOR_BGR2HSV)
    mean_hue = float(np.mean(hsv[:, :, 0]))
    if mean_hue < 30:
        return "Main"
    if mean_hue < 90:
        return "Don"
    if mean_hue < 150:
        return "Combat"
    return "Draw"


def build_event(phase: str) -> dict:
    return {
        "type": "VISION_PHASE_HINT",
        "phase": phase,
        "source": "vision_pipeline",
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }


async def push_ws(uri: str, event: dict) -> None:
    if websockets is None:
        return
    try:
        async with websockets.connect(uri) as ws:
            await ws.send(json.dumps(event))
    except Exception as exc:
        print(f"[vision] WS push failed: {exc}")


def main() -> None:
    parser = argparse.ArgumentParser(description="OPTCG vision capture fallback")
    parser.add_argument("--x", type=int, default=0)
    parser.add_argument("--y", type=int, default=0)
    parser.add_argument("--width", type=int, default=800)
    parser.add_argument("--height", type=int, default=600)
    parser.add_argument("--interval", type=float, default=0.5)
    parser.add_argument("--ws", type=str, default=DEFAULT_WS)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()

    print(f"[vision] Capturing region ({args.x},{args.y}) {args.width}x{args.height}")
    print(f"[vision] Output: {args.output}, WS: {args.ws}")

    args.output.parent.mkdir(parents=True, exist_ok=True)

    with args.output.open("a", encoding="utf-8") as out:
        while True:
            frame = capture_region(args.x, args.y, args.width, args.height)
            if frame is None:
                print("[vision] Capture failed, retrying...")
                time.sleep(args.interval)
                continue

            phase = detect_phase_hint(frame)
            event = build_event(phase)
            line = json.dumps(event)
            out.write(line + "\n")
            out.flush()
            print(f"[vision] {line}")

            if websockets is not None:
                asyncio.run(push_ws(args.ws, event))

            time.sleep(args.interval)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n[vision] Stopped.")
