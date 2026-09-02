use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::error::ObsResult;
use crate::types::{ObservationEnvelope, ObservationSource};
use crate::window_source::{WindowSource, WindowSourceConfig};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

/// Screen vision adapter stub — window capture deferred to future vision pipeline.
pub struct ScreenVisionAdapter {
    status: Arc<Mutex<AdapterStatus>>,
    window_source: Arc<Mutex<WindowSource>>,
    shutdown: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl ScreenVisionAdapter {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            window_source: Arc::new(Mutex::new(WindowSource::new(WindowSourceConfig {
                window_title_hint: Some("simulator".into()),
                process_name_hint: None,
            }))),
            shutdown: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for ScreenVisionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ObservationAdapter for ScreenVisionAdapter {
    fn source(&self) -> ObservationSource {
        ObservationSource::ScreenVision
    }

    fn status(&self) -> AdapterStatus {
        *self.status.lock()
    }

    async fn detect(&self) -> ObsResult<bool> {
        *self.status.lock() = AdapterStatus::Detecting;
        let ready = self.window_source.lock().detect();
        *self.status.lock() = if ready {
            AdapterStatus::Connected
        } else {
            AdapterStatus::Unavailable
        };
        Ok(ready)
    }

    async fn start(&self, _sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        if !self.detect().await? {
            return Err(crate::error::ObservationError::Unavailable(
                "no capturable window".into(),
            ));
        }
        *self.status.lock() = AdapterStatus::Degraded;
        info!("screen vision adapter active (metadata-only stub)");
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
