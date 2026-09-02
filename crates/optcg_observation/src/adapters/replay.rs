use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::error::{ObsResult, ObservationError};
use crate::recording::load_replay_lines;
use crate::types::{ObservationEnvelope, ObservationSource};
use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

/// Playback speed for replay adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaySpeed {
    Half,
    Normal,
    Double,
    Maximum,
    Step,
}

impl ReplaySpeed {
    pub fn delay_ms(&self) -> u64 {
        match self {
            Self::Half => 200,
            Self::Normal => 100,
            Self::Double => 50,
            Self::Maximum => 0,
            Self::Step => u64::MAX,
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label.to_lowercase().as_str() {
            "half" | "0.5x" => Self::Half,
            "double" | "2x" => Self::Double,
            "maximum" | "max" => Self::Maximum,
            "step" => Self::Step,
            _ => Self::Normal,
        }
    }
}

/// Replays recorded observation sessions through the canonical pipeline.
pub struct ReplayAdapter {
    status: Arc<Mutex<AdapterStatus>>,
    envelopes: Arc<Mutex<Vec<ObservationEnvelope>>>,
    speed: Arc<Mutex<ReplaySpeed>>,
    index: Arc<Mutex<usize>>,
    step_signal: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    shutdown: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl ReplayAdapter {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            envelopes: Arc::new(Mutex::new(Vec::new())),
            speed: Arc::new(Mutex::new(ReplaySpeed::Normal)),
            index: Arc::new(Mutex::new(0)),
            step_signal: Arc::new(Mutex::new(None)),
            shutdown: Arc::new(Mutex::new(None)),
        }
    }

    pub fn load(&self, path: &PathBuf) -> ObsResult<()> {
        let lines = load_replay_lines(path)?;
        *self.envelopes.lock() = lines;
        *self.index.lock() = 0;
        *self.status.lock() = AdapterStatus::Connected;
        Ok(())
    }

    pub fn set_speed(&self, speed: ReplaySpeed) {
        *self.speed.lock() = speed;
    }

    pub fn step_forward(&self) -> bool {
        if let Some(tx) = self.step_signal.lock().take() {
            let _ = tx.send(());
            return true;
        }
        false
    }

    pub fn position(&self) -> usize {
        *self.index.lock()
    }

    pub fn total(&self) -> usize {
        self.envelopes.lock().len()
    }
}

impl Default for ReplayAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ObservationAdapter for ReplayAdapter {
    fn source(&self) -> ObservationSource {
        ObservationSource::Replay
    }

    fn status(&self) -> AdapterStatus {
        *self.status.lock()
    }

    async fn detect(&self) -> ObsResult<bool> {
        Ok(!self.envelopes.lock().is_empty())
    }

    async fn start(&self, sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        let envelopes = self.envelopes.lock().clone();
        if envelopes.is_empty() {
            return Err(ObservationError::Unavailable(
                "no replay data loaded".into(),
            ));
        }

        let (stop_tx, mut stop_rx) = mpsc::channel(1);
        *self.shutdown.lock() = Some(stop_tx);
        *self.status.lock() = AdapterStatus::Observing;
        *self.index.lock() = 0;

        let speed = Arc::clone(&self.speed);
        let status = Arc::clone(&self.status);
        let index = Arc::clone(&self.index);
        let step_signal = Arc::clone(&self.step_signal);

        tokio::spawn(async move {
            info!(count = envelopes.len(), "replay adapter starting");
            for (i, mut envelope) in envelopes.into_iter().enumerate() {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                *index.lock() = i;
                envelope.timestamp_ms = Utc::now().timestamp_millis();
                envelope.source = ObservationSource::Replay;
                if sender.send(envelope).await.is_err() {
                    break;
                }

                let current_speed = *speed.lock();
                let delay = current_speed.delay_ms();
                if delay == u64::MAX {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    *step_signal.lock() = Some(tx);
                    let _ = rx.await;
                } else if delay > 0 {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
            *status.lock() = AdapterStatus::Disconnected;
        });

        Ok(())
    }

    async fn stop(&self) -> ObsResult<()> {
        let stop_tx = self.shutdown.lock().take();
        if let Some(tx) = stop_tx {
            let _ = tx.send(()).await;
        }
        *self.status.lock() = AdapterStatus::Disconnected;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ObservationEvent;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_replay_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        let line = serde_json::json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "source": "mock",
            "observation": {
                "kind": "structured_raw",
                "raw": "PHASE_CHANGED|MAIN",
                "source": "mock",
                "confidence": 1.0
            },
            "confidence": 1.0
        });
        writeln!(file, "{}", line).unwrap();
        file
    }

    #[tokio::test]
    async fn replay_emits_observations() {
        let file = write_replay_file();
        let adapter = ReplayAdapter::new();
        adapter.load(&file.path().to_path_buf()).unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        adapter.start(tx).await.unwrap();

        let envelope = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("closed");
        assert!(matches!(
            envelope.event,
            ObservationEvent::StructuredRaw { .. }
        ));

        adapter.stop().await.unwrap();
    }

    #[test]
    fn step_mode_waits_for_signal() {
        let adapter = ReplayAdapter::new();
        adapter.set_speed(ReplaySpeed::Step);
        assert_eq!(ReplaySpeed::Step.delay_ms(), u64::MAX);
        assert!(!adapter.step_forward());
    }
}
