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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplaySpeed {
    Half,
    Normal,
    Double,
    Maximum,
}

impl ReplaySpeed {
    pub fn delay_ms(&self) -> u64 {
        match self {
            Self::Half => 200,
            Self::Normal => 100,
            Self::Double => 50,
            Self::Maximum => 0,
        }
    }
}

/// Replays recorded observation sessions through the canonical pipeline.
pub struct ReplayAdapter {
    status: Arc<Mutex<AdapterStatus>>,
    envelopes: Arc<Mutex<Vec<ObservationEnvelope>>>,
    speed: Arc<Mutex<ReplaySpeed>>,
    shutdown: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl ReplayAdapter {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            envelopes: Arc::new(Mutex::new(Vec::new())),
            speed: Arc::new(Mutex::new(ReplaySpeed::Normal)),
            shutdown: Arc::new(Mutex::new(None)),
        }
    }

    pub fn load(&self, path: &PathBuf) -> ObsResult<()> {
        let lines = load_replay_lines(path)?;
        *self.envelopes.lock() = lines;
        *self.status.lock() = AdapterStatus::Connected;
        Ok(())
    }

    pub fn set_speed(&self, speed: ReplaySpeed) {
        *self.speed.lock() = speed;
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

        let speed = *self.speed.lock();
        let status = Arc::clone(&self.status);

        tokio::spawn(async move {
            info!(count = envelopes.len(), "replay adapter starting");
            for mut envelope in envelopes {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                envelope.timestamp_ms = Utc::now().timestamp_millis();
                envelope.source = ObservationSource::Replay;
                if sender.send(envelope).await.is_err() {
                    break;
                }
                if speed.delay_ms() > 0 {
                    tokio::time::sleep(Duration::from_millis(speed.delay_ms())).await;
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
}
