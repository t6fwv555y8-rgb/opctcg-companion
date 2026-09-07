use crate::types::StateFingerprint;
use std::time::{Duration, Instant};

/// The question an automatic read asks on the user's behalf.
pub const AUTO_QUESTION: &str =
    "The board just changed. Give me the single best move right now and why, in two sentences.";

/// How long the board must hold still before an automatic read fires.
///
/// A play sequence (drop a character, attach DON, declare an attack) produces
/// several positions in quick succession. Waiting for it to settle means one
/// read of the finished position instead of three reads of half-finished ones.
pub const DEFAULT_SETTLE_MS: u64 = 1_500;

/// Floor on the gap between automatic reads, regardless of board activity.
///
/// A backstop on token spend and on how often the panel can churn, for the
/// case where changes keep arriving exactly as each settle window closes.
pub const DEFAULT_MIN_INTERVAL_MS: u64 = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoTriggerConfig {
    /// On by default so the HUD keeps advising as the match moves.
    pub enabled: bool,
    pub settle: Duration,
    pub min_interval: Duration,
}

impl Default for AutoTriggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            settle: Duration::from_millis(DEFAULT_SETTLE_MS),
            min_interval: Duration::from_millis(DEFAULT_MIN_INTERVAL_MS),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoDecision {
    /// Nothing worth reading, or automatic reads are off.
    Idle,
    /// A change is waiting out the settle window or the rate limit.
    Settling,
    /// Read this position now.
    Fire,
}

/// Decides when a board change deserves an unprompted read.
///
/// Pure state machine with the clock passed in, so the policy is testable
/// without sleeping. It answers one question — should we ask now? — and knows
/// nothing about providers or transports.
#[derive(Debug, Default)]
pub struct AutoTrigger {
    config: AutoTriggerConfig,
    /// Position of the last read, so an unchanged board is not asked twice.
    answered: Option<String>,
    /// Position currently waiting out the settle window, and when it appeared.
    settling: Option<(String, Instant)>,
    last_fired: Option<Instant>,
}

impl AutoTrigger {
    pub fn new(config: AutoTriggerConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Turn automatic reads on or off, discarding any in-progress settle.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
        self.settling = None;
        if !enabled {
            // Re-enabling should read the current board rather than assume the
            // last position it saw is still answered.
            self.answered = None;
        }
    }

    /// Forget all history, e.g. when a new match starts.
    pub fn reset(&mut self) {
        self.answered = None;
        self.settling = None;
        self.last_fired = None;
    }

