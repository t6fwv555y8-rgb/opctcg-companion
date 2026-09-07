//! How a finished game actually went, written down so the next one is not
//! played blind.
//!
//! Matchup records keep the score. This keeps the shape of one game: how close
//! it was, when life moved, what you still had unspent. The notes are
//! implications of those readings, not guesses about cards nobody saw.

use crate::ledger::{OpenGame, Sighting, MAX_CARDS_PER_PROFILE};
use crate::matchup::Outcome;
use serde::{Deserialize, Serialize};

/// Recaps kept on disk. A couple of evenings of play, and older ones fall off.
pub const MAX_REVIEWS: usize = 25;

/// What you did in the game still open, accumulated the same way opponent
/// tempo is: one reading at a time, never reconstructed after the fact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayTrack {
    /// Cards you showed — board, trash, and any hand the page faced up.
    pub cards: Vec<Sighting>,
    /// Most characters you held at once.
    pub widest_board: u32,
    /// Life you took off them, read as a fall from their high-water mark.
    pub life_dealt: u32,
    /// Turn you first took a life.
    pub first_strike_turn: Option<u32>,
    /// DON still active on the last reading. At game-over, that is what you
    /// left on the table.
    pub leftover_don: u32,
    /// Cards still in hand on the last reading.
    pub leftover_hand: u32,
}

impl PlayTrack {
    /// Note `copies` of one of your cards visible on `turn`.
    pub fn record_card(&mut self, card_id: &str, copies: u32, turn: u32) {
        if card_id.is_empty() || copies == 0 {
            return;
        }
        match self.cards.iter_mut().find(|s| s.card_id == card_id) {
            Some(seen) => {
                seen.copies = seen.copies.max(copies);
                seen.first_turn = seen.first_turn.min(turn);
            }
            None => {
                if self.cards.len() >= MAX_CARDS_PER_PROFILE {
                    return;
                }
                self.cards.push(Sighting {
                    card_id: card_id.to_string(),
                    copies,
                    first_turn: turn,
                });
            }
        }
    }

    /// The last snapshot of your resources, plus the widest board seen so far.
    pub fn observe_end(&mut self, don: u32, hand: u32, board: u32) {
        self.leftover_don = don;
        self.leftover_hand = hand;
        self.widest_board = self.widest_board.max(board);
    }

    /// Life they have lost since their high-water mark.
    pub fn observe_strike(&mut self, dealt: u32, turn: u32) {
        if dealt > self.life_dealt {
            if self.first_strike_turn.is_none() {
                self.first_strike_turn = Some(turn);
            }
            self.life_dealt = dealt;
        }
    }
}

/// One finished game, as it should be shown after the table goes quiet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchReview {
    pub game_id: String,
    pub finished_at: String,
    pub outcome: Option<Outcome>,
    pub your_leader: String,
    pub your_leader_name: String,
    pub their_leader: String,
    pub their_leader_name: String,
    pub last_turn: u32,
    pub your_life: u32,
    pub their_life: u32,
    /// Life they took off you.
    pub your_life_lost: u32,
    /// Life you took off them.
    pub their_life_lost: u32,
    /// Turn you first lost life, when you did.
    pub first_damage_turn: Option<u32>,
    /// Turn you first took a life, when you did.
    pub first_strike_turn: Option<u32>,
    pub your_widest_board: u32,
    pub their_widest_board: u32,
    pub leftover_don: u32,
    pub leftover_hand: u32,
    /// Card ids you showed this game, earliest first.
    pub you_played: Vec<String>,
}

