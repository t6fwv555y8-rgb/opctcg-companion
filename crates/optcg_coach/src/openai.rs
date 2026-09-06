use crate::provider::{CancelToken, ChatProvider, CoachError, CoachResult, EventSink};
use crate::types::{ChatMessage, CoachEvent};
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// Connection settings for any OpenAI-compatible `/chat/completions` endpoint.
///
/// `base_url` is configurable so the same client works against OpenAI, Azure
/// OpenAI, or a local runner such as Ollama or LM Studio.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub request_timeout: Duration,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            api_key: String::new(),
            temperature: 0.3,
            max_tokens: 700,
            request_timeout: Duration::from_secs(60),
        }
    }
}

impl OpenAiConfig {
    /// Read configuration from the environment.
    ///
    /// `OPTCG_LLM_*` wins over `OPENAI_*` so a user can point the HUD at a
    /// different model than the rest of their tooling.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Some(key) = first_env(&["OPTCG_LLM_API_KEY", "OPENAI_API_KEY"]) {
            config.api_key = key;
        }
        if let Some(url) = first_env(&["OPTCG_LLM_BASE_URL", "OPENAI_BASE_URL"]) {
            config.base_url = url.trim_end_matches('/').to_string();
        }
        if let Some(model) = first_env(&["OPTCG_LLM_MODEL", "OPENAI_MODEL"]) {
            config.model = model;
        }
        config
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

/// Streaming chat client for OpenAI-compatible APIs.
pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> CoachResult<Self> {
        if !config.is_configured() {
            return Err(CoachError::NotConfigured(
                "no API key; set OPTCG_LLM_API_KEY".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| CoachError::Transport(e.to_string()))?;
        Ok(Self { config, client })
    }

    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// Incremental parser for a `text/event-stream` body.
///
/// SSE frames are split across TCP reads at arbitrary byte offsets, so the
/// trailing partial line has to be carried into the next chunk.
#[derive(Default)]
pub struct SseParser {
    buffer: String,
    done: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once the server sent the `[DONE]` sentinel.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Feed raw bytes, returning any complete text deltas they contained.
    pub fn push(&mut self, bytes: &[u8]) -> CoachResult<Vec<String>> {
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        let mut deltas = Vec::new();

        while let Some(newline) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=newline).collect();
            if let Some(delta) = self.parse_line(line.trim_end_matches(['\r', '\n']))? {
                deltas.push(delta);
            }
        }
        Ok(deltas)
    }

    fn parse_line(&mut self, line: &str) -> CoachResult<Option<String>> {
        let line = line.trim();
        // Blank lines separate frames; `:` lines are server comments/keepalives.
        if line.is_empty() || line.starts_with(':') {
            return Ok(None);
        }
        let Some(payload) = line.strip_prefix("data:") else {
            return Ok(None);
        };
        let payload = payload.trim();
        if payload == "[DONE]" {
            self.done = true;
            return Ok(None);
        }

        // A malformed frame mid-stream should not discard the answer so far.
        match serde_json::from_str::<StreamChunk>(payload) {
            Ok(chunk) => Ok(chunk
                .choices
                .into_iter()
                .find_map(|choice| choice.delta.content)
                .filter(|text| !text.is_empty())),
            Err(e) => {
                tracing::debug!(error = %e, "skipping unparseable stream frame");
                Ok(None)
            }
        }
    }
}

#[async_trait::async_trait]
impl ChatProvider for OpenAiProvider {
    fn label(&self) -> String {
        self.config.model.clone()
    }

    fn is_live(&self) -> bool {
        true
    }