    /// Record the current position and decide whether to read it.
    ///
    /// `at_decision_point` should be false whenever the user has nothing to
    /// decide, so automatic reads stay quiet during the opponent's turn and
    /// the phases that play themselves.
    pub fn observe(
        &mut self,
        fingerprint: &StateFingerprint,
        at_decision_point: bool,
        now: Instant,
    ) -> AutoDecision {
        if !self.config.enabled || !at_decision_point {
            self.settling = None;
            return AutoDecision::Idle;
        }

        if self.answered.as_deref() == Some(fingerprint.digest.as_str()) {
            self.settling = None;
            return AutoDecision::Idle;
        }

        // Restart the settle window whenever the position differs from the one
        // being waited on.
        let first_seen = match &self.settling {
            Some((digest, since)) if *digest == fingerprint.digest => *since,
            _ => {
                self.settling = Some((fingerprint.digest.clone(), now));
                now
            }
        };

        if now.duration_since(first_seen) < self.config.settle {
            return AutoDecision::Settling;
        }
        if let Some(last) = self.last_fired {
            if now.duration_since(last) < self.config.min_interval {
                return AutoDecision::Settling;
            }
        }

        self.last_fired = Some(now);
        self.answered = Some(fingerprint.digest.clone());
        self.settling = None;
        AutoDecision::Fire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AutoTriggerConfig {
        AutoTriggerConfig {
            enabled: true,
            settle: Duration::from_millis(1_000),
            min_interval: Duration::from_millis(5_000),
        }
    }

    fn position(digest: &str) -> StateFingerprint {
        StateFingerprint {
            label: digest.into(),
            digest: digest.into(),
        }
    }

    #[test]
    fn on_by_default() {
        let mut trigger = AutoTrigger::default();
        assert!(trigger.is_enabled());
        assert_eq!(
            trigger.observe(&position("a"), true, Instant::now()),
            AutoDecision::Settling,
            "a live match should start reading as soon as the board moves"
        );
    }

    #[test]
    fn fires_once_the_board_has_settled() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();

        assert_eq!(
            trigger.observe(&position("a"), true, start),
            AutoDecision::Settling,
            "a fresh change should wait"
        );
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_millis(900)),
            AutoDecision::Settling,
            "still inside the settle window"
        );
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_millis(1_000)),
            AutoDecision::Fire
        );
    }

    #[test]
    fn a_play_sequence_produces_one_read_not_three() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();

        // Drop a character, attach DON, declare an attack, 300ms apart.
        for (i, digest) in ["play", "don", "attack"].iter().enumerate() {
            let at = start + Duration::from_millis(300 * i as u64);
            assert_eq!(
                trigger.observe(&position(digest), true, at),
                AutoDecision::Settling,
                "mid-sequence positions must not fire"
            );
        }

        // Only the position the sequence ended on gets read.
        assert_eq!(
            trigger.observe(
                &position("attack"),
                true,
                start + Duration::from_millis(1_700)
            ),
            AutoDecision::Fire
        );
    }

    #[test]
    fn an_unchanged_board_is_not_read_twice() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();

        trigger.observe(&position("a"), true, start);
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_secs(1)),
            AutoDecision::Fire
        );
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_secs(30)),
            AutoDecision::Idle,
            "the same position must not be asked about again"
        );
    }

    #[test]
    fn the_rate_limit_holds_back_a_rapid_second_read() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();

        trigger.observe(&position("a"), true, start);
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_millis(1_000)),
            AutoDecision::Fire
        );

        // A new position settles, but the rate limit has not elapsed.
        let settled = start + Duration::from_millis(3_000);
        trigger.observe(&position("b"), true, start + Duration::from_millis(1_100));
        assert_eq!(
            trigger.observe(&position("b"), true, settled),
            AutoDecision::Settling,
            "the floor between reads should hold it back"
        );

        assert_eq!(
            trigger.observe(&position("b"), true, start + Duration::from_millis(6_100)),
            AutoDecision::Fire,
            "and release it once the floor has passed"
        );
    }

    #[test]
    fn nothing_fires_away_from_a_decision_point() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();

        assert_eq!(
            trigger.observe(&position("a"), false, start),
            AutoDecision::Idle
        );
        assert_eq!(
            trigger.observe(&position("a"), false, start + Duration::from_secs(5)),
            AutoDecision::Idle,
            "waiting through the opponent's turn must not bank a read"
        );

        // Becoming a decision point starts the settle window from that moment.
        let resumed = start + Duration::from_secs(5);
        assert_eq!(
            trigger.observe(&position("a"), true, resumed),
            AutoDecision::Settling
        );
        assert_eq!(
            trigger.observe(&position("a"), true, resumed + Duration::from_secs(1)),
            AutoDecision::Fire
        );
    }

    #[test]
    fn toggling_off_discards_a_pending_read() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();
        trigger.observe(&position("a"), true, start);

        trigger.set_enabled(false);
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_secs(2)),
            AutoDecision::Idle
        );

        // Re-enabling reads the board as it is now, rather than treating the
        // position it last saw as already answered.
        trigger.set_enabled(true);
        let resumed = start + Duration::from_secs(3);
        assert_eq!(
            trigger.observe(&position("a"), true, resumed),
            AutoDecision::Settling
        );
        assert_eq!(
            trigger.observe(&position("a"), true, resumed + Duration::from_secs(1)),
            AutoDecision::Fire
        );
    }

    #[test]
    fn reset_clears_the_rate_limit_and_answered_position() {
        let mut trigger = AutoTrigger::new(config());
        let start = Instant::now();
        trigger.observe(&position("a"), true, start);
        assert_eq!(
            trigger.observe(&position("a"), true, start + Duration::from_secs(1)),
            AutoDecision::Fire
        );

        trigger.reset();
        let after = start + Duration::from_secs(2);
        assert_eq!(
            trigger.observe(&position("a"), true, after),
            AutoDecision::Settling
        );
        assert_eq!(
            trigger.observe(&position("a"), true, after + Duration::from_secs(1)),
            AutoDecision::Fire,
            "a new match should read the board again"
        );
    }
}