impl MatchReview {
    /// Build a recap from the game that just closed, or `None` when the
    /// position never counted as a game anyone played.
    pub fn from_open(game: &OpenGame, now: &str) -> Option<Self> {
        if !game.counts_as_played() {
            return None;
        }
        let mut you_played: Vec<String> =
            game.play.cards.iter().map(|s| s.card_id.clone()).collect();
        you_played.sort();
        Some(Self {
            game_id: game.game_id.clone(),
            finished_at: now.to_string(),
            outcome: game.outcome(),
            your_leader: game.your_leader_id.clone(),
            your_leader_name: game.your_leader_name.clone(),
            their_leader: game.leader_id.clone(),
            their_leader_name: game.leader_name.clone(),
            last_turn: game.tempo.last_turn,
            your_life: game.life.your_last,
            their_life: game.life.their_last,
            your_life_lost: game.life.your_high.saturating_sub(game.life.your_last),
            their_life_lost: game.life.their_high.saturating_sub(game.life.their_last),
            first_damage_turn: game.tempo.first_damage_turn,
            first_strike_turn: game.play.first_strike_turn,
            your_widest_board: game.play.widest_board,
            their_widest_board: game.tempo.widest_board,
            leftover_don: game.play.leftover_don,
            leftover_hand: game.play.leftover_hand,
            you_played,
        })
    }

    /// One line that says how the game ended.
    pub fn headline(&self) -> String {
        match self.outcome {
            Some(Outcome::Won) => format!(
                "Won on turn {} — {} life left",
                self.last_turn.max(1),
                self.your_life
            ),
            Some(Outcome::Lost) => format!(
                "Lost on turn {} — they had {} life",
                self.last_turn.max(1),
                self.their_life
            ),
            None => format!(
                "Game stopped on turn {} at {}–{}",
                self.last_turn.max(1),
                self.your_life,
                self.their_life
            ),
        }
    }

    /// What the readings imply about how you played. Each line is safe to show.
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();

        match self.outcome {
            Some(Outcome::Won) if self.your_life <= 1 => notes.push(
                "One life from a loss. Next game, keep a counter for the last swing.".into(),
            ),
            Some(Outcome::Won) if self.your_life >= 4 => {
                notes.push("Comfortable win — they never got the race going.".into());
            }
            Some(Outcome::Won) => notes.push(format!(
                "Finished on {} life. Not a steal, not a blowout.",
                self.your_life
            )),
            Some(Outcome::Lost) if self.their_life <= 1 => {
                notes.push("You had them to 1. The next game is about finishing.".into());
            }
            Some(Outcome::Lost) if self.their_life >= 4 => {
                notes.push("They stayed on high life. You never got the race going.".into());
            }
            Some(Outcome::Lost) => {
                notes.push(format!("They finished on {} life.", self.their_life));
            }
            None => notes.push(
                "No result was readable — a concede or a disconnect. Not counted as a win or a loss."
                    .into(),
            ),
        }

        if self.their_life_lost > 0 || self.your_life_lost > 0 {
            notes.push(format!(
                "Life taken: you {}, them {}.",
                self.their_life_lost, self.your_life_lost
            ));
        }

        match self.first_strike_turn {
            Some(turn) if turn >= 6 => notes.push(format!(
                "You first took life on turn {turn} — they dictated the early game."
            )),
            Some(turn) if turn <= 3 => {
                notes.push(format!("You punched first on turn {turn}."));
            }
            Some(_) => {}
            None if self.outcome == Some(Outcome::Lost) => {
                notes.push("You never took a life.".into());
            }
            None => {}
        }

        if let Some(turn) = self.first_damage_turn {
            if turn <= 3 && self.outcome == Some(Outcome::Lost) {
                notes.push(format!(
                    "They hit you on turn {turn}. The early damage decided it."
                ));
            }
        }

        if self.leftover_don >= 3 {
            notes.push(format!(
                "Ended with {} DON still up. That's a turn you didn't spend.",
                self.leftover_don
            ));
        }

        if self.leftover_hand >= 5 {
            notes.push(format!(
                "{} cards still in hand. Either you were holding, or the board never opened.",
                self.leftover_hand
            ));
        } else if self.leftover_hand == 0 && self.outcome == Some(Outcome::Lost) {
            notes.push(
                "Hand empty at the end — you spent everything and still came up short.".into(),
            );
        }

