//! OPTCGSim desktop adapter — installation discovery, combat logs, visual fallback.

pub mod card_art;
pub mod config;
pub mod detector;
pub mod log_reader;
pub mod parser;
pub mod regions;
pub mod versions;
pub mod vision;

pub use config::{ObservationMode, OptcgSimConfig, OptcgSimStatus};
pub use detector::{discover_combat_logs, discover_installation, DetectedInstallation};
pub use log_reader::IncrementalLogReader;
pub use parser::OptcgSimLogParser;
pub use regions::{NormalizedRegion, RegionConfig};
pub use vision::{VisionObservation, VisionPipeline};

use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::confidence::ConfidenceConfig;
use crate::error::{ObsResult, ObservationError};
use crate::process_detect::{detect_optcgsim_processes, DetectedApplication};
use crate::types::{ObservationEnvelope, ObservationEvent, ObservationSource};
use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Dedicated OPTCGSim observation adapter.
pub struct OptcgSimAdapter {
    config: Arc<Mutex<OptcgSimConfig>>,
    status: Arc<Mutex<AdapterStatus>>,
    sim_status: Arc<Mutex<OptcgSimStatus>>,
    detected_app: Arc<Mutex<Option<DetectedApplication>>>,
    installation: Arc<Mutex<Option<detector::DetectedInstallation>>>,
    shutdown: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    seq: Arc<Mutex<u64>>,
}

impl OptcgSimAdapter {
    pub fn new(config: OptcgSimConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            sim_status: Arc::new(Mutex::new(OptcgSimStatus::default())),
            detected_app: Arc::new(Mutex::new(None)),
            installation: Arc::new(Mutex::new(None)),
            shutdown: Arc::new(Mutex::new(None)),
            seq: Arc::new(Mutex::new(0)),
        }
    }

    pub fn status_info(&self) -> OptcgSimStatus {
        self.sim_status.lock().clone()
    }

    pub fn config(&self) -> OptcgSimConfig {
        self.config.lock().clone()
    }

    fn next_envelope(&self, event: ObservationEvent) -> ObservationEnvelope {
        *self.seq.lock() += 1;
        ObservationEnvelope {
            sequence: *self.seq.lock(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: ObservationSource::DesktopSimulator,
            event,
        }
    }

    async fn probe(&self) -> ObsResult<()> {
        *self.status.lock() = AdapterStatus::Detecting;

        let apps = detect_optcgsim_processes();
        *self.detected_app.lock() = apps.first().cloned();

        let install = discover_installation(&self.config.lock());
        *self.installation.lock() = install.clone();

        let combat_logs = discover_combat_logs(&install);
        let mode = if combat_logs.live_capable {
            ObservationMode::StructuredLog
        } else if combat_logs.path.is_some() {
            ObservationMode::ReplayOnly
        } else if apps.first().is_some() {
            ObservationMode::VisualFallback
        } else {
            ObservationMode::Unavailable
        };

        *self.sim_status.lock() = OptcgSimStatus {
            process_detected: apps.first().is_some(),
            installation: install.clone(),
            combat_logs,
            mode: mode.clone(),
            label: mode.label(),
        };

        let found = apps.first().is_some() || install.is_some();
        *self.status.lock() = if found {
            AdapterStatus::Connected
        } else {
            AdapterStatus::Unavailable
        };
        Ok(())
    }
}

#[async_trait]
impl ObservationAdapter for OptcgSimAdapter {
    fn source(&self) -> ObservationSource {
        ObservationSource::DesktopSimulator
    }

    fn status(&self) -> AdapterStatus {
        *self.status.lock()
    }

    async fn detect(&self) -> ObsResult<bool> {
        self.probe().await?;
        Ok(self.status() != AdapterStatus::Unavailable)
    }

    async fn start(&self, sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        self.probe().await?;
        if self.status() == AdapterStatus::Unavailable {
            return Err(ObservationError::Unavailable(
                "OPTCGSim not detected".into(),
            ));
        }

        let mode = self.sim_status.lock().mode.clone();
        let (stop_tx, mut stop_rx) = mpsc::channel(1);
        *self.shutdown.lock() = Some(stop_tx);
        *self.status.lock() = AdapterStatus::Observing;

        let config = self.config.lock().clone();
        let sim_status = Arc::clone(&self.sim_status);
        let status = Arc::clone(&self.status);
        let seq = Arc::clone(&self.seq);

        match mode {
            ObservationMode::StructuredLog => {
                let log_path = sim_status
                    .lock()
                    .combat_logs
                    .path
                    .clone()
                    .ok_or_else(|| ObservationError::Unavailable("no log path".into()))?;
                let parser = OptcgSimLogParser::new();
                let mut reader = IncrementalLogReader::open(&log_path)?;
                info!(path = %log_path.display(), "OPTCGSim structured log watching");

                tokio::spawn(async move {
                    loop {
                        if stop_rx.try_recv().is_ok() {
                            break;
                        }
                        if let Ok(lines) = reader.read_new_lines() {
                            for line in lines {
                                if let Ok(events) = parser.parse_line(&line) {
                                    for event in events {
                                        *seq.lock() += 1;
                                        let envelope = ObservationEnvelope {
                                            sequence: *seq.lock(),
                                            timestamp_ms: Utc::now().timestamp_millis(),
                                            source: ObservationSource::DesktopSimulator,
                                            event,
                                        };
                                        if sender.send(envelope).await.is_err() {
                                            *status.lock() = AdapterStatus::Disconnected;
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    *status.lock() = AdapterStatus::Disconnected;
                });
            }
            ObservationMode::VisualFallback | ObservationMode::ReplayOnly => {
                let regions = config.vision_regions.clone();
                let mut vision = VisionPipeline::new(regions);
                if let Some(streaming) = sim_status
                    .lock()
                    .installation
                    .as_ref()
                    .and_then(|i| i.streaming_assets.clone())
                {
                    let index = card_art::CardArtIndex::build_from_streaming_assets(&streaming);
                    if index.len() > 0 {
                        vision = vision.with_card_index(index);
                    }
                }
                let vision = Arc::new(Mutex::new(vision));
                let vision_clone = Arc::clone(&vision);
                if mode == ObservationMode::ReplayOnly {
                    warn!("OPTCGSim CombatLogs are replay-only — using visual fallback for live if process running");
                }
                tokio::spawn(async move {
                    loop {
                        if stop_rx.try_recv().is_ok() {
                            break;
                        }
                        let events = if let Some(obs) = {
                            let obs = vision_clone.lock().capture_observation();
                            obs
                        } {
                            obs.to_observation_events()
                        } else {
                            Vec::new()
                        };
                        for event in events {
                            *seq.lock() += 1;
                            let envelope = ObservationEnvelope {
                                sequence: *seq.lock(),
                                timestamp_ms: Utc::now().timestamp_millis(),
                                source: ObservationSource::DesktopSimulator,
                                event,
                            };
                            if sender.send(envelope).await.is_err() {
                                *status.lock() = AdapterStatus::Disconnected;
                                return;
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(125)).await;
                    }
                    *status.lock() = AdapterStatus::Disconnected;
                });
            }
            ObservationMode::Unavailable => {
                return Err(ObservationError::Unavailable(
                    "no observation mode available".into(),
                ));
            }
        }

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
