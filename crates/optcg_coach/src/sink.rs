use crate::provider::EventSink;
use crate::types::CoachEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Default flush cadence. Below the threshold where batching is visible as
/// stutter, while still collapsing a fast token stream into ~25 frames/sec.
pub const DEFAULT_FLUSH_INTERVAL_MS: u64 = 40;

/// Flush early once this much text is buffered, so a burst does not wait for
/// the timer and the buffer cannot grow without bound between ticks.
pub const DEFAULT_MAX_BUFFERED_CHARS: usize = 512;

/// Batches `TextDelta` frames before forwarding them downstream.
///
/// A model emitting 50 tokens a second would otherwise cause 50 IPC messages
/// and 50 React renders a second, which starves the UI thread during rapid
/// turns. Batching trades up to one flush interval of latency for roughly an
/// order of magnitude fewer frames.
///
/// Ordering is preserved: any non-text frame flushes the buffered text first,
/// so a `status`, `tool_run`, or `done` frame can never overtake text that was
/// produced before it.
pub struct CoalescingSink {
    inner: EventSink,
    buffer: Mutex<String>,
    max_buffered_chars: usize,
    /// Set once a terminal frame has been forwarded, so a late flush cannot
    /// emit text after `done`.
    closed: AtomicBool,
}

impl CoalescingSink {
    pub fn new(inner: EventSink) -> Self {
        Self::with_capacity(inner, DEFAULT_MAX_BUFFERED_CHARS)
    }

    pub fn with_capacity(inner: EventSink, max_buffered_chars: usize) -> Self {
        Self {
            inner,
            buffer: Mutex::new(String::new()),
            max_buffered_chars,
            closed: AtomicBool::new(false),
        }
    }

    /// Accept a frame, buffering text and forwarding everything else at once.
    pub fn push(&self, event: CoachEvent) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }

        match event {
            CoachEvent::TextDelta(text) => {
                // Take the buffered text out under the lock, then emit outside
                // it: `inner` is caller-supplied and must never run while this
                // sink holds its own lock.
                let ready = {
                    let mut buffer = self.buffer.lock().expect("sink buffer poisoned");
                    buffer.push_str(&text);
                    if buffer.len() >= self.max_buffered_chars {
                        Some(std::mem::take(&mut *buffer))
                    } else {
                        None
                    }
                };
                if let Some(text) = ready {
                    (self.inner)(CoachEvent::TextDelta(text));
                }
            }
            other => {
                self.flush();
                if other.is_terminal() {
                    self.closed.store(true, Ordering::SeqCst);
                }
                (self.inner)(other);
            }
        }
    }

    /// Forward any buffered text now. Safe to call when nothing is buffered.
    pub fn flush(&self) {
        let ready = {
            let mut buffer = self.buffer.lock().expect("sink buffer poisoned");
            if buffer.is_empty() {
                return;
            }
            std::mem::take(&mut *buffer)
        };
        (self.inner)(CoachEvent::TextDelta(ready));
    }

    /// True once a terminal frame has passed through.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// View this sink as an [`EventSink`] for a provider to write into.
    pub fn as_event_sink(self: &Arc<Self>) -> EventSink {
        let sink = Arc::clone(self);
        Arc::new(move |event| sink.push(event))
    }
}

/// Flushes a [`CoalescingSink`] on a fixed cadence until dropped.
///
/// Without this, a model that stalls mid-answer would leave its last partial
/// sentence in the buffer until the next frame arrived. The task also exits on
/// its own once the sink is closed, so a forgotten guard cannot leak it.
#[must_use = "dropping the guard stops the flush ticker"]
pub struct FlushTicker(tokio::task::JoinHandle<()>);

impl FlushTicker {
    /// Must be called from within a Tokio runtime.
    pub fn spawn(sink: Arc<CoalescingSink>, interval: std::time::Duration) -> Self {
        Self(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // The first tick resolves immediately; skip it.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if sink.is_closed() {
                    return;
                }
                sink.flush();
            }
        }))
    }
}

