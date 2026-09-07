//! Turning sightings into something a coach can use.
//!
//! A profile is a pile of counts. What a player needs is two readings from it:
//! which cards this leader is actually running, and how the deck tends to play.
//! Both come with the number of games behind them, because advice drawn from
//! one game and advice drawn from twenty are not the same advice.

use crate::ledger::LeaderProfile;
use crate::matchup::MatchupRecord;
use serde::{Deserialize, Serialize};

/// Games needed before a pace is called at all. Below this the measurements are
/// one opponent's draw rather than a deck's character.
pub const MIN_GAMES_FOR_PACE: u32 = 3;

/// Share of games a card must appear in to be called part of the deck's spine.
pub const STAPLE_CONFIDENCE: f32 = 0.6;

/// Average turn of first damage at or below which a deck is racing you.
const AGGRESSIVE_FIRST_DAMAGE_TURN: f32 = 3.5;

/// Average final turn at or above which a deck is playing the long game.
const GRINDY_LAST_TURN: f32 = 9.0;

/// How much of a profile to trust, in the plainest terms available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reliability {
    /// One or two games. Suggestive, no more.
    Thin,
    /// Enough games that repeated cards mean something.
    Fair,
    /// Enough games to plan around.
    Solid,
}

impl Reliability {
    pub fn of(games: u32) -> Self {
        match games {
            0..=2 => Self::Thin,
            3..=5 => Self::Fair,
            _ => Self::Solid,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Thin => "thin",
            Self::Fair => "fair",
            Self::Solid => "solid",
        }
    }
}

/// How a deck has actually played, as opposed to what its leader is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    /// Too few games to say.
    Unknown,
    /// Damage early and often.
    Aggressive,
    /// Neither racing nor stalling.
    Midrange,
    /// Content to go long.
    Grindy,
}

impl Pace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "not yet established",
            Self::Aggressive => "aggressive",
            Self::Midrange => "midrange",
            Self::Grindy => "grindy",
        }
    }
}

/// One card in a mapped deck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappedCard {
    pub card_id: String,
    pub games_seen: u32,
    /// Share of games this card appeared in, between 0 and 1.
    pub confidence: f32,
    /// Most copies ever seen at once: a floor on how many they run.
    pub likely_copies: u32,
    pub earliest_turn: u32,
}

impl MappedCard {
    pub fn is_staple(&self) -> bool {
        self.confidence >= STAPLE_CONFIDENCE
    }
}

/// A deck rebuilt from what has been seen of it, most reliable card first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckMap {
    pub leader_id: String,
    pub leader_name: String,
    pub games: u32,
    pub reliability: Reliability,
    pub cards: Vec<MappedCard>,
}

impl DeckMap {
    /// Build a map from a leader's history, or `None` when there is none.
    pub fn from_profile(profile: &LeaderProfile) -> Option<Self> {
        if profile.games == 0 || profile.cards.is_empty() {
            return None;
        }
        let games = profile.games as f32;
        let mut cards: Vec<MappedCard> = profile
            .cards
            .iter()
            .map(|record| MappedCard {
                card_id: record.card_id.clone(),
                games_seen: record.games_seen,
                confidence: (record.games_seen as f32 / games).clamp(0.0, 1.0),
                likely_copies: record.max_copies,
                earliest_turn: record.earliest_turn,
            })
            .collect();
        // Most-established first, with ties broken by id so the ordering is
        // stable between runs and the prompt does not churn.
        cards.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.likely_copies.cmp(&a.likely_copies))
                .then(a.card_id.cmp(&b.card_id))
        });

        Some(Self {
            leader_id: profile.leader_id.clone(),
            leader_name: profile.leader_name.clone(),
            games: profile.games,
            reliability: Reliability::of(profile.games),
            cards,
        })
    }

    /// The cards that show up often enough to plan around.
    pub fn staples(&self) -> impl Iterator<Item = &MappedCard> {
        self.cards.iter().filter(|card| card.is_staple())
    }

    /// Copies accounted for by the staples, against the fifty in a deck.
    ///
    /// This is the honest headline for a map: "we can name 22 of their 50".
    pub fn mapped_copies(&self) -> u32 {
        self.staples().map(|card| card.likely_copies).sum()
    }
}

