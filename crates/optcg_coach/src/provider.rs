use crate::types::{ChatMessage, CoachEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Callback the provider pushes stream frames into.
///
/// Deliberately synchronous: the desktop transport is `AppHandle::emit`, which
/// does not await, and this keeps providers free of channel plumbing.
pub type EventSink = Arc<dyn Fn(CoachEvent) + Send + Sync>;

/// Cooperative cancellation shared between a running turn and the UI.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoachError {
    #[error("coach is not configured: {0}")]
    NotConfigured(String),
    #[error("request failed: {0}")]
    Transport(String),
    #[error("model API returned {status}: {body}")]
    Api { status: u16, body: String },
    #[error("could not read the model response: {0}")]
    Decode(String),
    #[error("cancelled")]
    Cancelled,
}

pub type CoachResult<T> = Result<T, CoachError>;

/// A backend that can answer a chat turn, streaming text as it is produced.
#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    /// Short name for the HUD, e.g. `gpt-4o-mini` or `Offline coach`.
    fn label(&self) -> String;

    /// True when this provider talks to a real model API.
    fn is_live(&self) -> bool;

    /// Stream a reply for `messages`, pushing [`CoachEvent::TextDelta`] frames
    /// into `sink`, and return the complete text.
    ///
    /// Implementations must poll `cancel` between chunks and return
    /// [`CoachError::Cancelled`] promptly once it is set. Emitting the
    /// terminal `Done` frame is the caller's job, not the provider's.
    async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        sink: &EventSink,
        cancel: &CancelToken,
    ) -> CoachResult<String>;
}

/// Recording helpers for tests in this crate and its consumers.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    /// Thread-safe recorder so tests can assert on the frames a provider emits.
    #[derive(Clone, Default)]
    pub struct Recorder(Arc<Mutex<Vec<CoachEvent>>>);

    impl Recorder {
        pub fn events(&self) -> Vec<CoachEvent> {
            self.0.lock().expect("recorder poisoned").clone()
        }

        /// Concatenation of every `TextDelta` frame, i.e. what the UI renders.
        pub fn text(&self) -> String {
            self.events()
                .iter()
                .filter_map(|event| match event {
                    CoachEvent::TextDelta(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    pub fn recording_sink() -> (EventSink, Recorder) {
        let recorder = Recorder::default();
        let target = recorder.clone();
        let sink: EventSink = Arc::new(move |event| {
            target.0.lock().expect("recorder poisoned").push(event);
        });
        (sink, recorder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_token_is_shared_across_clones() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled(), "cancellation must be visible to holders");
    }
}
