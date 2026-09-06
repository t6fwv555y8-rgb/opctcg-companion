use crate::provider::CancelToken;
use crate::types::{ChatMessage, ChatRole};

/// How many prior turns to replay to the model. Each turn is a user message
/// plus its answer, so this is a bound on prompt growth, not on stored history.
pub const HISTORY_TURNS: usize = 8;

/// Longest single question accepted, to keep one paste from filling the prompt.
pub const MAX_MESSAGE_CHARS: usize = 2000;

/// Conversation state for the coach chat.
///
/// Holds no locks of its own: the desktop app wraps this in a lock and is
/// responsible for never holding that lock across an await.
#[derive(Debug, Default)]
pub struct CoachSession {
    history: Vec<ChatMessage>,
    next_turn_id: u64,
    in_flight: Option<(u64, CancelToken)>,
}

impl CoachSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    pub fn is_busy(&self) -> bool {
        self.in_flight.is_some()
    }

    pub fn active_turn(&self) -> Option<u64> {
        self.in_flight.as_ref().map(|(id, _)| *id)
    }

    /// Begin a turn: validate the question, record it, and hand back the turn
    /// id and its cancellation token.
    ///
    /// Any turn still in flight is cancelled first, so a user who sends again
    /// without waiting gets the newest answer rather than two interleaved ones.
    pub fn begin_turn(&mut self, message: &str) -> Result<(u64, CancelToken), String> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err("Ask the coach a question first".into());
        }
        if trimmed.chars().count() > MAX_MESSAGE_CHARS {
            return Err(format!(
                "Question is too long ({} of {MAX_MESSAGE_CHARS} characters)",
                trimmed.chars().count()
            ));
        }

        self.cancel_active();

        self.next_turn_id += 1;
        let turn_id = self.next_turn_id;
        let cancel = CancelToken::new();
        self.in_flight = Some((turn_id, cancel.clone()));
        self.history.push(ChatMessage::user(trimmed));
        Ok((turn_id, cancel))
    }

    /// Record a completed answer and clear the in-flight turn.
    ///
    /// Ignored when `turn_id` is not the active turn, so a superseded turn
    /// finishing late cannot append its answer after a newer question.
    pub fn finish_turn(&mut self, turn_id: u64, answer: &str) -> bool {
        if self.active_turn() != Some(turn_id) {
            return false;
        }
        self.in_flight = None;
        if !answer.trim().is_empty() {
            self.history.push(ChatMessage::assistant(answer.trim()));
        }
        self.trim_history();
        true
    }

    /// Abandon a turn without recording an answer.
    pub fn abandon_turn(&mut self, turn_id: u64) {
        if self.active_turn() == Some(turn_id) {
            self.in_flight = None;
        }
    }

    /// True when a newer turn has taken over from `turn_id`.
    ///
    /// Distinguishes "the user asked something else" (the UI has already moved
    /// on, so late frames are noise) from "the turn was cancelled or finished"
    /// (the UI still needs to be told it ended).
    pub fn is_superseded(&self, turn_id: u64) -> bool {
        matches!(self.active_turn(), Some(active) if active != turn_id)
    }

    /// Cancel whatever is streaming, returning the turn id if there was one.
    pub fn cancel_active(&mut self) -> Option<u64> {
        let (turn_id, cancel) = self.in_flight.take()?;
        cancel.cancel();
        Some(turn_id)
    }

    /// Clear the conversation and stop any streaming turn.
    pub fn reset(&mut self) {
        self.cancel_active();
        self.history.clear();
    }

    /// Messages to send for this turn: the system briefing then recent history.
    pub fn prompt_with(&self, system: String) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(self.history.len() + 1);
        messages.push(ChatMessage::system(system));
        messages.extend(self.history.iter().cloned());
        messages
    }

    /// Keep the most recent turns, always starting at a user message so the
    /// model never receives a dangling assistant reply as the first turn.
    fn trim_history(&mut self) {
        let max_messages = HISTORY_TURNS * 2;
        if self.history.len() <= max_messages {
            return;
        }
        let mut start = self.history.len() - max_messages;
        while start < self.history.len() && self.history[start].role != ChatRole::User {
            start += 1;
        }
        self.history.drain(..start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_turn_records_the_question_and_issues_ids() {
        let mut session = CoachSession::new();
        let (first, _) = session.begin_turn("what now?").unwrap();
        assert_eq!(first, 1);
        assert!(session.is_busy());
        assert_eq!(session.history().len(), 1);
        assert_eq!(session.history()[0].role, ChatRole::User);

        assert!(session.finish_turn(first, "attack"));
        let (second, _) = session.begin_turn("and then?").unwrap();
        assert_eq!(second, 2, "turn ids must keep increasing");
    }

    #[test]
    fn blank_and_oversized_questions_are_rejected() {
        let mut session = CoachSession::new();
        assert!(session.begin_turn("   ").is_err());
        assert!(session
            .begin_turn(&"x".repeat(MAX_MESSAGE_CHARS + 1))
            .is_err());
        assert!(session.history().is_empty());
        assert!(!session.is_busy());
    }

    #[test]
    fn questions_are_stored_trimmed() {
        let mut session = CoachSession::new();
        session.begin_turn("  spaced  ").unwrap();
        assert_eq!(session.history()[0].content, "spaced");
    }

    #[test]
    fn finish_turn_appends_the_answer() {
        let mut session = CoachSession::new();
        let (turn, _) = session.begin_turn("q").unwrap();
        assert!(session.finish_turn(turn, "a"));

        assert!(!session.is_busy());
        assert_eq!(session.history().len(), 2);
        assert_eq!(session.history()[1].role, ChatRole::Assistant);
        assert_eq!(session.history()[1].content, "a");
    }

    #[test]
    fn empty_answers_are_not_recorded() {
        let mut session = CoachSession::new();
        let (turn, _) = session.begin_turn("q").unwrap();
        assert!(session.finish_turn(turn, "   "));
        assert_eq!(session.history().len(), 1, "only the question is kept");
    }

    #[test]
    fn a_new_question_cancels_the_previous_turn() {
        let mut session = CoachSession::new();
        let (first, first_cancel) = session.begin_turn("q1").unwrap();
        let (second, _) = session.begin_turn("q2").unwrap();

        assert!(first_cancel.is_cancelled(), "superseded turn must be cancelled");
        assert_eq!(session.active_turn(), Some(second));
        assert_ne!(first, second);
    }

    #[test]
    fn a_superseded_turn_cannot_append_its_answer() {
        let mut session = CoachSession::new();
        let (first, _) = session.begin_turn("q1").unwrap();
        let (second, _) = session.begin_turn("q2").unwrap();

        assert!(
            !session.finish_turn(first, "stale answer"),
            "the stale turn should be refused"
        );
        assert!(
            session.history().iter().all(|m| m.content != "stale answer"),
            "stale text must not enter history"
        );
        assert_eq!(session.active_turn(), Some(second), "the live turn survives");
    }

    #[test]
    fn supersession_is_distinct_from_cancellation() {
        let mut session = CoachSession::new();
        let (first, _) = session.begin_turn("q1").unwrap();
        assert!(!session.is_superseded(first), "the only turn is not superseded");

        session.begin_turn("q2").unwrap();
        assert!(session.is_superseded(first), "a newer turn supersedes the old one");

        let mut cancelled = CoachSession::new();
        let (turn, _) = cancelled.begin_turn("q").unwrap();
        cancelled.cancel_active();
        assert!(
            !cancelled.is_superseded(turn),
            "an explicitly cancelled turn is not superseded, so it still reports"
        );
    }

    #[test]
    fn cancel_active_reports_and_clears_the_turn() {
        let mut session = CoachSession::new();
        let (turn, cancel) = session.begin_turn("q").unwrap();

        assert_eq!(session.cancel_active(), Some(turn));
        assert!(cancel.is_cancelled());
        assert!(!session.is_busy());
        assert_eq!(session.cancel_active(), None, "nothing left to cancel");
    }

    #[test]
    fn abandon_only_affects_the_named_turn() {
        let mut session = CoachSession::new();
        let (first, _) = session.begin_turn("q1").unwrap();
        session.abandon_turn(first);
        assert!(!session.is_busy());

        let (second, _) = session.begin_turn("q2").unwrap();
        session.abandon_turn(999);
        assert_eq!(session.active_turn(), Some(second));
    }

    #[test]
    fn reset_clears_history_and_stops_streaming() {
        let mut session = CoachSession::new();
        let (turn, _) = session.begin_turn("q").unwrap();
        session.finish_turn(turn, "a");
        let (_, streaming) = session.begin_turn("q2").unwrap();

        session.reset();
        assert!(session.history().is_empty());
        assert!(!session.is_busy());
        assert!(
            streaming.is_cancelled(),
            "reset must cancel the streaming turn"
        );
    }

    #[test]
    fn prompt_puts_the_system_briefing_first() {
        let mut session = CoachSession::new();
        let (turn, _) = session.begin_turn("q1").unwrap();
        session.finish_turn(turn, "a1");

        let prompt = session.prompt_with("BRIEFING".into());
        assert_eq!(prompt[0].role, ChatRole::System);
        assert_eq!(prompt[0].content, "BRIEFING");
        assert_eq!(prompt[1].content, "q1");
        assert_eq!(prompt[2].content, "a1");
    }

    #[test]
    fn history_is_capped_and_starts_on_a_user_message() {
        let mut session = CoachSession::new();
        for i in 0..HISTORY_TURNS + 5 {
            let (turn, _) = session.begin_turn(&format!("q{i}")).unwrap();
            session.finish_turn(turn, &format!("a{i}"));
        }

        assert!(
            session.history().len() <= HISTORY_TURNS * 2,
            "history grew to {}",
            session.history().len()
        );
        assert_eq!(
            session.history()[0].role,
            ChatRole::User,
            "a trimmed history must not start with an assistant reply"
        );
        assert_eq!(
            session.history().last().unwrap().content,
            format!("a{}", HISTORY_TURNS + 4),
            "the newest turn is kept"
        );
    }
}
