use crate::confidence::ConfidenceConfig;
use crate::error::ObsResult;
use crate::latency::{LatencySnapshot, LatencyTracker};
use crate::manager::{AdapterManager, SourceSelection};
use crate::reconciler::ObservationReconciler;
use crate::recording::ObservationRecorder;
use crate::session::GameSession;
use crate::types::{ObservationEnvelope, ObservationSource};
use chrono::Utc;
use optcg_core::{ConnectionStatus, GameState, LastEventInfo};
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Result emitted after processing one observation through the pipeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineResult {
    pub applied: bool,
    pub source: ObservationSource,
    pub observation_sequence: u64,
    pub event_sequence: u64,
    pub latency: LatencySnapshot,
    pub corrected: bool,
    pub error: Option<String>,
    pub last_event: Option<LastEventInfo>,
}

pub struct ObservationPipelineConfig {
    pub desktop_log_path: PathBuf,
    pub sessions_dir: PathBuf,
    pub mock_port: u16,
    pub browser_port: u16,
    pub recording_enabled: bool,
}

impl Default for ObservationPipelineConfig {
    fn default() -> Self {
        Self {
            desktop_log_path: PathBuf::from("logs"),
            sessions_dir: PathBuf::from("sessions"),
            mock_port: 9002,
            browser_port: 9003,
            recording_enabled: false,
        }
    }
}