/// How a deck plays, with the measurements behind the claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrategyRead {
    pub games: u32,
    pub reliability: Reliability,
    pub pace: Pace,
    /// Plain statements of what was measured, each safe to show a user.
    pub notes: Vec<String>,
}

impl StrategyRead {
    pub fn from_profile(profile: &LeaderProfile) -> Option<Self> {
        if profile.games == 0 {
            return None;
        }
        let games = profile.games as f32;
        let tempo = &profile.tempo;

        let average_first_damage = (tempo.games_with_damage > 0)
            .then(|| tempo.summed_first_damage_turn as f32 / tempo.games_with_damage as f32);
        let average_first_board = (tempo.games_with_board > 0)
            .then(|| tempo.summed_first_board_turn as f32 / tempo.games_with_board as f32);
        let average_last_turn = tempo.summed_last_turn as f32 / games;
        let average_life_taken = tempo.summed_life_taken as f32 / games;

        let pace = if profile.games < MIN_GAMES_FOR_PACE {
            Pace::Unknown
        } else if average_first_damage.is_some_and(|turn| turn <= AGGRESSIVE_FIRST_DAMAGE_TURN) {
            Pace::Aggressive
        } else if average_last_turn >= GRINDY_LAST_TURN {
            Pace::Grindy
        } else {
            Pace::Midrange
        };

        let mut notes = Vec::new();
        if let Some(turn) = average_first_board {
            notes.push(format!("First character on turn {turn:.1} on average."));
        }
        if let Some(turn) = average_first_damage {
            notes.push(format!(
                "First takes life on turn {turn:.1}, in {} of {} games.",
                tempo.games_with_damage, profile.games
            ));
        } else {
            notes.push("Has never taken life off you.".to_string());
        }
        if average_life_taken > 0.0 {
            notes.push(format!("Takes {average_life_taken:.1} life per game."));
        }
        if tempo.widest_board > 0 {
            notes.push(format!(
                "Widest board seen: {} characters.",
                tempo.widest_board
            ));
        }
        if average_last_turn > 0.0 {
            notes.push(format!("Games run to turn {average_last_turn:.1}."));
        }

        Some(Self {
            games: profile.games,
            reliability: Reliability::of(profile.games),
            pace,
            notes,
        })
    }
}

/// Which way a matchup has gone, once enough games say anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// Too few finished games to call.
    TooEarly,
    Favourable,
    Even,
    Rough,
}

impl Standing {
    pub fn label(self) -> &'static str {
        match self {
            Self::TooEarly => "too early to call",
            Self::Favourable => "favourable",
            Self::Even => "even",
            Self::Rough => "rough",
        }
    }
}

/// Finished games needed before a win rate is characterised at all.
///
/// Three games is a coin landing the same way twice. Calling a matchup
/// "favourable" off that would be the single most misleading thing this crate
/// could tell someone, since they would then build around it.
pub const MIN_GAMES_FOR_STANDING: u32 = 5;

/// Win rate at or above which a matchup is called favourable.
const FAVOURABLE_RATE: f32 = 0.6;

/// Win rate at or below which it is called rough.
const ROUGH_RATE: f32 = 0.4;

/// How your deck has actually fared against one leader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchupRead {
    pub their_leader: String,
    pub their_leader_name: String,
    pub wins: u32,
    pub losses: u32,
    pub unfinished: u32,
    pub standing: Standing,
    /// `None` until a game has actually finished.
    pub win_rate: Option<f32>,
    /// Plain statements of what was measured, each safe to show a user.
    pub notes: Vec<String>,
}

