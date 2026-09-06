# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- Add durable project-specific notes here as they are discovered through real work.

## Sharp edges

- **Never call `.lock()` twice on the same `parking_lot::Mutex` within one statement/struct-literal.** Rust extends a temporary's lifetime to the end of its enclosing statement, so two `session.lock()` calls in the same `SomeStruct { a: session.lock().x, b: session.lock().y }` keep the first `MutexGuard` alive while the second tries to lock the same (non-reentrant) mutex on the same thread — a permanent self-deadlock, not a panic. This exact bug froze `ObservationPipeline::run_worker` (`crates/optcg_observation/src/pipeline.rs`) after the first processed event, and — because whichever tokio worker thread happens to be holding the runtime's shared I/O/timer driver at the time of the deadlock stops that driver ticking process-wide — it could also hang unrelated I/O such as new WebSocket handshakes. Diagnose this class of bug with `sample <pid>` (macOS) looking for threads parked in `parking_lot::RawMutex::lock_slow`, not by guessing from symptoms. Lock once, copy out the fields you need, then build the struct from locals.
- A regression test for a deadlock that can freeze tokio's I/O/timer driver must bound itself from a plain OS thread (`std::thread::spawn` + `std::sync::mpsc::Receiver::recv_timeout`), not `tokio::time::timeout` — the in-runtime timeout can itself hang. See `pipeline::tests::pipeline_keeps_processing_after_reconnect` in `crates/optcg_observation/src/pipeline.rs` for the pattern.

## Cloud / headless Linux dev environment

- Toolchain floor is higher than the README's "Rust 1.75+": the locked dependency tree pulls `icu_normalizer 2.3.0`, which needs **edition 2024 (Rust ≥ 1.85)**. On an older default toolchain the whole workspace fails to build with `feature 'edition2024' is required`; run `rustup default stable && rustup update stable`.
- Tauri v2 on Ubuntu 24.04 needs these system packages (no `webkit2gtk-4.0` on noble — use the `-4.1` stack): `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf build-essential curl wget file libssl-dev libxdo-dev`.
- The mock stream needs Python `websockets` (`apt-get install -y python3-websockets`).
- Run the native HUD headlessly (no physical display) under Xvfb; WebKit needs its GPU paths disabled or it renders blank/crashes:
  ```bash
  Xvfb :99 -screen 0 1280x1024x24 &
  export DISPLAY=:99 WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1
  OPTCG_SOURCE=mock npm --prefix src-ui run tauri:dev     # native window titled "OPTCG Companion HUD"
  python3 scripts/mock_stream.py                           # second shell: drives ws://127.0.0.1:9002
  ```
  Use `OPTCG_SOURCE=mock` in headless/cloud runs — the default (`onesimulator`, browser bridge on :9003) has no data without the extension. `mock_stream.py` is a client; start the HUD first.
- Fast end-to-end sanity without a GUI: `cargo test --workspace` (Rust) and `cd browser-companion && npm test` (extension extract/combat/session).

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
