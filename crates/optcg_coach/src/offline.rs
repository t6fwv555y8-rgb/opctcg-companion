use crate::provider::{CancelToken, ChatProvider, CoachError, CoachResult, EventSink};
use crate::types::{ChatMessage, ChatRole, CoachEvent};
use std::time::Duration;

/// Fallback coach used when no model API key is configured.
///
/// It answers from the grounded briefing alone — the same board readout, phase
/// guidance, and ranked options the rules engine already produces — and streams
/// it word by word so the HUD behaves identically with or without a key. This
/// keeps the feature usable offline and lets the whole streaming path be tested
/// without network access.
pub struct OfflineProvider {
    /// Delay between words. Zero in tests, small in the app for readability.
    word_delay: Duration,
}

impl Default for OfflineProvider {
    fn default() -> Self {
        Self {
            word_delay: Duration::from_millis(18),
        }
    }
}

impl OfflineProvider {
    pub fn new(word_delay: Duration) -> Self {
        Self { word_delay }
    }

    /// Construct a provider that streams with no artificial delay.
    pub fn instant() -> Self {
        Self::new(Duration::ZERO)
    }
}

#[async_trait::async_trait]
impl ChatProvider for OfflineProvider {
    fn label(&self) -> String {
        "Offline coach".to_string()
    }

    fn is_live(&self) -> bool {
        false
    }

    async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        sink: &EventSink,
        cancel: &CancelToken,
    ) -> CoachResult<String> {
        sink(CoachEvent::status("Answering from the rules engine"));

        let briefing = messages
            .iter()
            .find(|m| m.role == ChatRole::System)
            .map(|m| m.content.as_str())
            .unwrap_or_default();
        let question = messages
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or_default();

        let answer = compose_answer(briefing, question);
        let mut streamed = String::new();

        for (i, word) in answer.split_inclusive(' ').enumerate() {
            if cancel.is_cancelled() {
                return Err(CoachError::Cancelled);
            }
            // Yield on the first word too, so cancellation can win a race.
            if !self.word_delay.is_zero() && i > 0 {
                tokio::time::sleep(self.word_delay).await;
            }
            streamed.push_str(word);
            sink(CoachEvent::TextDelta(word.to_string()));
        }

        Ok(streamed)
    }
}

/// Assemble a reply from the briefing sections most relevant to the question.
fn compose_answer(briefing: &str, question: &str) -> String {
    let sections = parse_sections(briefing);
    let mut parts = Vec::new();

    if let Some(options) = sections.get("Ranked options") {
        if let Some(best) = options.lines().next() {
            let line =
                best.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
            parts.push(format!("Best line right now: {line}"));
        }
    }
    if let Some(combat) = sections.get("Combat math") {
        parts.push(combat.trim().to_string());
    }
    if let Some(phase) = sections.get("Phase guidance") {
        parts.push(phase.trim().to_string());
    }

    // Deck plan is only worth spending words on for strategy-shaped questions.
    if mentions_deck_strategy(question) {
        if let Some(decks) = sections.get("Decks") {
            parts.push(decks.trim().to_string());
        }
    }

    if parts.is_empty() {
        return "No live board state is available yet, so there is nothing to coach from. \
Connect the simulator and the coach will read the board automatically. \
Set OPTCG_LLM_API_KEY to get conversational answers."
            .to_string();
    }

    parts.push(
        "(Offline coach: answering from the rules engine. Set OPTCG_LLM_API_KEY for \
conversational answers.)"
            .to_string(),
    );
    parts.join("\n\n")
}

fn mentions_deck_strategy(question: &str) -> bool {
    let q = question.to_ascii_lowercase();
    ["deck", "matchup", "plan", "strategy", "against", "list"]
        .iter()
        .any(|needle| q.contains(needle))
}