impl MatchupRead {
    /// Read a record, or `None` when no game has been played.
    pub fn from_record(record: &MatchupRecord) -> Option<Self> {
        if record.played() == 0 {
            return None;
        }
        let finished = record.finished();
        let win_rate = record.win_rate();

        let standing = match win_rate {
            Some(rate) if finished >= MIN_GAMES_FOR_STANDING => {
                if rate >= FAVOURABLE_RATE {
                    Standing::Favourable
                } else if rate <= ROUGH_RATE {
                    Standing::Rough
                } else {
                    Standing::Even
                }
            }
            _ => Standing::TooEarly,
        };

        let mut notes = Vec::new();
        if finished > 0 {
            notes.push(format!(
                "You are {}-{} in {} finished game{}.",
                record.wins,
                record.losses,
                finished,
                if finished == 1 { "" } else { "s" }
            ));
        }
        if standing == Standing::TooEarly && finished > 0 {
            notes.push(format!(
                "Too few games to call the matchup; {} needed.",
                MIN_GAMES_FOR_STANDING.saturating_sub(finished).max(1)
            ));
        }
        if let Some(turn) = record.average_length() {
            notes.push(format!("Games run to turn {turn:.1}."));
        }
        if let Some(life) = record.average_life_left_on_win() {
            notes.push(format!("When you win, you finish on {life:.1} life."));
        }
        if let Some(life) = record.average_their_life_left_on_loss() {
            // The difference between losing a close game and never being in it
            // is the difference between tuning a deck and changing it.
            let closeness = if life >= 3.0 {
                " — not close"
            } else {
                " — close games"
            };
            notes.push(format!(
                "When you lose, they finish on {life:.1} life{closeness}."
            ));
        }
        if record.unfinished > 0 {
            notes.push(format!(
                "{} game{} ended without a result and {} not counted.",
                record.unfinished,
                if record.unfinished == 1 { "" } else { "s" },
                if record.unfinished == 1 { "is" } else { "are" }
            ));
        }

        Some(Self {
            their_leader: record.their_leader.clone(),
            their_leader_name: record.their_leader_name.clone(),
            wins: record.wins,
            losses: record.losses,
            unfinished: record.unfinished,
            standing,
            win_rate,
            notes,
        })
    }

