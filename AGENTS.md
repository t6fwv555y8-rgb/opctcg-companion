# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- Add durable project-specific notes here as they are discovered through real work.

## Sharp edges

- **Never call `.lock()` twice on the same `parking_lot::Mutex` within one statement/struct-literal.** Rust extends a temporary's lifetime to the end of its enclosing statement, so two `session.lock()` calls in the same `SomeStruct { a: session.lock().x, b: session.lock().y }` keep the first `MutexGuard` alive while the second tries to lock the same (non-reentrant) mutex on the same thread — a permanent self-deadlock, not a panic. This exact bug froze `ObservationPipeline::run_worker` (`crates/optcg_observation/src/pipeline.rs`) after the first processed event, and — because whichever tokio worker thread happens to be holding the runtime's shared I/O/timer driver at the time of the deadlock stops that driver ticking process-wide — it could also hang unrelated I/O such as new WebSocket handshakes. Diagnose this class of bug with `sample <pid>` (macOS) looking for threads parked in `parking_lot::RawMutex::lock_slow`, not by guessing from symptoms. Lock once, copy out the fields you need, then build the struct from locals.
- A regression test for a deadlock that can freeze tokio's I/O/timer driver must bound itself from a plain OS thread (`std::thread::spawn` + `std::sync::mpsc::Receiver::recv_timeout`), not `tokio::time::timeout` — the in-runtime timeout can itself hang. See `pipeline::tests::pipeline_keeps_processing_after_reconnect` in `crates/optcg_observation/src/pipeline.rs` for the pattern.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
