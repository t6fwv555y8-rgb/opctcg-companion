//! Learning opponents across games.
//!
//! An opponent never hands over their list. What they do hand over, slowly and
//! without meaning to, is the part of it they need to play in order to win.
//! This crate writes those cards down game after game and turns the pile into
//! two readings: which cards a leader is actually running, and how the deck
//! tends to play.
//!
//! Nothing here claims certainty. Every reading carries the number of games
//! behind it, because the whole point is that the deck is unseen — a card
//! showing up in four of five games is worth planning around, and the same card
//! showing up once is not.
//!
//! The other half of the crate points the same idea at your own deck. Which
//! cards an opponent runs is worth knowing; what happens when your fifty meet
//! theirs is worth more, and no amount of reasoning about a matchup beats
//! having played it ten times. So results are recorded too, per pairing of
//! leaders, with the same refusal to overstate a small sample.

pub mod ledger;
pub mod matchup;
pub mod report;
pub mod scout;

pub use ledger::{
    CardRecord, LeaderProfile, OpenGame, ScoutingLedger, Sighting, Tempo, TempoRecord,
    LEDGER_VERSION, MAX_CARDS_PER_PROFILE, MAX_PROFILES,
};
pub use matchup::{LifeTrack, MatchupLedger, MatchupRecord, Outcome, MAX_MATCHUPS};
pub use report::{
    DeckMap, MappedCard, MatchupRead, Pace, Reliability, Standing, StrategyRead,
    MIN_GAMES_FOR_PACE, MIN_GAMES_FOR_STANDING, STAPLE_CONFIDENCE,
};
pub use scout::Scout;
