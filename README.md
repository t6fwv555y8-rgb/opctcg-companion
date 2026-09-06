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
- **Deck Collection** — Save multiple decks, and choose per side whether a deck
  is read from play or taken from a saved list (see below)
- **Scouting** — Learns opponents' decks and pace across games (see below)
- **Ask the Coach** — Streaming chat grounded in the live board (see below)
- **Connectivity Bar** — WebSocket, file monitor, latency, event count
- **Click-Through Toggle** — OS-level transparent overlay mode

## Where each side's deck comes from

Both sides of the **Decks** panel have a source, and both start on **Read from
play**: the leader across the table plus whatever cards have been revealed.
Pasting a list is an option, not a prerequisite.

| Source | What the app knows |
| --- | --- |
| Read from play | The leader and the cards actually revealed. |
| Presumed | The leader matched exactly one saved list, so that list is used as a read. Badged `read` in amber. |
| Attached | A list you supplied for that side. Badged `list`. |

The paste form has a **mine / theirs** target, so recording an opponent's list
does not change what you are playing. Save their list once and the next game
against that leader starts already mapped, without you doing anything.

Two guards keep a presumption from becoming an invention. Two saved lists on one
leader leaves the side read from play rather than choosing between them, and a
list attached to your side is never presumed for theirs — sharing a leader in a
mirror match is no reason to hand the opponent your own fifty cards.

Provenance reaches the coach, which matters more than it sounds. A presumed list
is named as a read rather than a fact, the model is told not to claim they hold
a particular card, and guessed cards stay out of the revealed-card set that the
counter estimate reasons from. Even a list you supplied for the opponent is
framed as bounding what they *could* hold, never what they do.

## Scouting

You cannot see what an opponent is playing unless they hand you their list. But
they show you the part of it they had to play in order to win, one card at a
time, every game. The **Scouting** panel keeps those cards.

Recording is automatic and needs no interaction: every observed position is read
for anything it says about the opponent's deck. Over a few games this turns into
two readings, each carrying the number of games behind it:

- **Which cards they run.** Per card, the share of games it appeared in and the
  most copies seen at once. A card in four of five games is worth playing
  around; the same card seen once is not, and the panel draws the difference.
  The headline is deliberately modest — "18 of their 50 mapped".
- **How the deck plays.** Pace measured from first damage, board development
  and game length rather than guessed from the leader's name. Left unstated
  below three games, because one game is an opponent's draw and not a deck's
  character. Reliability reads `thin`, `fair`, or `solid`.

A few things deliberately do not count as evidence. An idle HUD sitting on a
default position is not a game anyone played, twenty state updates are not
twenty games, and a card that stayed in their deck all game is invisible — copy
counts are a floor, never a claim about the fifty. The ledger lives in
`scouting.json` in the app data directory; an unfinished game survives a
restart, and closing the window folds it in.

The coach receives this as inference and is told so: built from N earlier games,
drawn from cards they have played, explicitly not a list anyone confirmed, and
never grounds for saying a card is in their hand right now. Once you attach
their real list the report disappears, since an estimate of a deck held in full
is noise. Withholding decks with the sharing pills withholds this too.

## Ask the Coach

The **Ask the Coach** panel answers questions about the live match. Every turn
runs in three stages, all visible in the HUD as they happen:

1. **Grounding** — read-only analysis of the current board: board readout,
   opponent counter ceiling, phase guidance, combat math, and ranked legal
   actions. Each step appears as a chip under the conversation. Grounding runs
   before any text is generated, so the chips arrive first.
2. **Generation** — the answer streams in, labelled with the position it was
   read from.
3. **Completion** — the turn closes, or reports that it was stopped,
   interrupted, or failed.

The coach is read-only by construction: it can query game state and the rules
engine, and there is no path from a model response back into game state, the
filesystem, or the simulator.

The opponent's counter estimate is an **upper bound from cards they have
revealed** this match, combined with their hand size — never a claim about
their actual hand, which is hidden.

### Answers stop when the board moves

An answer describes one position. If the board changes materially while it is
streaming — life, DON, hand size, board contents, phase, or combat — the turn
is interrupted and the bubble is marked `Board changed`, rather than finishing
advice about a position that no longer exists.

"Materially" excludes ordinary event churn: latency samples, log lines, trash
counts, and observation ordering do not interrupt a turn. See
`grounding::fingerprint`.

### Choosing what gets shared

Every turn sends the live board and your deck list to whichever model is
configured — which, with an API key set, is a third-party service. The pills
above the chat input show what the next question will carry, and each one can
be switched off:

