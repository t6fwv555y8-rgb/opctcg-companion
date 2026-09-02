use crate::error::{EventsError, EventsResult};
use crate::pipeline::{EventProcessor, EventSource, ProcessResult};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

/// WebSocket server configuration.
#[derive(Debug, Clone)]
pub struct WebSocketServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for WebSocketServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 9002,
        }
    }
}

impl WebSocketServerConfig {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Local async WebSocket server forwarding raw events into the pipeline.
pub struct WebSocketServer {
    config: WebSocketServerConfig,
    processor: Arc<EventProcessor>,
    on_connect: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl WebSocketServer {
    pub fn new(processor: Arc<EventProcessor>, config: WebSocketServerConfig) -> Self {
        Self {
            config,
            processor,
            on_connect: None,
        }
    }

    pub fn with_connect_hook(mut self, hook: Arc<dyn Fn(bool) + Send + Sync>) -> Self {
        self.on_connect = Some(hook);
        self
    }

    pub async fn run(self) -> EventsResult<()> {
        let addr: SocketAddr = self
            .config
            .addr()
            .parse()
            .map_err(|e| EventsError::WebSocket(format!("invalid addr: {e}")))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| EventsError::WebSocket(format!("bind failed: {e}")))?;

        info!(addr = %self.config.addr(), "websocket server listening");

        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(|e| EventsError::WebSocket(format!("accept failed: {e}")))?;

            info!(%peer, "websocket client connected");
            if let Some(hook) = &self.on_connect {
                hook(true);
            }

            let processor = Arc::clone(&self.processor);
            let hook = self.on_connect.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, processor, hook).await {
                    error!(error = %e, "connection handler error");
                }
            });
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    processor: Arc<EventProcessor>,
    hook: Option<Arc<dyn Fn(bool) + Send + Sync>>,
) -> EventsResult<()> {
    let ws_stream = accept_async(stream)
        .await
        .map_err(|e| EventsError::WebSocket(e.to_string()))?;

    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let start = std::time::Instant::now();
                let submit_result = processor.submit(text.clone(), EventSource::WebSocket).await;
                let elapsed = start.elapsed().as_millis() as u64;

                match submit_result {
                    Ok(()) => {
                        let ack = serde_json::json!({
                            "status": "accepted",
                            "latency_ms": elapsed
                        });
                        if write.send(Message::Text(ack.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "submit failed");
                        let err = serde_json::json!({"status": "error", "message": e.to_string()});
                        let _ = write.send(Message::Text(err.to_string())).await;
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    if let Some(hook) = hook {
        hook(false);
    }

    Ok(())
}

/// Spawn result fan-out task — useful for Tauri state broadcast hooks.
pub fn spawn_result_listener(
    mut rx: mpsc::Receiver<ProcessResult>,
    hook: impl Fn(ProcessResult) + Send + 'static,
) {
    tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            hook(result);
        }
    });
}

#[cfg(test)]
mod ws_integration {
    use super::*;
    use crate::pipeline::EventProcessor;
    use optcg_core::GameState;
    use parking_lot::RwLock;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn websocket_accepts_and_processes_event() {
        let state = Arc::new(RwLock::new(GameState::new()));
        let (processor, mut results) = EventProcessor::new(Arc::clone(&state));
        let processor = Arc::new(processor);

        let server = WebSocketServer::new(Arc::clone(&processor), WebSocketServerConfig::default());
        tokio::spawn(async move {
            let _ = server.run().await;
        });

        sleep(Duration::from_millis(200)).await;

        let uri = "ws://127.0.0.1:9002";
        let (mut ws, _) = tokio_tungstenite::connect_async(uri)
            .await
            .expect("connect to ws server");

        ws.send(Message::Text("PHASE_CHANGED|MAIN".into()))
            .await
            .unwrap();

        let result = tokio::time::timeout(Duration::from_secs(2), results.recv())
            .await
            .expect("timeout waiting for result")
            .expect("result channel closed");

        assert!(result.error.is_none());
        assert_eq!(result.last_event.event_name, "PHASE_CHANGED");

        let gs = state.read();
        assert_eq!(gs.phase, optcg_core::Phase::Main);
    }
}