        if self.your_widest_board >= 3 && self.their_widest_board <= 1 {
            notes.push("You owned the board for most of it.".into());
        } else if self.their_widest_board >= 3 && self.your_widest_board <= 1 {
            notes.push("They owned the board. Wide boards are how this matchup runs away.".into());
        }

        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matchup::LifeTrack;

    const NOW: &str = "2026-01-01T00:00:00Z";

    fn open(outcome: Outcome) -> OpenGame {
        let mut life = LifeTrack::default();
        match outcome {
            Outcome::Won => {
                life.observe(4, 5);
                life.observe(2, 0);
            }
            Outcome::Lost => {
                life.observe(5, 5);
                life.observe(0, 3);
            }
        }
        let mut play = PlayTrack::default();
        play.record_card("ST01-002", 1, 3);
        play.observe_end(1, 2, 2);
        play.observe_strike(life.their_high.saturating_sub(life.their_last), 4);
        OpenGame {
            game_id: "game-1".into(),
            leader_id: "OP17-079".into(),
            leader_name: "Loki".into(),
            started_at: NOW.into(),
            sightings: Vec::new(),
            tempo: crate::ledger::Tempo {
                first_board_turn: Some(3),
                widest_board: 2,
                life_taken: life.your_high.saturating_sub(life.your_last),
                first_damage_turn: Some(4),
                last_turn: 7,
            },
            your_leader_id: "ST01-001".into(),
            your_leader_name: "Red Luffy".into(),
            life,
            play,
        }
    }

    #[test]
    fn a_win_leads_with_the_turn_and_life_left() {
        let review = MatchReview::from_open(&open(Outcome::Won), NOW).expect("played");
        assert_eq!(review.headline(), "Won on turn 7 — 2 life left");
        assert_eq!(review.outcome, Some(Outcome::Won));
        assert!(review.you_played.contains(&"ST01-002".into()));
    }

    #[test]
    fn a_close_loss_says_you_had_them() {
        let mut game = open(Outcome::Lost);
        game.life = LifeTrack::default();
        game.life.observe(4, 5);
        game.life.observe(0, 1);
        game.play.observe_strike(4, 5);
        let review = MatchReview::from_open(&game, NOW).unwrap();
        assert!(review.headline().starts_with("Lost on turn 7"));
        assert!(
            review
                .notes()
                .iter()
                .any(|n| n.contains("You had them to 1")),
            "{:?}",
            review.notes()
        );
    }

    #[test]
    fn leftover_don_is_called_out() {
        let mut game = open(Outcome::Lost);
        game.play.observe_end(4, 2, 1);
        let review = MatchReview::from_open(&game, NOW).unwrap();
        assert!(
            review.notes().iter().any(|n| n.contains("4 DON still up")),
            "{:?}",
            review.notes()
        );
    }

    #[test]
    fn an_idle_position_is_not_a_recap() {
        let idle = OpenGame {
            game_id: "idle".into(),
            leader_id: "OP17-079".into(),
            leader_name: String::new(),
            started_at: NOW.into(),
            sightings: Vec::new(),
            tempo: crate::ledger::Tempo::default(),
            your_leader_id: "ST01-001".into(),
            your_leader_name: String::new(),
            life: LifeTrack::default(),
            play: PlayTrack::default(),
        };
        assert!(
            MatchReview::from_open(&idle, NOW).is_none(),
            "an idle HUD must not invent a recap"
        );
    }

    #[test]
    fn a_blowout_win_is_named_as_comfortable() {
        let mut game = open(Outcome::Won);
        game.life = LifeTrack::default();
        game.life.observe(5, 5);
        game.life.observe(5, 0);
        let review = MatchReview::from_open(&game, NOW).unwrap();
        assert!(
            review.notes().iter().any(|n| n.contains("Comfortable win")),
            "{:?}",
            review.notes()
        );
    }
}
