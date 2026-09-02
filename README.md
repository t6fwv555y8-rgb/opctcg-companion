# OPTCG Companion

Real-time desktop companion HUD overlay for the One Piece Card Game (OPTCG) Simulator. Provides optimal strategy recommendations, live combat math, blocker warnings, and system connectivity monitoring with a sub-100ms polling budget.

## Architecture

```
optcg-companion/
├── crates/
│   ├── optcg_core/       # GameState types & event normalizer
│   ├── optcg_database/   # SQLite card DB & JSON asset sync
│   ├── optcg_rules/        # Legal moves, combat math, beam search, MCTS
│   └── optcg_events/       # File monitor & WebSocket server (port 9002)
├── src-tauri/              # Tauri native backend & command handlers
├── src-ui/                 # React + TypeScript + Tailwind HUD
└── scripts/                # Python mock stream & vision fallback
```

## Prerequisites

- Rust 1.75+
- Node.js 18+
- Python 3.10+ (for mock stream / vision scripts)
- Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`

## Quick Start

```bash
# Install dependencies
npm run install:all

# Launch Tauri HUD + Python mock WebSocket streamer
npm run dev
```

The HUD polls game state every **100ms** via Tauri commands. The WebSocket server listens on `ws://127.0.0.1:9002` for simulator event injection.

## Individual Commands

| Command | Description |
|---------|-------------|
| `npm run dev` | Tauri app + mock stream concurrently |
| `npm run dev:ui` | Vite frontend only (port 1420) |
| `npm run dev:stream` | Python mock event broadcaster |
| `npm run vision` | OpenCV screen capture fallback |
| `npm run build` | Production Tauri build |

## Event Format

Inject events as JSON over WebSocket or write to monitored log files:

```json
{"type": "PHASE_CHANGED", "phase": "Main", "active_player": 0}
{"type": "COMBAT_DECLARED", "attacker": "ST01-002", "target": "leader", "target_player": 1}
{"type": "DON_ATTACHED", "player": 0, "card_id": "ST01-002", "amount": 1}
```

## HUD Features

- **Optimal Strategy** — Beam search sequencing + MCTS win-rate estimates
- **Combat Math** — Power differential, counter requirements, lethal detection
- **Blocker Warnings** — Real-time blocker availability and recommendations
- **Connectivity Bar** — WebSocket, file monitor, latency, event count
- **Click-Through Toggle** — OS-level transparent overlay mode

## License

MIT