/// End-to-end observation pipeline: adapters → reconciler → GameState → analysis hook.
pub struct ObservationPipeline {
    game_state: Arc<RwLock<GameState>>,
    manager: Arc<AdapterManager>,
    reconciler: Arc<Mutex<ObservationReconciler>>,
    session: Arc<Mutex<GameSession>>,
    latency: LatencyTracker,
    recorder: Arc<Mutex<ObservationRecorder>>,
    result_tx: Arc<Mutex<Option<mpsc::Sender<PipelineResult>>>>,
    worker: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl ObservationPipeline {
    pub fn new(game_state: Arc<RwLock<GameState>>, config: ObservationPipelineConfig) -> Self {
        let manager = Arc::new(
            AdapterManager::new(config.desktop_log_path)
                .with_mock_port(config.mock_port)
                .with_browser_port(config.browser_port),
        );

        let recorder = if config.recording_enabled {
            ObservationRecorder::open_dir(&config.sessions_dir)
                .unwrap_or_else(|_| ObservationRecorder::disabled())
        } else {
            ObservationRecorder::disabled()
        };

        Self {
            game_state,
            manager,
            reconciler: Arc::new(Mutex::new(ObservationReconciler::new(
                ConfidenceConfig::default(),
            ))),
            session: Arc::new(Mutex::new(GameSession::new(ObservationSource::Mock))),
            latency: LatencyTracker::new(),
            recorder: Arc::new(Mutex::new(recorder)),
            result_tx: Arc::new(Mutex::new(None)),
            worker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn manager(&self) -> Arc<AdapterManager> {
        Arc::clone(&self.manager)
    }

    pub fn latency(&self) -> LatencySnapshot {
        self.latency.snapshot()
    }

    pub fn sync_state(&self) -> crate::session::SyncState {
        self.session.lock().sync_state()
    }

    pub fn sync_status(&self) -> crate::sync_status::SyncStatus {
        let session = self.session.lock();
        crate::sync_status::SyncStatus::from_confidence(session.confidence, true)
    }

    pub fn analysis_eligibility(&self) -> crate::analysis::AnalysisEligibility {
        let session = self.session.lock();
        let gs = &session.state;
        crate::analysis::AnalysisEligibility::evaluate(
            session.confidence,
            true,
            gs.player_one().life > 0,
            gs.combat.active,
            true,
        )
    }

    pub fn validation_status(&self) -> Vec<crate::validation::AdapterValidationStatus> {
        crate::validation::all_adapter_validation()
    }

    pub fn set_selection(&self, selection: SourceSelection) {
        self.manager.set_selection(selection);
    }

    pub fn set_replay_path(&self, path: PathBuf) {
        self.manager.set_replay_path(path);
    }

    pub fn set_replay_speed(&self, label: &str) {
        use crate::adapters::replay::ReplaySpeed;
        self.manager
            .set_replay_speed(ReplaySpeed::from_label(label));
    }

    pub fn replay_step_forward(&self) -> bool {
        self.manager.replay_step_forward()
    }

    pub fn replay_position(&self) -> (usize, usize) {
        self.manager.replay_position()
    }

    pub async fn start(
        &self,
        selection: SourceSelection,
        result_tx: mpsc::Sender<PipelineResult>,
    ) -> ObsResult<()> {
        self.stop().await?;

        *self.result_tx.lock() = Some(result_tx);

        let (obs_tx, obs_rx) = mpsc::channel::<ObservationEnvelope>(512);
        self.manager.set_selection(selection.clone());
        self.manager.start(obs_tx).await?;

        let active = self
            .manager
            .active_source()
            .unwrap_or(ObservationSource::Mock);
        {
            let mut session = self.session.lock();
            session.reset_for_source(active);
        }

        self.sync_connection_status(active, AdapterStatus::Observing);

        let gs = Arc::clone(&self.game_state);
        let reconciler = Arc::clone(&self.reconciler);
        let session = Arc::clone(&self.session);
        let latency = self.latency.clone();
        let recorder = Arc::clone(&self.recorder);
        let result_tx_holder = Arc::clone(&self.result_tx);

        let handle = tokio::spawn(async move {
            Self::run_worker(
                obs_rx,
                gs,
                reconciler,
                session,
                latency,
                recorder,
                result_tx_holder,
            )
            .await;
        });

        *self.worker.lock() = Some(handle);
        info!(?selection, "observation pipeline started");
        Ok(())
    }

    pub async fn stop(&self) -> ObsResult<()> {
        self.manager.stop().await?;
        if let Some(handle) = self.worker.lock().take() {
            handle.abort();
        }
        *self.result_tx.lock() = None;
        Ok(())
    }

    async fn run_worker(
        mut obs_rx: mpsc::Receiver<ObservationEnvelope>,
        game_state: Arc<RwLock<GameState>>,
        reconciler: Arc<Mutex<ObservationReconciler>>,
        session: Arc<Mutex<GameSession>>,
        latency: LatencyTracker,
        recorder: Arc<Mutex<ObservationRecorder>>,
        result_tx_holder: Arc<Mutex<Option<mpsc::Sender<PipelineResult>>>>,
    ) {
        while let Some(envelope) = obs_rx.recv().await {
            let mut timer = latency.begin_observation();
            let start = Instant::now();

            if let Err(e) = recorder.lock().record(&envelope) {
                warn!(error = %e, "recording failed");
            }

            let outcome = {
                let mut session_guard = session.lock();
                if session_guard.source != envelope.source {
                    session_guard.reset_for_source(envelope.source);
                }
                if matches!(
                    envelope.event,
                    crate::types::ObservationEvent::GameDetected { .. }
                ) {
                    session_guard.reset_for_source(envelope.source);
                    session_guard.state.combat.reset();
                }
                reconciler
                    .lock()
                    .reconcile(&mut session_guard, &envelope.event)
            };

            timer.mark_analysis_start();

            let result = match outcome {
                Ok(outcome) => {
                    {
                        let session_guard = session.lock();
                        let mut gs = game_state.write();
                        *gs = session_guard.state.clone();
                        gs.connection.status = ConnectionStatus::Connected;
                        gs.connection.websocket_connected = envelope.source
                            == ObservationSource::Mock
                            || envelope.source == ObservationSource::BrowserSimulator;
                        gs.connection.file_monitor_active =
                            envelope.source == ObservationSource::DesktopSimulator;
                        gs.connection.latency_ms = start.elapsed().as_millis() as u64;
                        gs.connection.events_processed += 1;
                        gs.timestamp = Utc::now();
                    }

                    PipelineResult {
                        applied: outcome.applied,
                        source: envelope.source,
                        observation_sequence: session.lock().observation_sequence,
                        event_sequence: session.lock().event_sequence,
                        latency: latency.snapshot(),
                        corrected: outcome.corrected,
                        error: outcome.rejection_reason,
                        last_event: game_state.read().last_event.clone(),
                    }
                }
                Err(e) => {
                    warn!(error = %e, "reconcile error");
                    game_state.write().connection.last_error = Some(e.to_string());
                    PipelineResult {
                        applied: false,
                        source: envelope.source,
                        observation_sequence: session.lock().observation_sequence,
                        event_sequence: session.lock().event_sequence,
                        latency: latency.snapshot(),
                        corrected: false,
                        error: Some(e.to_string()),
                        last_event: None,
                    }
                }
            };

            timer.finish();

            if let Some(tx) = {
                let guard = result_tx_holder.lock();
                guard.clone()
            } {
                if tx.send(result).await.is_err() {
                    debug!("pipeline result channel closed");
                    break;
                }
            }
        }
    }

    fn sync_connection_status(
        &self,
        source: ObservationSource,
        status: crate::adapter::AdapterStatus,
    ) {
        let mut gs = self.game_state.write();
        gs.connection.status = match status {
            crate::adapter::AdapterStatus::Observing
            | crate::adapter::AdapterStatus::Connected
            | crate::adapter::AdapterStatus::Degraded => ConnectionStatus::Connected,
            crate::adapter::AdapterStatus::Detecting => ConnectionStatus::Connecting,
            crate::adapter::AdapterStatus::Error => ConnectionStatus::Error,
            _ => ConnectionStatus::Disconnected,
        };
        gs.connection.file_monitor_active = source == ObservationSource::DesktopSimulator;
    }
}

use crate::adapter::AdapterStatus;
