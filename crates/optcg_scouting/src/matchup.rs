//! How your deck actually fares against each leader you meet.
//!
//! The scouting ledger next door learns what the opponent is playing. This
//! learns something the opponent cannot tell you and a simulator would only
//! guess at: what happens when your fifty cards meet theirs. Ten real games
//! into a leader is worth more than any amount of theorising about the
//! matchup, and it is the only evidence available that costs nothing to
//! collect.
//!
//! The whole file turns on one problem. Nothing in the observation protocol
//! says who won — there is no game-over event to listen for — so a result has
//! to be inferred from life reaching zero. That inference is allowed to fail,
//! and when it does the game is filed as unfinished rather than guessed at.

use serde::{Deserialize, Serialize};

/// Cap on remembered matchups. Two hundred leaders against a handful of your
/// own decks is far more than anyone plays, and it bounds the file.
pub const MAX_MATCHUPS: usize = 400;

/// Which way a game went, once it is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Won,
    Lost,
}

/// One pairing of your leader against theirs.
///
/// Keyed on both leaders rather than on decks: leaders are what the game shows
/// you, they survive a list being tweaked between events, and they are what
/// anyone actually means by "the matchup".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchupRecord {
    pub your_leader: String,
    pub your_leader_name: String,
    pub their_leader: String,
    pub their_leader_name: String,
    pub wins: u32,
    pub losses: u32,
    /// Games watched that never reached a conclusion: a disconnect, a concede
    /// the HUD could not see, the app closed mid-game.
    ///
    /// Counted apart from wins and losses on purpose. Folding them into either
    /// would quietly corrupt the only number here anyone will act on, and
    /// folding them into the denominator would understate a real win rate.
    pub unfinished: u32,
    /// Turn counts from finished games only, so an abandoned turn-two game
    /// cannot drag the average game length down.
    pub summed_last_turn: u32,
    /// Life you had left in games you won, which separates a comfortable
    /// matchup from one you keep stealing.
    pub summed_life_left_on_win: u32,
    /// Life they had left in games you lost. High means you were never close.
    pub summed_their_life_left_on_loss: u32,
    pub first_played: String,
    pub last_played: String,
}

impl MatchupRecord {
    fn new(
        your_leader: &str,
        your_leader_name: &str,
        their_leader: &str,
        their_leader_name: &str,
        now: &str,
    ) -> Self {
        Self {
            your_leader: your_leader.to_string(),
            your_leader_name: your_leader_name.to_string(),
            their_leader: their_leader.to_string(),
            their_leader_name: their_leader_name.to_string(),
            wins: 0,
            losses: 0,
            unfinished: 0,
            summed_last_turn: 0,
            summed_life_left_on_win: 0,
            summed_their_life_left_on_loss: 0,
            first_played: now.to_string(),
            last_played: now.to_string(),
        }
    }

    /// Games that reached a result. The denominator for every rate here.
    pub fn finished(&self) -> u32 {
        self.wins + self.losses
    }

    /// Every game seen in this matchup, concluded or not.
    pub fn played(&self) -> u32 {
        self.finished() + self.unfinished
    }

    /// Share of finished games you won, or `None` when none finished.
    ///
    /// Deliberately not defaulting to zero: never having finished a game is
    /// not the same as never having won one.
    pub fn win_rate(&self) -> Option<f32> {
        let finished = self.finished();
        (finished > 0).then(|| self.wins as f32 / finished as f32)
    }

    /// Mean turn a finished game ended on.
    pub fn average_length(&self) -> Option<f32> {
        let finished = self.finished();
        (finished > 0).then(|| self.summed_last_turn as f32 / finished as f32)
    }

    /// Mean life left when you won.
    pub fn average_life_left_on_win(&self) -> Option<f32> {
        (self.wins > 0).then(|| self.summed_life_left_on_win as f32 / self.wins as f32)
    }

    /// Mean life they had left when you lost.
    pub fn average_their_life_left_on_loss(&self) -> Option<f32> {
        (self.losses > 0).then(|| self.summed_their_life_left_on_loss as f32 / self.losses as f32)
    }

