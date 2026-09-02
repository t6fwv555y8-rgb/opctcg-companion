use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::confidence::ConfidenceConfig;
use crate::error::{ObsResult, ObservationError};
use crate::process_detect::{detect_simulator_processes, DetectedApplication};
use crate::types::{ObservationEnvelope, ObservationEvent, ObservationSource};
use crate::window_source::{WindowSource, WindowSourceConfig};
use async_trait::async_trait;
use chrono::Utc;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Desktop simulator adapter with prioritized observation backends.
pub struct DesktopSimulatorAdapter {
    log_watch_path: PathBuf,
    status: Arc<Mutex<AdapterStatus>>,
    detected: Arc<Mutex<Vec<DetectedApplication>>>,
    window_source: Arc<Mutex<WindowSource>>,
    shutdown: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl DesktopSimulatorAdapter {
    pub fn new(log_watch_path: PathBuf) -> Self {
        Self {
            log_watch_path,
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            detected: Arc::new(Mutex::new(Vec::new())),
            window_source: Arc::new(Mutex::new(WindowSource::new(WindowSourceConfig {
                window_title_hint: Some("simulator".into()),
                process_name_hint: Some("optcg".into()),
            }))),
            shutdown: Arc::new(Mutex::new(None)),
        }
    }

    fn envelope(raw: String, seq: u64) -> ObservationEnvelope {
        ObservationEnvelope {
            sequence: seq,
            timestamp_ms: Utc::now().timestamp_millis(),
            source: ObservationSource::DesktopSimulator,
            event: ObservationEvent::StructuredRaw {
                raw,
                source: ObservationSource::DesktopSimulator,
                confidence: ConfidenceConfig::for_source(ObservationSource::DesktopSimulator),
            },
        }
    }
}

#[async_trait]
impl ObservationAdapter for DesktopSimulatorAdapter {
    fn source(&self) -> ObservationSource {
        ObservationSource::DesktopSimulator
    }

    fn status(&self) -> AdapterStatus {
        *self.status.lock()
    }

    async fn detect(&self) -> ObsResult<bool> {
        *self.status.lock() = AdapterStatus::Detecting;
        let apps = detect_simulator_processes();
        let window_ok = self.window_source.lock().detect();
        *self.detected.lock() = apps.clone();

        let found = !apps.is_empty() || window_ok;
        *self.status.lock() = if found {
            AdapterStatus::Connected
        } else {
            AdapterStatus::Unavailable
        };
        Ok(found)
    }

    async fn start(&self, sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        if !self.detect().await? {
            return Err(ObservationError::Unavailable(
                "no desktop simulator detected".into(),
            ));
        }

        if !self.log_watch_path.exists() {
            std::fs::create_dir_all(&self.log_watch_path)?;
        }

        let (stop_tx, mut stop_rx) = mpsc::channel(1);
        *self.shutdown.lock() = Some(stop_tx);
        *self.status.lock() = AdapterStatus::Observing;

        let watch_path = self.log_watch_path.clone();
        let status = Arc::clone(&self.status);

        std::thread::spawn(move || {
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();
            let mut watcher = RecommendedWatcher::new(
                move |res| {
                    let _ = notify_tx.send(res);
                },
                Config::default().with_poll_interval(Duration::from_millis(200)),
            )
            .expect("watcher");

            if watcher
                .watch(&watch_path, RecursiveMode::Recursive)
                .is_err()
            {
                *status.lock() = AdapterStatus::Degraded;
                return;
            }

            info!(path = %watch_path.display(), "desktop adapter watching logs");
            let mut seq = 0u64;

            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                while let Ok(Ok(event)) = notify_rx.try_recv() {
                    for path in event.paths {
                        if is_log_file(&path) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                if let Some(line) = content.lines().last() {
                                    if !line.trim().is_empty() {
                                        seq += 1;
                                        let envelope = DesktopSimulatorAdapter::envelope(
                                            line.trim().to_string(),
                                            seq,
                                        );
                                        let _ = sender.blocking_send(envelope);
                                    }
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
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

fn is_log_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e, "log" | "jsonl" | "txt"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn desktop_detect_and_watch() {
        let dir = tempdir().unwrap();
        let adapter = DesktopSimulatorAdapter::new(dir.path().to_path_buf());
        // May or may not detect process; should not error
        let _ = adapter.detect().await;
    }
}
