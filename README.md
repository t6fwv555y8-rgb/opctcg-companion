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
│   ├── optcg_coach/        # Streaming AI coach: grounding, providers, events
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
- **Deck Collection** — Save multiple decks, switch the active one mid-session
- **Ask the Coach** — Streaming chat grounded in the live board (see below)
- **Connectivity Bar** — WebSocket, file monitor, latency, event count
- **Click-Through Toggle** — OS-level transparent overlay mode

## Ask the Coach

The **Ask the Coach** panel answers questions about the live match. Every turn
runs in three stages, all visible in the HUD as they happen:

1. **Grounding** — read-only analysis of the current board: board readout,
   phase guidance, combat math, and ranked legal actions. Each step appears as
   a chip under the conversation.
2. **Generation** — the answer streams in token by token.
3. **Completion** — the turn closes, or reports that it was stopped or failed.

The coach is read-only by construction: it can query game state and the rules
engine, and there is no path from a model response back into game state, the
filesystem, or the simulator.

### Configuration

Without an API key the panel still works: the **Offline coach** answers from
the rules engine and streams the same way, so nothing needs configuring to try
it. To get conversational answers, point it at a model:

```bash
export OPTCG_LLM_API_KEY=sk-...            # required for live answers
export OPTCG_LLM_MODEL=gpt-4o-mini         # optional, this is the default
export OPTCG_LLM_BASE_URL=https://api.openai.com/v1   # optional
```

`OPENAI_API_KEY`, `OPENAI_MODEL`, and `OPENAI_BASE_URL` are read as fallbacks.
Because the endpoint is configurable, any OpenAI-compatible API works — Azure
OpenAI, or a local runner such as Ollama or LM Studio:

```bash
export OPTCG_LLM_BASE_URL=http://localhost:11434/v1
export OPTCG_LLM_MODEL=llama3.1
export OPTCG_LLM_API_KEY=ollama            # local runners ignore the value
```

The badge in the panel header shows which provider is active. Note that a live
provider sends your board state and saved deck list to that endpoint.

### Streaming transport

Frames reach the HUD on the `coach-chat-event` Tauri event channel as
`{"turn_id": 1, "type": "status" | "tool_run" | "text_delta" | "done", "data": ...}`.

A Tauri app has no HTTP origin to serve Server-Sent Events from, and `emit` is
already the app's backend-to-webview push channel — the same mechanism
`game-state-updated` uses. It fills the role SSE would in a browser deployment
without shipping a second process. SSE is still involved, just on the outbound
side: the client parses `text/event-stream` from the model API.

## License

MIT
