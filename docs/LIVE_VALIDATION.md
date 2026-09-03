# Live Validation Checklist

Use this guide to validate the OPTCG Companion on a real laptop. The cloud build environment cannot run OneSimulator in a browser extension or OPTCGSim.exe — only you can mark **Live Validation: PASS**.

| Platform | OneSimulator | OPTCGSim live capture |
|----------|--------------|------------------------|
| **macOS** | Supported (recommended) | Not implemented (Windows-only in M6) |
| **Windows** | Supported | Supported |

Branch: `cursor/milestone-6-live-validation-4c31`

---

## macOS (MacBook)

On Mac, **OneSimulator is the live path**. The Tauri HUD and Chrome/Edge extension are cross-platform. OPTCGSim window capture is Windows-only in Milestone 6 — selecting OPTCGSim on macOS will not produce live visual observation.

### Prerequisites

- macOS with Xcode Command Line Tools (`xcode-select --install`)
- Rust 1.75+ (`rustup`)
- Node.js 18+
- Chrome or Edge
- OneSimulator account

### Setup

```bash
git checkout cursor/milestone-6-live-validation-4c31
npm run install:all
cd browser-companion && npm install && npm run build
cd ..
```

### OneSimulator on Mac

1. Start the companion (first run may take several minutes to compile):
   ```bash
   cd src-ui && npm run tauri:dev
   ```
2. Load the browser extension:
   - Open Chrome/Edge → `chrome://extensions` (or Edge equivalent)
   - Enable **Developer mode**
   - **Load unpacked** → select `browser-companion/` (folder that contains `manifest.json`)
