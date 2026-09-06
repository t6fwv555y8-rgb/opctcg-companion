use serde::{Deserialize, Serialize};

/// Who authored a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    /// Role name expected by OpenAI-compatible chat completion APIs.
    pub fn api_name(self) -> &'static str {
        match self {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

/// A read-only analysis step the agent ran to ground its answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRun {
    /// Stable identifier, e.g. `board_readout` or `rank_actions`.
    pub tool: String,
    /// One-line human-readable result for the HUD.
    pub summary: String,
}

/// Why a turn stopped producing output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Complete,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSummary {
    pub reason: FinishReason,
    /// Authoritative full text when the turn produced one, letting the UI
    /// reconcile its incrementally built copy. `None` means keep what streamed,
    /// which is what a cancelled or failed turn wants.
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl TurnSummary {
    pub fn complete(text: impl Into<String>) -> Self {
        Self {
            reason: FinishReason::Complete,
            text: Some(text.into()),
            error: None,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            reason: FinishReason::Cancelled,
            text: None,
            error: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            reason: FinishReason::Failed,
            text: None,
            error: Some(error.into()),
        }
    }
}

/// One frame of the agent's output stream.
///
/// Serializes to the flat `{"type": ..., "data": ...}` envelope the HUD reads,
/// which keeps the wire format stable as variants are added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum CoachEvent {
    /// Progress line, e.g. "Reading board state".
    Status(String),
    /// A grounding tool finished.
    ToolRun(ToolRun),
    /// Next piece of assistant text. Append to whatever arrived before.
    TextDelta(String),
    /// Terminal frame for the turn. Exactly one is emitted per turn.
    Done(TurnSummary),
}

impl CoachEvent {
    pub fn status(text: impl Into<String>) -> Self {
        CoachEvent::Status(text.into())
    }

    pub fn tool(tool: impl Into<String>, summary: impl Into<String>) -> Self {
        CoachEvent::ToolRun(ToolRun {
            tool: tool.into(),
            summary: summary.into(),
        })
    }

    pub fn delta(text: impl Into<String>) -> Self {
        CoachEvent::TextDelta(text.into())
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, CoachEvent::Done(_))
    }
}

/// A [`CoachEvent`] tagged with the turn it belongs to.
///
/// The turn id lets the HUD drop frames from a turn the user already cancelled
/// or superseded, rather than interleaving them into the current answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoachStreamEvent {
    pub turn_id: u64,
    #[serde(flatten)]
    pub event: CoachEvent,
}

impl CoachStreamEvent {
    pub fn new(turn_id: u64, event: CoachEvent) -> Self {
        Self { turn_id, event }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_use_the_flat_type_data_envelope() {
        let json = serde_json::to_value(CoachEvent::status("Reading board state")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "status", "data": "Reading board state"})
        );

        let json = serde_json::to_value(CoachEvent::delta("Attack ")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "text_delta", "data": "Attack "})
        );

        let json = serde_json::to_value(CoachEvent::tool("rank_actions", "3 options")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "tool_run",
                "data": {"tool": "rank_actions", "summary": "3 options"}
            })
        );
    }

    #[test]
    fn stream_events_flatten_turn_id_alongside_type() {
        let json =
            serde_json::to_value(CoachStreamEvent::new(7, CoachEvent::delta("hi"))).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"turn_id": 7, "type": "text_delta", "data": "hi"})
        );
    }

    #[test]
    fn a_complete_turn_carries_authoritative_text() {
        let event = CoachEvent::Done(TurnSummary::complete("full answer"));
        assert!(event.is_terminal());

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "done");
        assert_eq!(json["data"]["reason"], "complete");
        assert_eq!(json["data"]["text"], "full answer");
        assert_eq!(json["data"]["error"], serde_json::Value::Null);
    }

    #[test]
    fn cancelled_and_failed_turns_leave_streamed_text_alone() {
        let cancelled = serde_json::to_value(CoachEvent::Done(TurnSummary::cancelled())).unwrap();
        assert_eq!(cancelled["data"]["reason"], "cancelled");
        assert_eq!(
            cancelled["data"]["text"],
            serde_json::Value::Null,
            "a cancelled turn must not overwrite what already streamed"
        );

        let failed = serde_json::to_value(CoachEvent::Done(TurnSummary::failed("boom"))).unwrap();
        assert_eq!(failed["data"]["reason"], "failed");
        assert_eq!(failed["data"]["error"], "boom");
        assert_eq!(failed["data"]["text"], serde_json::Value::Null);
    }

    #[test]
    fn events_round_trip() {
        let events = vec![
            CoachEvent::status("thinking"),
            CoachEvent::tool("board_readout", "life 4-3"),
            CoachEvent::delta("text"),
            CoachEvent::Done(TurnSummary::complete("text")),
            CoachEvent::Done(TurnSummary::cancelled()),
            CoachEvent::Done(TurnSummary::failed("nope")),
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            assert_eq!(serde_json::from_str::<CoachEvent>(&json).unwrap(), event);
        }
    }
}
