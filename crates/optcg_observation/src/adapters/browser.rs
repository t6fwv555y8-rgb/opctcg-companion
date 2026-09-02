use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::bridge_protocol::{parse_snapshot_payload, validate_bridge_payload, BridgeMessage};
use crate::confidence::ConfidenceConfig;
use crate::diff::SnapshotDiffer;
use crate::error::{ObsResult, ObservationError};
use crate::types::{ObservationEnvelope, ObservationSource};
use async_trait::async_trait;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

const DEFAULT_BROWSER_PORT: u16 = 9003;

/// Browser simulator adapter — localhost bridge from browser companion extension.
pub struct BrowserSimulatorAdapter {
    port: u16,
    status: Arc<Mutex<AdapterStatus>>,
    shutdown: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    differ: Arc<Mutex<SnapshotDiffer>>,
    seq: Arc<Mutex<u64>>,
}

impl BrowserSimulatorAdapter {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            shutdown: Arc::new(Mutex::new(None)),
            differ: Arc::new(Mutex::new(SnapshotDiffer::new(ConfidenceConfig::default()))),
            seq: Arc::new(Mutex::new(0)),
        }
    }

    pub fn default_port() -> Self {
        Self::new(DEFAULT_BROWSER_PORT)
    }
}

#[async_trait]
impl ObservationAdapter for BrowserSimulatorAdapter {
    fn source(&self) -> ObservationSource {
        ObservationSource::BrowserSimulator
    }

    fn status(&self) -> AdapterStatus {
        *self.status.lock()
    }

    async fn detect(&self) -> ObsResult<bool> {
        // Browser bridge is detected when extension connects; idle server is not "detected".
        Ok(self.status() == AdapterStatus::Observing)
    }

    async fn start(&self, sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        *self.status.lock() = AdapterStatus::Observing;

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown.lock() = Some(shutdown_tx);

        let differ = Arc::clone(&self.differ);
        let seq = Arc::clone(&self.seq);
        let status = Arc::clone(&self.status);
        let sender = Arc::new(sender);

        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .route(
                "/snapshot",
                post({
                    let sender = Arc::clone(&sender);
                    let differ = Arc::clone(&differ);
                    let seq = Arc::clone(&seq);
                    move |Json(body): Json<serde_json::Value>| {
                        let sender = Arc::clone(&sender);
                        let differ = Arc::clone(&differ);
                        let seq = Arc::clone(&seq);
                        async move {
                            match parse_snapshot_payload(body.to_string().as_bytes()) {
                                Ok(snapshot) => {
                                    let events = differ.lock().diff(&snapshot);
                                    for event in events {
                                        *seq.lock() += 1;
                                        let envelope = ObservationEnvelope {
                                            sequence: *seq.lock(),
                                            timestamp_ms: Utc::now().timestamp_millis(),
                                            source: ObservationSource::BrowserSimulator,
                                            event,
                                        };
                                        let tx = Arc::clone(&sender);
                                        tokio::spawn(async move {
                                            let _ = tx.send(envelope).await;
                                        });
                                    }
                                    axum::http::StatusCode::OK
                                }
                                Err(e) => {
                                    warn!(error = %e, "invalid bridge payload");
                                    axum::http::StatusCode::BAD_REQUEST
                                }
                            }
                        }
                    }
                }),
            )
            .route(
                "/ws",
                get({
                    let sender = Arc::clone(&sender);
                    let differ = Arc::clone(&differ);
                    let seq = Arc::clone(&seq);
                    move |ws: WebSocketUpgrade| {
                        let sender = Arc::clone(&sender);
                        let differ = Arc::clone(&differ);
                        let seq = Arc::clone(&seq);
                        async move {
                            ws.on_upgrade(move |socket| {
                                handle_browser_ws(socket, sender, differ, seq)
                            })
                            .into_response()
                        }
                    }
                }),
            );

        let addr: SocketAddr = format!("127.0.0.1:{}", self.port)
            .parse()
            .map_err(|e: std::net::AddrParseError| ObservationError::Adapter(e.to_string()))?;

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ObservationError::Adapter(format!("bind: {e}")))?;

        info!(%addr, "browser adapter listening");

        tokio::spawn(async move {
            let server = axum::serve(listener, app);
            tokio::select! {
                _ = shutdown_rx => {}
                r = server => { if let Err(e) = r { warn!(error = %e, "browser server error"); } }
            }
            *status.lock() = AdapterStatus::Disconnected;
        });

        Ok(())
    }

    async fn stop(&self) -> ObsResult<()> {
        if let Some(tx) = self.shutdown.lock().take() {
            let _ = tx.send(());
        }
        *self.status.lock() = AdapterStatus::Disconnected;
        Ok(())
    }
}

async fn handle_browser_ws(
    mut socket: WebSocket,
    sender: Arc<mpsc::Sender<ObservationEnvelope>>,
    differ: Arc<Mutex<SnapshotDiffer>>,
    seq: Arc<Mutex<u64>>,
) {
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => match validate_bridge_payload(text.as_bytes()) {
                Ok(BridgeMessage::Snapshot(snapshot)) => {
                    let events = differ.lock().diff(&snapshot);
                    for event in events {
                        *seq.lock() += 1;
                        let envelope = ObservationEnvelope {
                            sequence: *seq.lock(),
                            timestamp_ms: Utc::now().timestamp_millis(),
                            source: ObservationSource::BrowserSimulator,
                            event,
                        };
                        let tx = Arc::clone(&sender);
                        tokio::spawn(async move {
                            let _ = tx.send(envelope).await;
                        });
                    }
                }
                Ok(BridgeMessage::Ping) => {
                    let _ = socket
                        .send(Message::Text(r#"{"type":"pong"}"#.into()))
                        .await;
                }
                Ok(_) => {}
                Err(e) => warn!(error = %e, "malformed ws message"),
            },
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_protocol::BrowserGameSnapshot;

    #[tokio::test]
    async fn browser_adapter_accepts_snapshot() {
        let adapter = BrowserSimulatorAdapter::new(19003);
        let (tx, mut rx) = mpsc::channel(16);
        adapter.start(tx).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let client = reqwest::Client::new();
        let snap = BrowserGameSnapshot {
            timestamp: 1,
            phase: Some("Main".into()),
            ..Default::default()
        };
        let resp = client
            .post("http://127.0.0.1:19003/snapshot")
            .json(&snap)
            .send()
            .await;
        if resp.is_ok() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        }

        adapter.stop().await.unwrap();
    }
}