    async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        sink: &EventSink,
        cancel: &CancelToken,
    ) -> CoachResult<String> {
        let body = serde_json::json!({
            "model": self.config.model,
            "stream": true,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "messages": messages
                .iter()
                .map(|m| serde_json::json!({
                    "role": m.role.api_name(),
                    "content": m.content,
                }))
                .collect::<Vec<_>>(),
        });

        sink(CoachEvent::status(format!("Asking {}", self.config.model)));

        let response = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoachError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CoachError::Api {
                status: status.as_u16(),
                body: truncate(&body, 400),
            });
        }

        let mut parser = SseParser::new();
        let mut answer = String::new();
        let mut stream = response.bytes_stream();

        loop {
            // Race the read against cancellation: a stalled model must not make
            // the Stop button wait for the request timeout.
            let chunk = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(CoachError::Cancelled),
                next = stream.next() => match next {
                    Some(chunk) => chunk,
                    None => break,
                },
            };
            let chunk = chunk.map_err(|e| CoachError::Transport(e.to_string()))?;
            for delta in parser.push(&chunk)? {
                answer.push_str(&delta);
                sink(CoachEvent::TextDelta(delta));
            }
            if parser.is_done() {
                break;
            }
        }

        if answer.trim().is_empty() {
            return Err(CoachError::Decode(
                "model returned an empty response".into(),
            ));
        }
        Ok(answer)
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(content: &str) -> String {
        format!(
            "data: {}\n\n",
            serde_json::json!({"choices": [{"delta": {"content": content}}]})
        )
    }

    #[test]
    fn parses_whole_frames() {
        let mut parser = SseParser::new();
        let deltas = parser.push(frame("Attack ").as_bytes()).unwrap();
        assert_eq!(deltas, vec!["Attack "]);
        assert!(!parser.is_done());
    }

    #[test]
    fn reassembles_frames_split_across_reads() {
        let payload = format!("{}{}", frame("Hello "), frame("world"));
        let bytes = payload.as_bytes();

        // Split at every offset; the parser must never lose or duplicate text.
        for split in 1..bytes.len() {
            let mut parser = SseParser::new();
            let mut out = Vec::new();
            out.extend(parser.push(&bytes[..split]).unwrap());
            out.extend(parser.push(&bytes[split..]).unwrap());
            assert_eq!(
                out.concat(),
                "Hello world",
                "lost text when split at byte {split}"
            );
        }
    }

    #[test]
    fn one_byte_at_a_time_still_reassembles() {
        let payload = format!("{}{}", frame("abc"), frame("def"));
        let mut parser = SseParser::new();
        let mut out = Vec::new();
        for byte in payload.as_bytes() {
            out.extend(parser.push(&[*byte]).unwrap());
        }
        assert_eq!(out.concat(), "abcdef");
    }

    #[test]
    fn recognizes_the_done_sentinel() {
        let mut parser = SseParser::new();
        parser.push(b"data: [DONE]\n\n").unwrap();
        assert!(parser.is_done());
    }

    #[test]
    fn ignores_keepalives_blank_lines_and_junk() {
        let mut parser = SseParser::new();
        let deltas = parser
            .push(b": keepalive\n\n\nevent: ping\ndata: not-json\n\n")
            .unwrap();
        assert!(deltas.is_empty(), "unexpected deltas: {deltas:?}");
        assert!(!parser.is_done());

        // A junk frame must not poison the frames that follow it.
        let deltas = parser.push(frame("still here").as_bytes()).unwrap();
        assert_eq!(deltas, vec!["still here"]);
    }

    #[test]
    fn skips_empty_content_deltas() {
        let mut parser = SseParser::new();
        let deltas = parser
            .push(b"data: {\"choices\":[{\"delta\":{}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n")
            .unwrap();
        assert!(deltas.is_empty());
    }

    #[test]
    fn handles_crlf_line_endings() {
        let mut parser = SseParser::new();
        let deltas = parser
            .push(b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\r\n\r\n")
            .unwrap();
        assert_eq!(deltas, vec!["x"]);
    }

    #[test]
    fn requires_an_api_key() {
        match OpenAiProvider::new(OpenAiConfig::default()) {
            Err(CoachError::NotConfigured(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("a provider without an API key should be rejected"),
        }
    }

    #[test]
    fn endpoint_tolerates_a_trailing_slash() {
        let config = OpenAiConfig {
            base_url: "http://localhost:11434/v1/".into(),
            ..Default::default()
        };
        assert_eq!(
            config.endpoint(),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn truncate_keeps_short_text_intact() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdef", 3), "abc…");
    }
}
