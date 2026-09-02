# Live Validation Checklist

Use this guide to validate the OPTCG Companion on a real Windows laptop. The cloud build environment cannot run OneSimulator in a browser extension or OPTCGSim.exe — only you can mark **Live Validation: PASS**.

## Prerequisites

- Windows 10/11 laptop
- OPTCG Companion built from `cursor/milestone-6-live-validation-4c31`
- OneSimulator account (browser) **or** OPTCGSim installed locally

## OneSimulator

1. Build the browser companion:
   ```bash
   cd browser-companion && npm install && npm run build
   ```
2. Load the unpacked extension from `browser-companion/dist` in Chrome/Edge.
3. Start the Tauri companion (`cargo tauri dev` or installed build).
4. Open [OneSimulator](https://onesimulator.slidingcodes.com) and enter a match.
5. Confirm HUD shows **ONESIMULATOR · LIVE** (or SYNCING briefly, then LIVE).
6. Verify life, DON, phase, and board update within ~100ms of visible changes.
7. Enter combat — confirm attacker/target/power appear in combat panel.
8. Open debug panel (`DEBUG=1` dev build) — check:
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

## OPTCGSim

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

## Recording a Test Game

Enable recording:

```bash
set OPTCG_RECORD_OBSERVATIONS=1
```

Sessions save to `%LOCALAPPDATA%\optcg-companion\sessions\` as JSONL + v1 JSON.

For debug captures:

```bash
set OPTCG_DEBUG_CAPTURE=1
```

Use **Capture Debug Snapshot** in the debug panel.

## Reporting a Recognition Failure

1. Enable `OPTCG_DEBUG_CAPTURE=1`.
2. Reproduce the bad board state.
3. Click **Capture Debug Snapshot** (or use Tauri command `capture_debug_snapshot`).
4. Note: source, phase, what was wrong vs. visible game state.
5. Attach the JSON from `%LOCALAPPDATA%\optcg-companion\debug-captures\`.
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
| OPTCGSim      | COMPLETE       | PASS          | PASS (you)      |

Until then, status remains **Live Validation: REQUIRED**.

## Common Issues

| Symptom | Check |
|---------|-------|
| HUD stuck on SEARCHING | Bridge port 9003, extension loaded, firewall |
| PARTIAL STATE forever | Tab hidden, window minimized, low capture confidence |
| Wrong card IDs | Run calibration; verify StreamingAssets card index built |
| Combat power missing | OneSimulator selector health; OPTCGSim combat_area region |
| Stale recommendations | Analysis paused banner should appear when sync degraded |