| Pill | Covers |
| --- | --- |
| `Board` | Live position, opponent counter estimate, phase guidance, combat math, ranked options |
| `Deck` | Your saved deck list, leader, and matchup plan |

Both are on by default, since board-aware coaching is the point of the app.
Withholding is per-question rather than permanent, and the scope is captured
when a turn starts, so toggling mid-answer cannot change what was already
sent.

Two consequences worth knowing:

- Whatever is withheld is **named in the briefing**, with an instruction not to
  guess. Without that the model fills the gap by inventing a board.
- An answer given without the board has no position, so it **cannot be
  interrupted** by a board change, and automatic reads are unavailable —
  they exist to answer board changes and have nothing to read.

### Reading the board unprompted

The `auto` toggle in the panel header lets the coach answer without being
asked. The same fingerprint that interrupts a stale answer decides when a new
position is worth reading, so advice appears on its own as the game moves.

It is **off by default**, because unprompted reads spend tokens nobody asked
to spend. Once on, a read fires when all of the following hold:

| Guard | Default | Why |
| --- | --- | --- |
| Settle window | 1.5s | A play sequence (character, DON, attack) is read once when it finishes, not three times half-finished. |
| Floor between reads | 8s | Backstop on token spend and panel churn. |
| Position not already read | — | An unchanged board is never asked about twice. |
| At a decision point | — | Your Main or Combat phase, or an attack resolving against you. Draw, DON, and End play themselves, so advice there is noise. |
| Nothing already streaming | — | Automatic reads never talk over a question you asked. |

Automatic answers are marked `auto` and stream into the same panel, so `Stop`
and the board-change interrupt work on them unchanged. Asking a question
supersedes a read in progress — you do not have to stop it first.

They are deliberately **left out of the conversation history**. Recording them
would evict your own questions from the capped history within a few game turns
and bias the model toward repeating its last answer. The consequence: asking
"why?" straight after an automatic read gives the model the current board but
not its own prior wording.

Tuning lives in `auto::AutoTriggerConfig`. The policy is a pure state machine
with the clock passed in (`auto::AutoTrigger`), so it is tested without
sleeping.

To try it against the mock source, pace the stream slower than the settle
window:

```bash
python3 scripts/mock_stream.py --interval 2
```

At the default 1s interval **no automatic read ever fires**, because the next
position arrives before the previous one has settled. That is the intended
behaviour rather than a bug — a board still in motion is not one to give
advice about — but it does mean unbroken churn starves the trigger entirely.
Real play has natural pauses; a synthetic stream does not.

> **Why not a `MutationObserver` on the game log?** That layer already exists:
> `browser-companion` observes the simulator's DOM and feeds snapshots to the
> pipeline, which normalizes them into `GameState`. Triggering off the
> normalized state instead of scraped log text survives markup changes and
> gives the model the full board rather than the last 500 characters of a log.

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

Frames reach the HUD on the `coach://event` Tauri event channel:

```json
{"turn_id": 1, "type": "state_sync", "data": {"label": "turn 4 · Main · life 3-2", "digest": "..."}}
{"turn_id": 1, "type": "status",      "data": "Ranking legal actions"}
{"turn_id": 1, "type": "tool_run",    "data": {"tool": "counter_estimate", "summary": "≤8000 from 4 cards"}}
{"turn_id": 1, "type": "text_delta",  "data": "Attack the leader"}
{"turn_id": 1, "type": "done",        "data": {"reason": "complete", "text": "..."}}
```

`turn_id` lets the UI drop frames from a question the user has already replaced.
Exactly one `done` frame is emitted per turn.

A Tauri app has no HTTP origin to serve Server-Sent Events from, and `emit` is
already the app's backend-to-webview push channel — the same mechanism
`game-state-updated` uses. It fills the role SSE would in a browser deployment
without shipping a second process. SSE is still involved, just on the outbound
side: the client parses `text/event-stream` from the model API.

### Backpressure

A model emitting 50 tokens a second would otherwise mean 50 IPC messages and 50
React renders a second, competing with the HUD's own state updates.
`CoalescingSink` batches text on a 40ms cadence, which cuts that by roughly an
order of magnitude, and a flush ticker drains the tail if the model stalls
mid-answer. Any non-text frame flushes buffered text first, so `status`,
`tool_run`, and `done` can never overtake text produced before them.

On the frontend, message bubbles are memoized and completed messages keep their
object identity, so a streaming answer re-renders only its own bubble.

## License

MIT