impl Drop for FlushTicker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::test_support::recording_sink;
    use crate::types::TurnSummary;
    use std::time::Duration;

    #[test]
    fn text_is_batched_until_flushed() {
        let (inner, recorder) = recording_sink();
        let sink = CoalescingSink::new(inner);

        sink.push(CoachEvent::delta("Attack "));
        sink.push(CoachEvent::delta("the "));
        sink.push(CoachEvent::delta("leader."));
        assert!(
            recorder.events().is_empty(),
            "text should still be buffered: {:?}",
            recorder.events()
        );

        sink.flush();
        assert_eq!(recorder.events().len(), 1, "one batched frame expected");
        assert_eq!(recorder.text(), "Attack the leader.");
    }

    #[test]
    fn a_large_burst_flushes_without_waiting() {
        let (inner, recorder) = recording_sink();
        let sink = CoalescingSink::with_capacity(inner, 8);

        sink.push(CoachEvent::delta("12345"));
        assert!(recorder.events().is_empty(), "under the threshold");

        sink.push(CoachEvent::delta("678"));
        assert_eq!(
            recorder.text(),
            "12345678",
            "reaching the threshold should flush"
        );
    }

    #[test]
    fn non_text_frames_flush_buffered_text_first() {
        let (inner, recorder) = recording_sink();
        let sink = CoalescingSink::new(inner);

        sink.push(CoachEvent::delta("before "));
        sink.push(CoachEvent::status("thinking"));
        sink.push(CoachEvent::delta("after"));
        sink.push(CoachEvent::tool("rank_actions", "3 options"));

        let events = recorder.events();
        assert_eq!(
            events,
            vec![
                CoachEvent::delta("before "),
                CoachEvent::status("thinking"),
                CoachEvent::delta("after"),
                CoachEvent::tool("rank_actions", "3 options"),
            ],
            "batching must not reorder text against other frames"
        );
    }

    #[test]
    fn the_terminal_frame_flushes_and_closes() {
        let (inner, recorder) = recording_sink();
        let sink = CoalescingSink::new(inner);

        sink.push(CoachEvent::delta("tail"));
        sink.push(CoachEvent::Done(TurnSummary::complete("tail")));

        let events = recorder.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], CoachEvent::delta("tail"), "tail text must escape");
        assert!(events[1].is_terminal());
        assert!(sink.is_closed());
    }

    #[test]
    fn nothing_escapes_after_the_terminal_frame() {
        let (inner, recorder) = recording_sink();
        let sink = CoalescingSink::new(inner);

        sink.push(CoachEvent::Done(TurnSummary::cancelled()));
        sink.push(CoachEvent::delta("late"));
        sink.flush();
        sink.push(CoachEvent::status("late status"));

        let events = recorder.events();
        assert_eq!(events.len(), 1, "only the terminal frame: {events:?}");
        assert!(events[0].is_terminal());
    }

    #[test]
    fn flushing_an_empty_buffer_emits_nothing() {
        let (inner, recorder) = recording_sink();
        let sink = CoalescingSink::new(inner);

        sink.flush();
        sink.flush();
        assert!(recorder.events().is_empty());
    }

    #[tokio::test]
    async fn the_ticker_flushes_a_stalled_tail() {
        let (inner, recorder) = recording_sink();
        let sink = Arc::new(CoalescingSink::new(inner));
        let _ticker = FlushTicker::spawn(Arc::clone(&sink), Duration::from_millis(10));

        // Buffer text and then send nothing more, mimicking a stalled model.
        sink.push(CoachEvent::delta("stalled tail"));
        assert!(recorder.events().is_empty(), "buffered, not yet flushed");

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(
            recorder.text(),
            "stalled tail",
            "the ticker should have flushed the tail without another frame"
        );
    }

    #[tokio::test]
    async fn dropping_the_guard_stops_the_ticker() {
        let (inner, recorder) = recording_sink();
        let sink = Arc::new(CoalescingSink::new(inner));

        let ticker = FlushTicker::spawn(Arc::clone(&sink), Duration::from_millis(5));
        drop(ticker);
        tokio::time::sleep(Duration::from_millis(20)).await;

        sink.push(CoachEvent::delta("orphaned"));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            recorder.events().is_empty(),
            "a stopped ticker must not keep flushing: {:?}",
            recorder.events()
        );
    }

    #[tokio::test]
    async fn an_event_sink_wrapper_batches_the_same_way() {
        let (inner, recorder) = recording_sink();
        let sink = Arc::new(CoalescingSink::new(inner));
        let event_sink = sink.as_event_sink();

        event_sink(CoachEvent::delta("a"));
        event_sink(CoachEvent::delta("b"));
        event_sink(CoachEvent::Done(TurnSummary::complete("ab")));

        assert_eq!(recorder.text(), "ab");
        assert_eq!(recorder.events().len(), 2);
    }
}
