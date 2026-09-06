//! End-to-end tests for the streaming chat client against a local HTTP server.
//!
//! These cover what the unit tests on `SseParser` cannot: the real reqwest
//! request, chunked delivery over a socket, and cancellation mid-stream.

use optcg_coach::provider::test_support::recording_sink;
use optcg_coach::{
    CancelToken, ChatMessage, ChatProvider, CoachError, CoachEvent, OpenAiConfig, OpenAiProvider,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// What the fake model server should do with the one request it accepts.
enum Behaviour {
    /// Stream these words as separate SSE frames, then `[DONE]`.
    Stream(Vec<&'static str>),
    /// Reply with an HTTP error status and body.
    Error(u16, &'static str),
    /// Stream one frame, then stall so the client can cancel.
    Stall(&'static str),
}

/// Serve exactly one request, then shut down. Returns the base URL.
async fn spawn_server(behaviour: Behaviour) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();

        // Drain the request headers so the client's write completes.
        let mut buf = [0u8; 4096];
        let _ = socket.read(&mut buf).await;

        match behaviour {
            Behaviour::Error(status, body) => {
                let response = format!(
                    "HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            Behaviour::Stream(words) => {
                write_stream_headers(&mut socket).await;
                for word in words {
                    socket.write_all(frame(word).as_bytes()).await.unwrap();
                    socket.flush().await.unwrap();
                    // Force the deltas into separate reads on the client side.
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                socket.write_all(b"data: [DONE]\n\n").await.unwrap();
            }
            Behaviour::Stall(word) => {
                write_stream_headers(&mut socket).await;
                socket.write_all(frame(word).as_bytes()).await.unwrap();
                socket.flush().await.unwrap();
                // Keep the body open; the client should give up on its own.
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        }
        let _ = socket.shutdown().await;
    });

    format!("http://{addr}/v1")
}

async fn write_stream_headers(socket: &mut tokio::net::TcpStream) {
    socket
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
              Cache-Control: no-cache\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    socket.flush().await.unwrap();
}

fn frame(content: &str) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"choices": [{"delta": {"content": content}}]})
    )
}

fn provider(base_url: String) -> OpenAiProvider {
    OpenAiProvider::new(OpenAiConfig {
        base_url,
        model: "test-model".into(),
        api_key: "test-key".into(),
        request_timeout: Duration::from_secs(5),
        ..Default::default()
    })
    .unwrap()
}

fn question() -> Vec<ChatMessage> {
    vec![
        ChatMessage::system("briefing"),
        ChatMessage::user("what now?"),
    ]
}

#[tokio::test]
async fn streams_deltas_from_a_live_endpoint() {
    let base = spawn_server(Behaviour::Stream(vec!["Attack ", "the ", "leader."])).await;
    let (sink, recorder) = recording_sink();

    let answer = provider(base)
        .stream_chat(&question(), &sink, &CancelToken::new())
        .await
        .expect("stream should succeed");

    assert_eq!(answer, "Attack the leader.");
    assert_eq!(recorder.text(), answer, "deltas must rebuild the answer");

    let deltas: Vec<_> = recorder
        .events()
        .into_iter()
        .filter(|e| matches!(e, CoachEvent::TextDelta(_)))
        .collect();
    assert_eq!(
        deltas.len(),
        3,
        "each server frame should surface as its own delta: {deltas:?}"
    );
    assert!(
        recorder
            .events()
            .iter()
            .any(|e| matches!(e, CoachEvent::Status(s) if s.contains("test-model"))),
        "the provider should announce which model it is asking"
    );
}

#[tokio::test]
async fn reports_api_errors_with_status_and_body() {
    let base = spawn_server(Behaviour::Error(401, r#"{"error":"bad key"}"#)).await;
    let (sink, recorder) = recording_sink();

    let result = provider(base)
        .stream_chat(&question(), &sink, &CancelToken::new())
        .await;

    match result {
        Err(CoachError::Api { status, body }) => {
            assert_eq!(status, 401);
            assert!(body.contains("bad key"), "body was: {body}");
        }
        other => panic!("expected an API error, got {other:?}"),
    }
    assert!(recorder.text().is_empty(), "a failed call streams no text");
}

#[tokio::test]
async fn cancelling_mid_stream_stops_promptly() {
    let base = spawn_server(Behaviour::Stall("Attack ")).await;

    let cancel = CancelToken::new();
    // Cancel as soon as the first delta lands, mimicking the Stop button.
    let trigger = cancel.clone();
    let sink: optcg_coach::EventSink = Arc::new(move |event| {
        if matches!(event, CoachEvent::TextDelta(_)) {
            trigger.cancel();
        }
    });

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        provider(base).stream_chat(&question(), &sink, &cancel),
    )
    .await
    .expect("cancellation should return well before the server stops stalling");

    assert!(
        matches!(result, Err(CoachError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    // The server holds the body open, so returning quickly proves the read was
    // interrupted rather than waiting for the next chunk or the HTTP timeout.
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "Stop took {:?}; cancellation should interrupt the read",
        started.elapsed()
    );
}

#[tokio::test]
async fn an_empty_response_is_reported_rather_than_returned() {
    let base = spawn_server(Behaviour::Stream(vec![])).await;
    let (sink, _recorder) = recording_sink();

    let result = provider(base)
        .stream_chat(&question(), &sink, &CancelToken::new())
        .await;

    assert!(
        matches!(result, Err(CoachError::Decode(_))),
        "expected a decode error, got {result:?}"
    );
}
