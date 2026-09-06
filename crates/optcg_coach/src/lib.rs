//! Streaming AI coach for the OPTCG Companion HUD.
//!
//! The agent answers questions about the live match. A turn runs in three
//! stages, each surfaced to the UI as it happens:
//!
//! 1. **Grounding** — [`grounding::build_context`] runs read-only analysis
//!    (board readout, phase guidance, combat math, ranked legal actions) and
//!    reports each step as a [`types::CoachEvent::ToolRun`].
//! 2. **Generation** — a [`provider::ChatProvider`] streams the answer as
//!    [`types::CoachEvent::TextDelta`] frames.
//! 3. **Completion** — the caller emits one [`types::CoachEvent::Done`].
//!
//! Transport is deliberately not part of this crate. Providers push frames into
//! an [`provider::EventSink`] callback, so the desktop app can forward them over
//! Tauri events while tests collect them in memory.
//!
//! The coach is read-only by construction: grounding can query game state and
//! the rules engine, and there is no path from a model response back into game
//! state or the filesystem.

pub mod auto;
pub mod grounding;
pub mod offline;
pub mod openai;
pub mod provider;
pub mod session;
pub mod sink;
pub mod types;

pub use auto::{AutoDecision, AutoTrigger, AutoTriggerConfig, AUTO_QUESTION};
pub use grounding::{
    build_context, estimate_counters, fingerprint, is_decision_point, ContextScope,
    CounterEstimate, DeckContext, GroundedContext, ListStanding, SYSTEM_PROMPT,
};
pub use offline::OfflineProvider;
pub use openai::{OpenAiConfig, OpenAiProvider, DEFAULT_BASE_URL, DEFAULT_MODEL};
pub use provider::{CancelReason, CancelToken, ChatProvider, CoachError, CoachResult, EventSink};
pub use session::{CoachSession, TurnKind, HISTORY_TURNS, MAX_MESSAGE_CHARS};
pub use sink::{CoalescingSink, FlushTicker, DEFAULT_FLUSH_INTERVAL_MS};
pub use types::{
    ChatMessage, ChatRole, CoachEvent, CoachStreamEvent, FinishReason, StateFingerprint, ToolRun,
    TurnSummary,
};

/// Pick a provider from the environment, falling back to the offline coach.
///
/// Returning the offline provider rather than an error means the HUD always has
/// a working coach; the UI shows which one is active.
pub fn provider_from_env() -> std::sync::Arc<dyn ChatProvider> {
    let config = OpenAiConfig::from_env();
    if config.is_configured() {
        match OpenAiProvider::new(config) {
            Ok(provider) => return std::sync::Arc::new(provider),
            Err(e) => tracing::warn!(error = %e, "falling back to the offline coach"),
        }
    }
    std::sync::Arc::new(OfflineProvider::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_offline_without_a_key() {
        // Env vars are process-wide; this asserts the default path only when the
        // host has no key configured, which is the case in CI.
        if OpenAiConfig::from_env().is_configured() {
            return;
        }
        let provider = provider_from_env();
        assert!(!provider.is_live());
        assert_eq!(provider.label(), "Offline coach");
    }
}
