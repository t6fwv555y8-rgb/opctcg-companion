use crate::error::{EventsError, EventsResult};
use futures_util::{SinkExt, StreamExt};
use optcg_core::{Normalizer, RawEvent};
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
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

type SharedGameState = Arc<RwLock<optcg_core::GameState>>;

/// Local async WebSocket server for external stream injection.
pub struct WebSocketServer {
    config: WebSocketServerConfig,
    state: SharedGameState,
    event_tx: broadcast::Sender<String>,
}

impl WebSocketServer {
    pub fn new(state: SharedGameState, config: WebSocketServerConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            config,
            state,
            event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.event_tx.subscribe()
    }

    pub async fn run(&self) -> EventsResult<()> {
        let addr: SocketAddr = self
            .config
            .addr()
            .parse()
            .map_err(|e| EventsError::WebSocket(format!("invalid addr: {e}")))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| EventsError::WebSocket(format!("bind failed: {e}")))?;

        info!(addr = %self.config.addr(), "websocket server listening");

        {
            let mut state = self.state.write();
            state.connection.websocket_connected = false;
        }

        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(|e| EventsError::WebSocket(format!("accept failed: {e}")))?;

            info!(%peer, "websocket client connected");
            {
                let mut state = self.state.write();
                state.connection.websocket_connected = true;
            }

            let state = Arc::clone(&self.state);
            let event_tx = self.event_tx.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, state, event_tx).await {
                    error!(error = %e, "connection handler error");
                }
            });
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    state: SharedGameState,
    event_tx: broadcast::Sender<String>,
) -> EventsResult<()> {
    let ws_stream = accept_async(stream)
        .await
        .map_err(|e| EventsError::WebSocket(e.to_string()))?;

    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let start = std::time::Instant::now();
                let result = process_event(&state, &text);
                let elapsed = start.elapsed().as_millis() as u64;

                {
                    let mut gs = state.write();
                    gs.connection.latency_ms = elapsed;
                }

                match result {
                    Ok(()) => {
                        let _ = event_tx.send(text.clone());
                        let ack = serde_json::json!({"status": "ok", "latency_ms": elapsed});
                        if write
                            .send(Message::Text(ack.to_string()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "event processing failed");
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

    {
        let mut gs = state.write();
        gs.connection.websocket_connected = false;
    }

    Ok(())
}

fn process_event(state: &SharedGameState, text: &str) -> EventsResult<()> {
    let event: RawEvent = serde_json::from_str(text)?;
    let mut gs = state.write();
    Normalizer::apply_event(&mut gs, &event).map_err(EventsError::Core)?;
    Ok(())
}