    /// The record as it would be written on a sheet: `3-2`.
    pub fn score(&self) -> String {
        format!("{}-{}", self.wins, self.losses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{ScoutingLedger, Tempo};

    /// A profile built by playing `games` games, seeing `always` every game and
    /// `once` only in the first.
    fn profile(games: u32, always: &[&str], once: &[&str]) -> LeaderProfile {
        let mut ledger = ScoutingLedger::default();
        for n in 0..games {
            ledger.begin_game(&format!("g{n}"), "OP17-079", "Monkey.D.Luffy", "2026-01-01");
            for card in always {
                ledger.record_card(card, 2, 4);
            }
            if n == 0 {
                for card in once {
                    ledger.record_card(card, 1, 6);
                }
            }
            ledger.record_tempo(Tempo {
                first_board_turn: Some(2),
                widest_board: 4,
                life_taken: 3,
                first_damage_turn: Some(3),
                last_turn: 7,
            });
        }
        ledger.close_open_game("2026-01-02");
        ledger.profile("OP17-079").cloned().expect("profile built")
    }

    #[test]
    fn nothing_is_mapped_without_a_game() {
        let empty = LeaderProfile {
            leader_id: "OP17-079".into(),
            leader_name: "Monkey.D.Luffy".into(),
            games: 0,
            first_seen: "2026-01-01".into(),
            last_seen: "2026-01-01".into(),
            cards: Vec::new(),
            tempo: Default::default(),
        };

        assert!(DeckMap::from_profile(&empty).is_none());
        assert!(StrategyRead::from_profile(&empty).is_none());
    }

    #[test]
    fn a_card_seen_every_game_maps_at_full_confidence() {
        let map = DeckMap::from_profile(&profile(4, &["OP17-080"], &[])).unwrap();

        let card = &map.cards[0];
        assert_eq!(card.card_id, "OP17-080");
        assert_eq!(card.games_seen, 4);
        assert!((card.confidence - 1.0).abs() < f32::EPSILON);
        assert_eq!(card.likely_copies, 2);
    }

    #[test]
    fn a_card_seen_once_in_many_games_is_not_a_staple() {
        let map = DeckMap::from_profile(&profile(5, &["OP17-080"], &["OP05-094"])).unwrap();

        let one_off = map
            .cards
            .iter()
            .find(|c| c.card_id == "OP05-094")
            .expect("still recorded");
        assert!(
            !one_off.is_staple(),
            "one appearance in five games is not the deck's spine: {}",
            one_off.confidence
        );
        assert!(map.staples().all(|card| card.card_id == "OP17-080"));
    }

    #[test]
    fn staples_come_first_and_ties_are_stable() {
        let map =
            DeckMap::from_profile(&profile(3, &["OP17-081", "OP17-080"], &["OP05-094"])).unwrap();

        assert_eq!(
            map.cards
                .iter()
                .map(|c| c.card_id.as_str())
                .collect::<Vec<_>>(),
            vec!["OP17-080", "OP17-081", "OP05-094"],
            "equal confidence should order by id so the prompt does not churn"
        );
    }

    #[test]
    fn the_map_reports_how_much_of_the_deck_it_can_name() {
        let map = DeckMap::from_profile(&profile(4, &["OP17-080", "OP17-081"], &[])).unwrap();

        assert_eq!(
            map.mapped_copies(),
            4,
            "two staples at two copies each is four of their fifty"
        );
    }

    #[test]
    fn one_game_is_labelled_thin_and_refuses_to_call_a_pace() {
        let read = StrategyRead::from_profile(&profile(1, &["OP17-080"], &[])).unwrap();

        assert_eq!(read.reliability, Reliability::Thin);
        assert_eq!(
            read.pace,
            Pace::Unknown,
            "one game is a draw, not a deck's character"
        );
    }

    #[test]
    fn a_deck_that_hits_early_every_game_reads_as_aggressive() {
        let read = StrategyRead::from_profile(&profile(4, &["OP17-080"], &[])).unwrap();

        assert_eq!(read.pace, Pace::Aggressive);
        assert_eq!(read.reliability, Reliability::Fair);
        assert!(
            read.notes.iter().any(|n| n.contains("turn 3.0")),
            "the measurement behind the call should be stated: {:?}",
            read.notes
        );
    }

    #[test]
    fn a_deck_that_never_presses_reads_as_grindy() {
        let mut ledger = ScoutingLedger::default();
        for n in 0..4 {
            ledger.begin_game(&format!("g{n}"), "OP17-079", "Luffy", "2026-01-01");
            ledger.record_card("OP17-080", 1, 5);
            ledger.record_tempo(Tempo {
                first_board_turn: Some(4),
                widest_board: 2,
                life_taken: 0,
                first_damage_turn: None,
                last_turn: 11,
            });
        }
        ledger.close_open_game("2026-01-02");

        let read = StrategyRead::from_profile(ledger.profile("OP17-079").unwrap()).unwrap();
        assert_eq!(read.pace, Pace::Grindy);
        assert!(
            read.notes.iter().any(|n| n.contains("never taken life")),
            "an absence is a measurement too: {:?}",
            read.notes
        );
    }

    #[test]
    fn reliability_grows_with_games_played() {
        assert_eq!(Reliability::of(0), Reliability::Thin);
        assert_eq!(Reliability::of(2), Reliability::Thin);
        assert_eq!(Reliability::of(3), Reliability::Fair);
        assert_eq!(Reliability::of(5), Reliability::Fair);
        assert_eq!(Reliability::of(6), Reliability::Solid);
    }
}
