use crate::error::{EventsError, EventsResult};
use chrono::Utc;
use optcg_core::{CoreError, GameState, LastEventInfo, Normalizer};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Result of processing one ingested event through the pipeline.
#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub last_event: LastEventInfo,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// Inbound raw event message.
#[derive(Debug, Clone)]
pub struct InboundEvent {
    pub raw: String,
    pub source: EventSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSource {
    WebSocket,
    FileMonitor,
    Manual,
}

/// Async event processor: channel → parse → normalize → state mutation.
pub struct EventProcessor {
    state: Arc<RwLock<GameState>>,
    inbox: mpsc::Sender<InboundEvent>,
}

impl EventProcessor {
    pub fn new(state: Arc<RwLock<GameState>>) -> (Self, mpsc::Receiver<ProcessResult>) {
        let (in_tx, mut in_rx) = mpsc::channel::<InboundEvent>(512);
        let (out_tx, out_rx) = mpsc::channel::<ProcessResult>(512);
        let shared = Arc::clone(&state);

        tokio::spawn(async move {
            while let Some(inbound) = in_rx.recv().await {
                let start = Instant::now();
                let result = {
                    let mut gs = shared.write();
                    match Normalizer::process_raw(&mut gs, &inbound.raw) {
                        Ok(info) => {
                            gs.connection.latency_ms = start.elapsed().as_millis() as u64;
                            if inbound.source == EventSource::WebSocket {
                                gs.connection.websocket_connected = true;
                                gs.connection.status = optcg_core::ConnectionStatus::Connected;
                            }
                            ProcessResult {
                                last_event: info,
                                latency_ms: gs.connection.latency_ms,
                                error: None,
                            }
                        }
                        Err(CoreError::DuplicateEvent(_)) => {
                            debug!("duplicate event ignored");
                            continue;
                        }
                        Err(e) => {
                            warn!(error = %e, "event processing failed");
                            gs.connection.last_error = Some(e.to_string());
                            gs.connection.status = optcg_core::ConnectionStatus::Error;
                            ProcessResult {
                                last_event: gs.last_event.clone().unwrap_or(LastEventInfo {
                                    sequence: gs.event_sequence,
                                    event_name: "ERROR".into(),
                                    summary: e.to_string(),
                                    processed_at: Utc::now(),
                                }),
                                latency_ms: start.elapsed().as_millis() as u64,
                                error: Some(e.to_string()),
                            }
                        }
                    }
                };

                if out_tx.send(result).await.is_err() {
                    break;
                }
            }
            info!("event processor stopped");
        });

        (
            Self {
                state,
                inbox: in_tx,
            },
            out_rx,
        )
    }

    pub async fn submit(&self, raw: String, source: EventSource) -> EventsResult<()> {
        self.inbox
            .send(InboundEvent { raw, source })
            .await
            .map_err(|e| EventsError::WebSocket(format!("inbox closed: {e}")))
    }

    pub fn submit_blocking(&self, raw: String, source: EventSource) -> EventsResult<()> {
        self.inbox
            .blocking_send(InboundEvent { raw, source })
            .map_err(|e| EventsError::WebSocket(format!("inbox closed: {e}")))
    }

    pub fn state(&self) -> Arc<RwLock<GameState>> {
        Arc::clone(&self.state)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn pipeline_processes_event_sequence() {
        let state = Arc::new(RwLock::new(GameState::new()));
        let (processor, mut results) = EventProcessor::new(Arc::clone(&state));

        processor
            .submit("PHASE_CHANGED|MAIN".into(), EventSource::WebSocket)
            .await
            .unwrap();
        processor
            .submit(
                "DON_ATTACHED|PLAYER_1|LEADER|1".into(),
                EventSource::WebSocket,
            )
            .await
            .unwrap();

        let r1 = results.recv().await.unwrap();
        assert!(r1.error.is_none());
        assert_eq!(r1.last_event.sequence, 1);

        let r2 = results.recv().await.unwrap();
        assert!(r2.error.is_none());
        assert_eq!(r2.last_event.sequence, 2);

        let gs = state.read();
        assert_eq!(gs.event_sequence, 2);
    }
}