3. Open [OneSimulator](https://onesimulator.slidingcodes.com) and enter a match.
4. Extension badge should show **ON** when the bridge is connected.
5. In the HUD, select source **OneSimulator** (or **Auto**).
6. Confirm HUD shows **ONESIMULATOR · LIVE** (or SYNCING briefly, then LIVE).
7. Verify life, DON, phase, and board update within ~100ms of visible changes.
8. Enter combat — confirm attacker/target/power appear in combat panel.
9. Open the debug panel (shown in `tauri:dev` builds) — check:
   - Bridge status connected
   - Selector health: leader/life/don/board/combat not **missing** / **degraded**
   - Observation latency under 200ms typical
10. Refresh the tab — companion should reconnect without full restart.
11. Start a new match — old board/life/combat must not leak.

### Quick smoke test (no browser)

```bash
npm run dev
```

Starts Tauri + Python mock WebSocket on port **9002**. Select source **Mock** if Auto does not pick it up.

### Mac paths

| Data | Location |
|------|----------|
| Sessions / recordings | `~/Library/Application Support/optcg-companion/sessions/` |
| Calibration | `~/Library/Application Support/optcg-companion/calibration/` |
| Debug captures | `~/Library/Application Support/optcg-companion/debug-captures/` |

### Recording / debug on Mac

```bash
export OPTCG_RECORD_OBSERVATIONS=1
export OPTCG_DEBUG_CAPTURE=1
cd src-ui && npm run tauri:dev
```

### Mac troubleshooting

| Symptom | Check |
|---------|-------|
| HUD stuck on SEARCHING | Companion running before OneSimulator; port **9003** free; allow firewall for the app |
| Extension badge `!` | Companion not running, or rebuild + reload extension (`cd browser-companion && npm run build`) |
| No debug panel | Use `tauri:dev` (dev builds only) |
| OPTCGSim does nothing | Expected — live capture is Windows-only; use OneSimulator |

---

## Windows

### Prerequisites

- Windows 10/11
- OPTCG Companion built from `cursor/milestone-6-live-validation-4c31`
- OneSimulator account (browser) **and/or** OPTCGSim installed locally

### OneSimulator

1. Build the browser companion:
   ```bash
   cd browser-companion && npm install && npm run build
   ```
2. Load the unpacked extension from `browser-companion/` (folder containing `manifest.json`) in Chrome/Edge.
3. Start the Tauri companion (`cd src-ui && npm run tauri:dev` or installed build).
4. Open [OneSimulator](https://onesimulator.slidingcodes.com) and enter a match.
5. Confirm HUD shows **ONESIMULATOR · LIVE** (or SYNCING briefly, then LIVE).
6. Verify life, DON, phase, and board update within ~100ms of visible changes.
7. Enter combat — confirm attacker/target/power appear in combat panel.
8. Open debug panel (`tauri:dev` builds) — check:
   - Bridge status connected
   - Selector health: leader/life/don/board/combat not **degraded**
   - Observation latency under 200ms typical
9. Refresh the tab — companion should reconnect without full restart.
10. Start a new match — old board/life/combat must not leak.

### OneSimulator diagnostics (CLI)

```bash
cd browser-companion && npm test
```

All extract/combat/session tests should pass before live testing.

### OPTCGSim

1. Launch **OPTCGSim.exe** and start a match (windowed or maximized).
2. In companion, select source **OPTCGSim** (or Auto if sim is running).
3. Confirm debug panel shows:
   - Process detected
   - Window detected (title, size)
   - Capture working (capture FPS > 0)
   - Calibration loaded
4. If recognition is offset, open **Calibration** → adjust regions → **Save Calibration**.
5. Verify life/DON/board update during play.
6. Enter combat — confirm attack observation (attacker power → target).
7. Minimize window — HUD should show SYNCING/PARTIAL, not stale recommendations.
8. Restore window — should recover to LIVE within a few seconds.
9. Close and reopen OPTCGSim — companion should reconnect.

### OPTCGSim calibration

- Default normalized regions work for 16:9 at common resolutions.
- Custom profiles save to `%LOCALAPPDATA%\optcg-companion\calibration\`.
- Use **Reset Defaults** if layout breaks after a sim update.

### Windows paths

| Data | Location |
|------|----------|
| Sessions / recordings | `%LOCALAPPDATA%\optcg-companion\sessions\` |
| Calibration | `%LOCALAPPDATA%\optcg-companion\calibration\` |
| Debug captures | `%LOCALAPPDATA%\optcg-companion\debug-captures\` |

### Recording a Test Game (Windows)

```cmd
set OPTCG_RECORD_OBSERVATIONS=1
set OPTCG_DEBUG_CAPTURE=1
```

Use **Capture Debug Snapshot** in the debug panel.

---

## Reporting a Recognition Failure

1. Enable `OPTCG_DEBUG_CAPTURE=1` (Mac: `export …`; Windows: `set …`).
2. Reproduce the bad board state.
3. Click **Capture Debug Snapshot** (or use Tauri command `capture_debug_snapshot`).
4. Note: source, phase, what was wrong vs. visible game state.
5. Attach the JSON from the debug-captures directory (see platform paths above).
6. Optional: enable screenshots only in dev (`OPTCG_RECORD_SCREENSHOTS=1`) — captures game window only.

## Replaying a Recorded Session

1. In companion, select source **Replay**.
2. Provide path to `.v1.json` or legacy `.jsonl` session file.
3. Speeds: 0.5x, 1x, 2x, max, **step** (use step-forward for debugging).
4. Watch debug panel: observation sequence, sync status, reconciliation latency.
5. Confirm GameState matches what you remember from the live match at key moments.

## Validation Sign-off

After successful live testing, set in code or report:

| Adapter       | Implementation | Fixture Tests | Live Validation |
|---------------|----------------|---------------|-----------------|
| OneSimulator  | COMPLETE       | PASS          | PASS (you)      |
| OPTCGSim      | COMPLETE       | PASS          | PASS (you) — Windows only |

Until then, status remains **Live Validation: REQUIRED**.

## Common Issues

| Symptom | Check |
|---------|-------|
| HUD stuck on SEARCHING | Bridge port 9003, extension loaded, firewall |
| PARTIAL STATE forever | Tab hidden, window minimized, low capture confidence |
| Wrong card IDs | Run calibration; verify StreamingAssets card index built |
| Combat power missing | OneSimulator selector health; OPTCGSim combat_area region |
| Stale recommendations | Analysis paused banner should appear when sync degraded |
| OPTCGSim silent on Mac | Expected — use OneSimulator for Mac live validation |
