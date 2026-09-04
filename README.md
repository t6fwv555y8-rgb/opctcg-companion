# OPTCG Companion

Real-time desktop companion HUD overlay for the One Piece Card Game (OPTCG) Simulator. Provides optimal strategy recommendations, live combat math, blocker warnings, and system connectivity monitoring with a sub-100ms polling budget.

## Milestone 2 — Real-Time State Pipeline

The companion now implements a full vertical slice:

```
mock_stream.py → ws://127.0.0.1:9002 → EventProcessor → GameEvent → GameState
    → CombatMath / StrategyEngine → Tauri emit → React HUD (push updates)
```

### Event formats

**Pipe-delimited (preferred):**
```
PHASE_CHANGED|MAIN
DON_ATTACHED|PLAYER_1|LEADER|1
ATTACK_DECLARED|PLAYER_1|ST01-002|LEADER|PLAYER_2|6000
LIFE_CHANGED|PLAYER_2|-1
```

**JSON (legacy + mock stream wrapper):**
```json
{"type": "PHASE_CHANGED", "raw": "PHASE_CHANGED|MAIN"}
```

### Mock stream options

```bash
python3 scripts/mock_stream.py                  # deterministic, 1s interval
python3 scripts/mock_stream.py --random         # randomized events
python3 scripts/mock_stream.py --interval 0.5   # faster stream
```

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

## Quick Start (MacBook)

GitHub is only the source host — opening the website is not the app.

See **[docs/MAC_RUN.md](docs/MAC_RUN.md)** for the full Mac walkthrough.

```bash
npm run install:all
cd browser-companion && npm install && npm run build && cd ..

# Real native HUD (required) — NOT npm run dev:ui
cd src-ui && npm run tauri:dev
```

You should get a desktop window titled **OPTCG Companion HUD**.  
A browser tab on `localhost:1420` alone means the frontend-only Vite server is running.

Optional mock stream (second terminal):

```bash
python3 scripts/mock_stream.py
```

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
