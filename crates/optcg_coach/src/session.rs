use crate::provider::{CancelReason, CancelToken};
use crate::types::{ChatMessage, ChatRole, StateFingerprint};

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
    in_flight: Option<InFlight>,
}

/// Who asked for a turn, which decides whether it joins the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnKind {
    /// The user asked. Recorded in history so follow-ups have context.
    User,
    /// Fired by a board change. Not recorded: repeated board reads would
    /// evict the user's own conversation from the capped history and bias the
    /// model toward repeating its last answer.
    Auto,
}

#[derive(Debug)]
struct InFlight {
    turn_id: u64,
    kind: TurnKind,
    /// Kept for automatic turns, whose question is never written to history
    /// and so has to be appended when the prompt is assembled.
    question: String,
    cancel: CancelToken,
    /// The position this turn was grounded on, once grounding has run.
    grounded_on: Option<StateFingerprint>,
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
        self.in_flight.as_ref().map(|turn| turn.turn_id)
    }

    /// Begin a turn: validate the question, record it, and hand back the turn
    /// id and its cancellation token.
    ///
    /// Any turn still in flight is cancelled first, so a user who sends again
    /// without waiting gets the newest answer rather than two interleaved ones.
    pub fn begin_turn(&mut self, message: &str) -> Result<(u64, CancelToken), String> {
        self.begin_turn_of_kind(message, TurnKind::User)
    }

    /// Begin a turn the user did not ask for, triggered by a board change.
    pub fn begin_auto_turn(&mut self, message: &str) -> Result<(u64, CancelToken), String> {
        self.begin_turn_of_kind(message, TurnKind::Auto)
    }

    pub fn begin_turn_of_kind(
        &mut self,
        message: &str,
        kind: TurnKind,
    ) -> Result<(u64, CancelToken), String> {
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
        self.in_flight = Some(InFlight {
            turn_id,
            kind,
            question: trimmed.to_string(),
            cancel: cancel.clone(),
            grounded_on: None,
        });
        if kind == TurnKind::User {
            self.history.push(ChatMessage::user(trimmed));
        }
        Ok((turn_id, cancel))
    }

    /// Kind of the streaming turn, if one is in flight.
    pub fn active_kind(&self) -> Option<TurnKind> {
        self.in_flight.as_ref().map(|turn| turn.kind)
    }

    /// Record the position `turn_id` was grounded on, enabling staleness checks.
    ///
    /// Ignored for a turn that is no longer active.
    pub fn record_grounding(&mut self, turn_id: u64, fingerprint: StateFingerprint) {
        if let Some(turn) = self.in_flight.as_mut().filter(|t| t.turn_id == turn_id) {
            turn.grounded_on = Some(fingerprint);
        }
    }

    /// Cancel the streaming turn if the board has moved away from what it was
    /// grounded on, returning the turn id that was interrupted.
    ///
    /// A turn whose grounding has not finished yet is left alone: it will read
    /// the current board when it gets there, so it cannot be stale.
    pub fn interrupt_if_stale(&mut self, current: &StateFingerprint) -> Option<u64> {
        let turn = self.in_flight.as_ref()?;
        let grounded_on = turn.grounded_on.as_ref()?;
        if !grounded_on.is_stale_against(current) {
            return None;
        }
        let turn_id = turn.turn_id;
        // The reason travels with the token so the streaming task emits the
        // single terminal frame, keeping text and `done` in order.
        turn.cancel.cancel_with(CancelReason::Stale);
        self.in_flight = None;
        Some(turn_id)
    }

    /// Record a completed answer and clear the in-flight turn.
    ///
    /// Ignored when `turn_id` is not the active turn, so a superseded turn
    /// finishing late cannot append its answer after a newer question.
    pub fn finish_turn(&mut self, turn_id: u64, answer: &str) -> bool {
        let Some(turn) = self.in_flight.take_if(|turn| turn.turn_id == turn_id) else {
            return false;
        };
        if turn.kind == TurnKind::User && !answer.trim().is_empty() {
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
        let turn = self.in_flight.take()?;
        turn.cancel.cancel();
        Some(turn.turn_id)
    }

    /// Clear the conversation and stop any streaming turn.
    pub fn reset(&mut self) {
        self.cancel_active();
        self.history.clear();
    }

    /// Messages to send for this turn: the system briefing then recent history.
    pub fn prompt_with(&self, system: String) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(self.history.len() + 2);
        messages.push(ChatMessage::system(system));
        messages.extend(self.history.iter().cloned());
        // An automatic turn is absent from history, so its question has to be
        // appended here or the model would be sent a briefing and no question.
        if let Some(turn) = self
            .in_flight
            .as_ref()
            .filter(|turn| turn.kind == TurnKind::Auto)
        {
            messages.push(ChatMessage::user(turn.question.clone()));
        }
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

        assert!(
            first_cancel.is_cancelled(),
            "superseded turn must be cancelled"
        );
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
            session
                .history()
                .iter()
                .all(|m| m.content != "stale answer"),
            "stale text must not enter history"
        );
        assert_eq!(
            session.active_turn(),
            Some(second),
            "the live turn survives"
        );
    }

    fn position(digest: &str) -> StateFingerprint {
        StateFingerprint {
            label: digest.into(),
            digest: digest.into(),
        }
    }

    #[test]
    fn a_stale_turn_is_interrupted() {
        let mut session = CoachSession::new();
        let (turn, cancel) = session.begin_turn("what now?").unwrap();
        session.record_grounding(turn, position("turn-4"));

        assert_eq!(
            session.interrupt_if_stale(&position("turn-4")),
            None,
            "an unchanged board must not interrupt the turn"
        );
        assert!(!cancel.is_cancelled());

        assert_eq!(
            session.interrupt_if_stale(&position("turn-5")),
            Some(turn),
            "a changed board should interrupt"
        );
        assert!(cancel.is_cancelled());
        assert_eq!(
            cancel.reason(),
            Some(CancelReason::Stale),
            "the reason must distinguish this from the user pressing Stop"
        );
        assert!(!session.is_busy());
    }

    #[test]
    fn a_turn_still_grounding_is_never_stale() {
        let mut session = CoachSession::new();
        let (_turn, cancel) = session.begin_turn("what now?").unwrap();

        // Grounding has not run, so the turn has not committed to a position
        // yet and will read whatever is current when it gets there.
        assert_eq!(session.interrupt_if_stale(&position("anything")), None);
        assert!(!cancel.is_cancelled());
        assert!(session.is_busy());
    }

    #[test]
    fn staleness_checks_are_safe_with_nothing_in_flight() {
        let mut session = CoachSession::new();
        assert_eq!(session.interrupt_if_stale(&position("turn-4")), None);

        let (turn, _) = session.begin_turn("q").unwrap();
        session.record_grounding(turn, position("turn-4"));
        session.finish_turn(turn, "a");
        assert_eq!(
            session.interrupt_if_stale(&position("turn-9")),
            None,
            "a finished turn cannot be interrupted"
        );
    }

    #[test]
    fn grounding_is_not_recorded_against_a_superseded_turn() {
        let mut session = CoachSession::new();
        let (first, _) = session.begin_turn("q1").unwrap();
        let (second, _) = session.begin_turn("q2").unwrap();

        session.record_grounding(first, position("stale-position"));
        assert_eq!(
            session.interrupt_if_stale(&position("current")),
            None,
            "the live turn has no grounding yet, so the stale write was ignored"
        );

        session.record_grounding(second, position("current"));
        assert_eq!(session.interrupt_if_stale(&position("current")), None);
        assert_eq!(session.interrupt_if_stale(&position("moved")), Some(second));
    }

    #[test]
    fn an_auto_turn_stays_out_of_the_conversation() {
        let mut session = CoachSession::new();
        let (turn, _) = session.begin_auto_turn("what changed?").unwrap();

        assert_eq!(session.active_kind(), Some(TurnKind::Auto));
        assert!(
            session.history().is_empty(),
            "an unprompted question is not part of the conversation"
        );

        // The question still reaches the model for this turn.
        let prompt = session.prompt_with("BRIEFING".into());
        assert_eq!(prompt.len(), 2, "system briefing plus the question");
        assert_eq!(prompt[1].role, ChatRole::User);
        assert_eq!(prompt[1].content, "what changed?");

        assert!(session.finish_turn(turn, "Attack the leader."));
        assert!(
            session.history().is_empty(),
            "an automatic answer must not evict the user's own conversation"
        );
    }

    #[test]
    fn auto_turns_do_not_crowd_out_user_history() {
        let mut session = CoachSession::new();
        let (asked, _) = session.begin_turn("why am I losing?").unwrap();
        session.finish_turn(asked, "You are behind on board.");

        // Many board changes fire many automatic reads.
        for i in 0..HISTORY_TURNS * 3 {
            let (turn, _) = session.begin_auto_turn(&format!("read {i}")).unwrap();
            session.finish_turn(turn, &format!("answer {i}"));
        }

        assert_eq!(
            session.history().len(),
            2,
            "only the user's own turn should remain: {:?}",
            session.history()
        );
        assert_eq!(session.history()[0].content, "why am I losing?");
    }

    #[test]
    fn a_user_turn_supersedes_a_running_auto_turn() {
        let mut session = CoachSession::new();
        let (auto, auto_cancel) = session.begin_auto_turn("read").unwrap();
        let (asked, _) = session.begin_turn("what about blocking?").unwrap();

        assert!(auto_cancel.is_cancelled(), "the user takes priority");
        assert_eq!(session.active_turn(), Some(asked));
        assert_eq!(session.active_kind(), Some(TurnKind::User));
        assert!(
            !session.finish_turn(auto, "stale read"),
            "the superseded auto turn must not report"
        );
    }

    #[test]
    fn a_user_prompt_omits_the_current_question_because_history_holds_it() {
        let mut session = CoachSession::new();
        session.begin_turn("what now?").unwrap();

        let prompt = session.prompt_with("BRIEFING".into());
        assert_eq!(
            prompt.len(),
            2,
            "system briefing plus the recorded question"
        );
        assert_eq!(prompt[1].content, "what now?");
        assert_eq!(
            prompt.iter().filter(|m| m.content == "what now?").count(),
            1,
            "the question must not be duplicated"
        );
    }

    #[test]
    fn supersession_is_distinct_from_cancellation() {
        let mut session = CoachSession::new();
        let (first, _) = session.begin_turn("q1").unwrap();
        assert!(
            !session.is_superseded(first),
            "the only turn is not superseded"
        );

        session.begin_turn("q2").unwrap();
        assert!(
            session.is_superseded(first),
            "a newer turn supersedes the old one"
        );

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