/// Split a briefing rendered by [`crate::grounding::GroundedContext::to_prompt`].
fn parse_sections(briefing: &str) -> std::collections::HashMap<String, String> {
    let mut sections = std::collections::HashMap::new();
    let mut heading: Option<String> = None;
    let mut body = String::new();

    for line in briefing.lines() {
        if let Some(next) = line.strip_prefix("## ") {
            if let Some(previous) = heading.take() {
                sections.insert(previous, body.trim().to_string());
            }
            heading = Some(next.trim().to_string());
            body.clear();
        } else if heading.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(previous) = heading {
        sections.insert(previous, body.trim().to_string());
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::test_support::recording_sink;

    const BRIEFING: &str = "\
## Board
Turn 4, Main phase, active player: you
You: life 3, hand 5

## Phase guidance
Play a character and attack the leader.

## Ranked options
1. Attack leader with ST01-002 (score 0.80) — trades up
2. Play ST01-003 (score 0.40) — builds board

## Decks
Your deck: Red Luffy Aggro (leader ST01-001)";

    fn messages(question: &str) -> Vec<ChatMessage> {
        vec![ChatMessage::system(BRIEFING), ChatMessage::user(question)]
    }

    #[tokio::test]
    async fn streams_the_answer_word_by_word() {
        let (sink, recorder) = recording_sink();
        let provider = OfflineProvider::instant();

        let answer = provider
            .stream_chat(&messages("what now?"), &sink, &CancelToken::new())
            .await
            .unwrap();

        assert_eq!(
            recorder.text(),
            answer,
            "concatenated deltas must equal the returned answer"
        );
        let delta_count = recorder
            .events()
            .iter()
            .filter(|e| matches!(e, CoachEvent::TextDelta(_)))
            .count();
        assert!(
            delta_count > 5,
            "expected incremental deltas, got {delta_count}"
        );
    }

    #[tokio::test]
    async fn leads_with_the_best_ranked_option() {
        let (sink, _recorder) = recording_sink();
        let answer = OfflineProvider::instant()
            .stream_chat(&messages("what should I do?"), &sink, &CancelToken::new())
            .await
            .unwrap();

        assert!(
            answer.starts_with("Best line right now: Attack leader with ST01-002"),
            "unexpected answer: {answer}"
        );
    }

    #[tokio::test]
    async fn deck_context_only_answers_deck_questions() {
        let (sink, _r) = recording_sink();
        let generic = OfflineProvider::instant()
            .stream_chat(&messages("what now?"), &sink, &CancelToken::new())
            .await
            .unwrap();
        assert!(!generic.contains("Red Luffy Aggro"));

        let (sink, _r) = recording_sink();
        let strategic = OfflineProvider::instant()
            .stream_chat(
                &messages("how does this matchup play out?"),
                &sink,
                &CancelToken::new(),
            )
            .await
            .unwrap();
        assert!(strategic.contains("Red Luffy Aggro"), "got: {strategic}");
    }

    #[tokio::test]
    async fn cancellation_stops_the_stream() {
        let (sink, recorder) = recording_sink();
        let cancel = CancelToken::new();
        cancel.cancel();

        let result = OfflineProvider::instant()
            .stream_chat(&messages("what now?"), &sink, &cancel)
            .await;

        assert!(matches!(result, Err(CoachError::Cancelled)));
        assert!(
            recorder.text().is_empty(),
            "no text should stream after cancellation"
        );
    }

    #[tokio::test]
    async fn explains_itself_when_there_is_no_board_state() {
        let (sink, _recorder) = recording_sink();
        let answer = OfflineProvider::instant()
            .stream_chat(
                &[ChatMessage::user("what now?")],
                &sink,
                &CancelToken::new(),
            )
            .await
            .unwrap();

        assert!(answer.contains("No live board state"), "got: {answer}");
        assert!(answer.contains("OPTCG_LLM_API_KEY"));
    }

    #[test]
    fn parses_briefing_sections() {
        let sections = parse_sections(BRIEFING);
        assert_eq!(sections.len(), 4);
        assert!(sections["Board"].starts_with("Turn 4"));
        assert!(sections["Ranked options"].contains("2. Play ST01-003"));
    }

    #[test]
    fn parsing_ignores_text_before_the_first_heading() {
        let sections = parse_sections("preamble\n## Only\nbody");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections["Only"], "body");
    }
}