    /// Fold in one game. `result` of `None` files it as unfinished.
    pub(crate) fn fold_in(&mut self, game: &FinishedGame, now: &str) {
        self.last_played = now.to_string();
        if !game.their_leader_name.is_empty() {
            self.their_leader_name = game.their_leader_name.clone();
        }
        if !game.your_leader_name.is_empty() {
            self.your_leader_name = game.your_leader_name.clone();
        }

        match game.outcome {
            None => {
                self.unfinished = self.unfinished.saturating_add(1);
                return;
            }
            Some(Outcome::Won) => {
                self.wins = self.wins.saturating_add(1);
                self.summed_life_left_on_win =
                    self.summed_life_left_on_win.saturating_add(game.your_life);
            }
            Some(Outcome::Lost) => {
                self.losses = self.losses.saturating_add(1);
                self.summed_their_life_left_on_loss = self
                    .summed_their_life_left_on_loss
                    .saturating_add(game.their_life);
            }
        }
        self.summed_last_turn = self.summed_last_turn.saturating_add(game.last_turn);
    }
}

/// A game as it looked when it ended, ready to fold into a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinishedGame {
    pub your_leader: String,
    pub your_leader_name: String,
    pub their_leader: String,
    pub their_leader_name: String,
    pub outcome: Option<Outcome>,
    pub your_life: u32,
    pub their_life: u32,
    pub last_turn: u32,
}

/// Life totals as a game progressed, and what they imply about the result.
///
/// Kept as its own type because the inference is the delicate part of this
/// module and deserves to be testable on its own, away from game state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifeTrack {
    /// Most life you have been seen with this game.
    pub your_high: u32,
    /// Most life they have been seen with.
    pub their_high: u32,
    /// Latest reading for each.
    pub your_last: u32,
    pub their_last: u32,
}

impl LifeTrack {
    /// Take a reading.
    pub fn observe(&mut self, your_life: u32, their_life: u32) {
        self.your_high = self.your_high.max(your_life);
        self.their_high = self.their_high.max(their_life);
        self.your_last = your_life;
        self.their_last = their_life;
    }

    /// The result these life totals imply, if any.
    ///
    /// Zero life is only believed when that side was previously seen with life
    /// to lose. Connecting to a game already over, or to a position the adapter
    /// has not filled in yet, both present as zero, and neither is a game you
    /// played. Requiring a fall from a positive reading costs us the rare game
    /// joined at exactly lethal and buys immunity from phantom losses on every
    /// startup.
    pub fn outcome(&self) -> Option<Outcome> {
        let you_died = self.your_last == 0 && self.your_high > 0;
        let they_died = self.their_last == 0 && self.their_high > 0;
        match (you_died, they_died) {
            // Both at zero cannot be read. It happens when a fresh game's
            // totals arrive one side at a time, and calling it either way
            // would be a coin flip recorded as a fact.
            (true, true) => None,
            (true, false) => Some(Outcome::Lost),
            (false, true) => Some(Outcome::Won),
            (false, false) => None,
        }
    }
}

/// Every matchup your decks have been through.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchupLedger {
    pub records: Vec<MatchupRecord>,
}

impl MatchupLedger {
    /// The record for one pairing, if it has been played.
    pub fn record(&self, your_leader: &str, their_leader: &str) -> Option<&MatchupRecord> {
        self.records
            .iter()
            .find(|r| r.your_leader == your_leader && r.their_leader == their_leader)
    }

    /// Every matchup played with one of your leaders, most played first.
    pub fn records_for(&self, your_leader: &str) -> Vec<&MatchupRecord> {
        let mut found: Vec<&MatchupRecord> = self
            .records
            .iter()
            .filter(|r| r.your_leader == your_leader)
            .collect();
        found.sort_by(|a, b| {
            b.played()
                .cmp(&a.played())
                .then(a.their_leader.cmp(&b.their_leader))
        });
        found
    }

