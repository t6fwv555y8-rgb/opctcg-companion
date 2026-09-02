use crate::adapter::{AdapterStatus, ObservationAdapter};
use crate::confidence::ConfidenceConfig;
use crate::error::{ObsResult, ObservationError};
use crate::types::{ObservationEnvelope, ObservationEvent, ObservationSource};
use async_trait::async_trait;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{info, warn};

const DEFAULT_MOCK_PORT: u16 = 9002;
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Mock adapter — wraps existing WebSocket mock-stream ingestion.
pub struct MockAdapter {
    port: u16,
    status: Arc<Mutex<AdapterStatus>>,
    shutdown: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl MockAdapter {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            status: Arc::new(Mutex::new(AdapterStatus::Unavailable)),
            shutdown: Arc::new(Mutex::new(None)),
        }
    }

    pub fn default_port() -> Self {
        Self::new(DEFAULT_MOCK_PORT)
    }

    fn wrap_raw(raw: String, seq: u64) -> ObservationEnvelope {
        ObservationEnvelope {
            sequence: seq,
            timestamp_ms: Utc::now().timestamp_millis(),
            source: ObservationSource::Mock,
            event: ObservationEvent::StructuredRaw {
                raw,
                source: ObservationSource::Mock,
                confidence: ConfidenceConfig::for_source(ObservationSource::Mock),
            },
        }
    }
}

#[async_trait]
impl ObservationAdapter for MockAdapter {
    fn source(&self) -> ObservationSource {
        ObservationSource::Mock
    }

    fn status(&self) -> AdapterStatus {
        *self.status.lock()
    }

    async fn detect(&self) -> ObsResult<bool> {
        Ok(true)
    }

    async fn start(&self, sender: mpsc::Sender<ObservationEnvelope>) -> ObsResult<()> {
        *self.status.lock() = AdapterStatus::Connected;
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        *self.shutdown.lock() = Some(stop_tx);

        let addr: SocketAddr = format!("127.0.0.1:{}", self.port)
            .parse()
            .map_err(|e: std::net::AddrParseError| ObservationError::Adapter(e.to_string()))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ObservationError::Adapter(format!("bind failed: {e}")))?;

        info!(%addr, "mock adapter listening");
        *self.status.lock() = AdapterStatus::Observing;

        let status = Arc::clone(&self.status);
        let seq = Arc::new(AtomicU64::new(0));

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, peer)) => {
                                info!(%peer, "mock adapter client connected");
                                let mut sender = sender.clone();
                                let seq = Arc::clone(&seq);
                                tokio::spawn(async move {
                                    if let Err(e) = handle_mock_client(stream, &mut sender, seq).await {
                                        warn!(error = %e, "mock client error");
                                    }
                                });
                            }
                            Err(e) => warn!(error = %e, "accept failed"),
                        }
                    }
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

async fn handle_mock_client(
    stream: tokio::net::TcpStream,
    sender: &mut mpsc::Sender<ObservationEnvelope>,
    seq: Arc<AtomicU64>,
) -> ObsResult<()> {
    let ws = accept_async(stream)
        .await
        .map_err(|e| ObservationError::Adapter(e.to_string()))?;
    let (mut write, mut read) = ws.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if text.len() > MAX_MESSAGE_SIZE {
                    let err = serde_json::json!({"status":"error","message":"payload too large"});
                    let _ = write.send(Message::Text(err.to_string())).await;
                    continue;
                }
                let raw = extract_raw_payload(&text);
                let next = seq.fetch_add(1, Ordering::SeqCst) + 1;
                let envelope = MockAdapter::wrap_raw(raw, next);
                if sender.send(envelope).await.is_err() {
                    break;
                }
                let ack = serde_json::json!({"status":"accepted","sequence":next});
                if write.send(Message::Text(ack.to_string())).await.is_err() {
                    break;
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    Ok(())
}

fn extract_raw_payload(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(raw) = v.get("raw").and_then(|r| r.as_str()) {
            return raw.to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_adapter_starts_and_accepts() {
        let adapter = MockAdapter::new(19002);
        let (tx, mut rx) = mpsc::channel(8);
        adapter.start(tx).await.unwrap();
        assert_eq!(adapter.status(), AdapterStatus::Observing);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (mut ws, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:19002")
            .await
            .expect("connect");
        ws.send(Message::Text("PHASE_CHANGED|MAIN".into()))
            .await
            .unwrap();

        let envelope = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        assert!(matches!(
            envelope.event,
            ObservationEvent::StructuredRaw { .. }
        ));

        adapter.stop().await.unwrap();
    }
}
