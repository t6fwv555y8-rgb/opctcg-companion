# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- Add durable project-specific notes here as they are discovered through real work.

## Build prerequisites

- The committed `Cargo.lock` needs a Rust newer than some preinstalled toolchains (a transitive `getrandom` requires edition 2024). If `cargo` fails resolving before compiling anything, install a current stable (`rustup toolchain install stable`) and use `cargo +stable`.
- Building or checking `src-tauri` on Linux needs GTK/WebKit and OpenSSL headers: `libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev`. Pass `-o Dpkg::Options::="--force-confold"` to `apt-get` to avoid an interactive `fuse.conf` prompt that silently hangs the install.
- `tauri::generate_context!` reads `frontendDist`, so `src-tauri` will not compile until the frontend exists: run `cd src-ui && npm install && npm run build` first. The pure-Rust crates (`cargo test -p optcg_rules`) need none of this.

## Sharp edges

- **Never call `.lock()` twice on the same `parking_lot::Mutex` within one statement/struct-literal.** Rust extends a temporary's lifetime to the end of its enclosing statement, so two `session.lock()` calls in the same `SomeStruct { a: session.lock().x, b: session.lock().y }` keep the first `MutexGuard` alive while the second tries to lock the same (non-reentrant) mutex on the same thread — a permanent self-deadlock, not a panic. This exact bug froze `ObservationPipeline::run_worker` (`crates/optcg_observation/src/pipeline.rs`) after the first processed event, and — because whichever tokio worker thread happens to be holding the runtime's shared I/O/timer driver at the time of the deadlock stops that driver ticking process-wide — it could also hang unrelated I/O such as new WebSocket handshakes. Diagnose this class of bug with `sample <pid>` (macOS) looking for threads parked in `parking_lot::RawMutex::lock_slow`, not by guessing from symptoms. Lock once, copy out the fields you need, then build the struct from locals.
- A regression test for a deadlock that can freeze tokio's I/O/timer driver must bound itself from a plain OS thread (`std::thread::spawn` + `std::sync::mpsc::Receiver::recv_timeout`), not `tokio::time::timeout` — the in-runtime timeout can itself hang. See `pipeline::tests::pipeline_keeps_processing_after_reconnect` in `crates/optcg_observation/src/pipeline.rs` for the pattern.

- **Fair play is a hard rule for anything that reads a simulator.** OPTCGSim's saved logs contain the opponent's hand and draws. Readers (`optcg_rules::optcgsim_log`, `optcg_observation::macsim`) must use only what both players can see plus your own cards, and must never store the opponent's player tag. Tests cover this; keep it covered.
- **OPTCGSim on macOS is read from its window** (`crates/optcg_observation/src/macsim`): the log panel by text recognition, cards by artwork hash against the simulator's own `StreamingAssets/Cards`. Screen positions are fractions measured on a ~2048x1015 window (`macsim/reader.rs`, `macsim/board.rs`); re-measure them from a real capture (`cargo run -p optcg_observation --example macsim_snapshot -- out.png`) when the simulator's layout changes. Needs the Screen Recording permission and Xcode command line tools (the OCR helper is compiled with `swiftc` on first use).
- **Image decoding is only fast because dev builds optimise it** (`[profile.dev.package.*]` in the root `Cargo.toml`). Hashing 2,800 cards took 7 minutes unoptimised and under 2 seconds with it; do not remove those entries.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