    pub(crate) fn fold_in(&mut self, game: &FinishedGame, now: &str) {
        if game.your_leader.is_empty() || game.their_leader.is_empty() {
            return;
        }
        let position = self
            .records
            .iter()
            .position(|r| r.your_leader == game.your_leader && r.their_leader == game.their_leader);
        let index = match position {
            Some(index) => index,
            None => {
                if self.records.len() >= MAX_MATCHUPS {
                    // Drop whichever matchup has gone longest without a game.
                    if let Some(stalest) = self
                        .records
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| a.last_played.cmp(&b.last_played))
                        .map(|(i, _)| i)
                    {
                        self.records.remove(stalest);
                    }
                }
                self.records.push(MatchupRecord::new(
                    &game.your_leader,
                    &game.your_leader_name,
                    &game.their_leader,
                    &game.their_leader_name,
                    now,
                ));
                self.records.len() - 1
            }
        };
        self.records[index].fold_in(game, now);
    }

    pub(crate) fn prune(&mut self) {
        self.records
            .retain(|r| !r.your_leader.trim().is_empty() && !r.their_leader.trim().is_empty());
        self.records.truncate(MAX_MATCHUPS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-01-01T00:00:00Z";

    fn game(outcome: Option<Outcome>) -> FinishedGame {
        FinishedGame {
            your_leader: "ST01-001".into(),
            your_leader_name: "Red Luffy".into(),
            their_leader: "OP17-079".into(),
            their_leader_name: "Loki".into(),
            outcome,
            your_life: 2,
            their_life: 0,
            last_turn: 7,
        }
    }

    #[test]
    fn a_win_and_a_loss_make_a_record() {
        let mut ledger = MatchupLedger::default();
        ledger.fold_in(&game(Some(Outcome::Won)), NOW);
        ledger.fold_in(&game(Some(Outcome::Lost)), NOW);

        let record = ledger.record("ST01-001", "OP17-079").expect("recorded");
        assert_eq!((record.wins, record.losses), (1, 1));
        assert_eq!(record.win_rate(), Some(0.5));
    }

    #[test]
    fn an_unfinished_game_is_neither_a_win_nor_a_loss() {
        let mut ledger = MatchupLedger::default();
        ledger.fold_in(&game(Some(Outcome::Won)), NOW);
        ledger.fold_in(&game(None), NOW);

        let record = ledger.record("ST01-001", "OP17-079").unwrap();
        assert_eq!(record.unfinished, 1);
        assert_eq!(record.finished(), 1);
        assert_eq!(record.played(), 2);
        assert_eq!(
            record.win_rate(),
            Some(1.0),
            "a game that never concluded must not be counted as a defeat"
        );
    }

    #[test]
    fn a_matchup_with_no_finished_games_reports_no_rate() {
        let mut ledger = MatchupLedger::default();
        ledger.fold_in(&game(None), NOW);

        let record = ledger.record("ST01-001", "OP17-079").unwrap();
        assert_eq!(
            record.win_rate(),
            None,
            "never having finished a game is not the same as never having won"
        );
    }

    #[test]
    fn unfinished_games_do_not_skew_game_length() {
        let mut ledger = MatchupLedger::default();
        let mut long = game(Some(Outcome::Won));
        long.last_turn = 9;
        ledger.fold_in(&long, NOW);
        let mut abandoned = game(None);
        abandoned.last_turn = 1;
        ledger.fold_in(&abandoned, NOW);

        let record = ledger.record("ST01-001", "OP17-079").unwrap();
        assert_eq!(
            record.average_length(),
            Some(9.0),
            "a game walked away from on turn one says nothing about how long the matchup runs"
        );
    }

    #[test]
    fn how_close_the_games_were_is_kept_per_side() {
        let mut ledger = MatchupLedger::default();
        let mut won = game(Some(Outcome::Won));
        won.your_life = 4;
        ledger.fold_in(&won, NOW);
        let mut lost = game(Some(Outcome::Lost));
        lost.their_life = 5;
        ledger.fold_in(&lost, NOW);

        let record = ledger.record("ST01-001", "OP17-079").unwrap();
        assert_eq!(record.average_life_left_on_win(), Some(4.0));
        assert_eq!(
            record.average_their_life_left_on_loss(),
            Some(5.0),
            "losing while they are still on full life is a different problem from losing narrowly"
        );
    }

    #[test]
    fn your_own_leader_separates_matchups() {
        let mut ledger = MatchupLedger::default();
        ledger.fold_in(&game(Some(Outcome::Won)), NOW);
        let mut other_deck = game(Some(Outcome::Lost));
        other_deck.your_leader = "OP01-002".into();
        ledger.fold_in(&other_deck, NOW);

        assert_eq!(ledger.record("ST01-001", "OP17-079").unwrap().wins, 1);
        assert_eq!(ledger.record("OP01-002", "OP17-079").unwrap().losses, 1);
    }

    #[test]
    fn a_game_missing_a_leader_is_dropped() {
        let mut ledger = MatchupLedger::default();
        let mut nameless = game(Some(Outcome::Won));
        nameless.their_leader = String::new();
        ledger.fold_in(&nameless, NOW);

        assert!(
            ledger.records.is_empty(),
            "a result that cannot be attributed to a matchup is not a data point"
        );
    }

    #[test]
    fn matchups_are_listed_most_played_first() {
        let mut ledger = MatchupLedger::default();
        ledger.fold_in(&game(Some(Outcome::Won)), NOW);
        let mut rare = game(Some(Outcome::Lost));
        rare.their_leader = "OP05-041".into();
        ledger.fold_in(&rare, NOW);
        ledger.fold_in(&game(Some(Outcome::Won)), NOW);

        let listed = ledger.records_for("ST01-001");
        assert_eq!(listed[0].their_leader, "OP17-079");
        assert_eq!(listed[0].played(), 2);
        assert_eq!(listed[1].their_leader, "OP05-041");
    }

    #[test]
    fn life_falling_to_zero_reads_as_a_loss() {
        let mut life = LifeTrack::default();
        life.observe(5, 5);
        life.observe(2, 4);
        life.observe(0, 4);

        assert_eq!(life.outcome(), Some(Outcome::Lost));
    }

    #[test]
    fn taking_their_last_life_reads_as_a_win() {
        let mut life = LifeTrack::default();
        life.observe(4, 5);
        life.observe(4, 0);

        assert_eq!(life.outcome(), Some(Outcome::Won));
    }

    #[test]
    fn a_game_still_being_played_has_no_outcome() {
        let mut life = LifeTrack::default();
        life.observe(5, 5);
        life.observe(3, 2);

        assert_eq!(life.outcome(), None);
    }

    #[test]
    fn zero_life_never_seen_above_zero_is_not_a_result() {
        // Connecting to a position the adapter has not populated yet, or to a
        // game already over, both look like this.
        let mut life = LifeTrack::default();
        life.observe(0, 0);
        life.observe(0, 0);

        assert_eq!(
            life.outcome(),
            None,
            "a HUD that just booted has not lost a game"
        );
    }

    #[test]
    fn one_side_arriving_before_the_other_is_not_a_win() {
        // The realistic startup case: your life resolves a beat before theirs,
        // so theirs reads zero without ever having been anything else. Believed
        // literally, that is a free win in the record on every launch.
        let mut life = LifeTrack::default();
        life.observe(5, 0);
        life.observe(5, 0);

        assert_eq!(life.outcome(), None);
    }

    #[test]
    fn one_side_arriving_before_the_other_is_not_a_loss() {
        let mut life = LifeTrack::default();
        life.observe(0, 5);
        life.observe(0, 5);

        assert_eq!(life.outcome(), None);
    }

    #[test]
    fn both_sides_at_zero_is_not_guessed_at() {
        let mut life = LifeTrack::default();
        life.observe(5, 5);
        life.observe(0, 0);

        assert_eq!(
            life.outcome(),
            None,
            "reading a coin flip as a result would put fiction in the record"
        );
    }
}
