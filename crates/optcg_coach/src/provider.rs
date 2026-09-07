use crate::types::{ChatMessage, CoachEvent};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Callback the provider pushes stream frames into.
///
/// Deliberately synchronous: the desktop transport is `AppHandle::emit`, which
/// does not await, and this keeps providers free of channel plumbing.
pub type EventSink = Arc<dyn Fn(CoachEvent) + Send + Sync>;

/// Why a turn was cancelled, so the UI can explain itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    /// The user pressed Stop.
    User,
    /// The board moved away from the position the answer was grounded on.
    Stale,
}

const NOT_CANCELLED: u8 = 0;
const CANCELLED_BY_USER: u8 = 1;
const CANCELLED_AS_STALE: u8 = 2;

/// Cooperative cancellation shared between a running turn and the UI.
///
/// Exposes both a cheap synchronous check for tight loops and an awaitable
/// form, so a provider blocked on a socket read can be interrupted instead of
/// waiting for the next chunk that may never come.
#[derive(Debug, Clone)]
pub struct CancelToken {
    state: Arc<AtomicU8>,
    changed: Arc<tokio::sync::watch::Sender<bool>>,
}

impl Default for CancelToken {
    fn default() -> Self {
        let (changed, _) = tokio::sync::watch::channel(false);
        Self {
            state: Arc::new(AtomicU8::new(NOT_CANCELLED)),
            changed: Arc::new(changed),
        }
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel at the user's request.
    pub fn cancel(&self) {
        self.cancel_with(CancelReason::User);
    }

    pub fn cancel_with(&self, reason: CancelReason) {
        let encoded = match reason {
            CancelReason::User => CANCELLED_BY_USER,
            CancelReason::Stale => CANCELLED_AS_STALE,
        };
        // First reason wins, so a later cause cannot relabel why a turn ended.
        let _ =
            self.state
                .compare_exchange(NOT_CANCELLED, encoded, Ordering::SeqCst, Ordering::SeqCst);
        self.changed.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst) != NOT_CANCELLED
    }

    pub fn reason(&self) -> Option<CancelReason> {
        match self.state.load(Ordering::SeqCst) {
            CANCELLED_BY_USER => Some(CancelReason::User),
            CANCELLED_AS_STALE => Some(CancelReason::Stale),
            _ => None,
        }
    }

    /// Resolves once cancelled, including when cancellation already happened.
    pub async fn cancelled(&self) {
        let mut rx = self.changed.subscribe();
        // `subscribe` snapshots the current value, so checking it before
        // awaiting cannot miss a cancellation that races this call.
        if *rx.borrow_and_update() {
            return;
        }
        let _ = rx.changed().await;
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
        assert_eq!(clone.reason(), None);

        token.cancel();
        assert!(
            clone.is_cancelled(),
            "cancellation must be visible to holders"
        );
        assert_eq!(clone.reason(), Some(CancelReason::User));
    }

    #[test]
    fn the_first_cancellation_reason_wins() {
        let token = CancelToken::new();
        token.cancel_with(CancelReason::Stale);
        token.cancel_with(CancelReason::User);
        assert_eq!(
            token.reason(),
            Some(CancelReason::Stale),
            "a later cause must not relabel why the turn ended"
        );
    }

    #[tokio::test]
    async fn awaiting_a_token_cancelled_later_resolves() {
        let token = CancelToken::new();
        let trigger = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            trigger.cancel();
        });

        tokio::time::timeout(std::time::Duration::from_secs(2), token.cancelled())
            .await
            .expect("cancelled() should resolve once cancel() runs");
    }

    #[tokio::test]
    async fn awaiting_an_already_cancelled_token_returns_immediately() {
        let token = CancelToken::new();
        token.cancel();

        tokio::time::timeout(std::time::Duration::from_millis(50), token.cancelled())
            .await
            .expect("cancelled() must not block when cancellation already happened");
    }
}
