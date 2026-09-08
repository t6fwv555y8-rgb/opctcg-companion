//! Standing orders for a matchup: what last games said to do differently.
//!
//! The ledger already records how a game went. This turns those measurements
//! into at most three durable lines, then keeps, rewrites, or drops each one
//! the next time the same pairing is played. The coach and the HUD read the
//! surviving lines; they do not invent a fourth.

use crate::matchup::Outcome;
use crate::review::MatchReview;
use serde::{Deserialize, Serialize};

/// How many orders a matchup is allowed to carry. More than this and Play
/// becomes a lecture.
pub const MAX_ORDERS: usize = 3;

/// Games a leak can stay gone before the order is dropped.
pub const DROP_AFTER_CLEAN: u32 = 2;

/// Auto-reads kept on the open game so the recap can colour an order.
pub const MAX_AUTO_READS: usize = 8;

/// One automatic read taken during a live game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoRead {
    pub turn: u32,
    pub line: String,
}

/// A durable instruction for this pairing of leaders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingOrder {
    /// Stable key so the next game can keep or drop this order.
    pub leak: String,
    pub text: String,
    #[serde(default)]
    pub source_game: String,
    /// Games this leak showed up, including the first.
    #[serde(default)]
    pub seen: u32,
    /// Consecutive games since the leak last appeared.
    #[serde(default)]
    pub clean: u32,
}

impl StandingOrder {
    fn fresh(leak: &str, text: &str, game_id: &str) -> Self {
        Self {
            leak: leak.to_string(),
            text: text.to_string(),
            source_game: game_id.to_string(),
            seen: 1,
            clean: 0,
        }
    }
}

/// Leaks visible in this recap, each with the line Rayleigh should keep saying.
pub fn leaks_from(review: &MatchReview, reads: &[AutoRead]) -> Vec<(String, String)> {
    let mut leaks = Vec::new();
    let them = if review.their_leader_name.trim().is_empty() {
        "this leader"
    } else {
        review.their_leader_name.trim()
    };

    if review.leftover_don >= 3 {
        leaks.push((
            "leftover_don".into(),
            format!(
                "Spend the DON against {them}. You have been leaving {} unspent.",
                review.leftover_don
            ),
        ));
    }
    if review.leftover_hand >= 5 {
        leaks.push((
            "held_hand".into(),
            format!(
                "Play or counter. You ended with {} cards in hand against {them}.",
                review.leftover_hand
            ),
        ));
    }
    if review.leftover_hand == 0 && review.outcome == Some(Outcome::Lost) {
        leaks.push((
            "empty_hand".into(),
            format!("Keep one card. You emptied the hand and still lost to {them}."),
        ));
    }
    match review.first_strike_turn {
        None if review.outcome == Some(Outcome::Lost) => leaks.push((
            "never_struck".into(),
            format!("Take a life earlier. You never punched {them}."),
        )),
        Some(turn) if turn >= 6 => leaks.push((
            "late_strike".into(),
            format!("Punch before turn {turn}. {them} dictates the early game."),
        )),
        _ => {}
    }
    if let Some(turn) = review.first_damage_turn {
        if turn <= 3 && review.outcome == Some(Outcome::Lost) {
            leaks.push((
                "early_damage".into(),
                format!(
                    "Stabilize the first three turns. {them} hit you on turn {turn} and it decided it."
                ),
            ));
        }
    }
    if review.their_widest_board >= 3 && review.your_widest_board <= 1 {
        leaks.push((
            "they_owned_board".into(),
            format!("Contest the board. {them} goes wide and you have been letting them."),
        ));
    }
    if review.your_widest_board >= 3
        && review.their_widest_board <= 1
        && review.outcome == Some(Outcome::Won)
    {
        leaks.push((
            "you_owned_board".into(),
            format!("Keep owning the board. Wide boards are how you beat {them}."),
        ));
    }
    match review.outcome {
        Some(Outcome::Lost) if review.their_life <= 1 => leaks.push((
            "close_loss".into(),
            format!("Finish. You had {them} to 1."),
        )),
        Some(Outcome::Lost) if review.their_life >= 4 => leaks.push((
            "blowout_loss".into(),
            format!("Get the race going. {them} stays on high life against you."),
        )),
        Some(Outcome::Won) if review.your_life <= 1 => leaks.push((
            "close_win".into(),
            format!("Keep a last counter. You keep winning this on 1 life."),
        )),
        _ => {}
    }

    if let Some(line) = reads.iter().rev().map(|r| r.line.as_str()).find(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("blocker") && review.outcome == Some(Outcome::Lost)
    }) {
        if !leaks.iter().any(|(k, _)| k == "their_blocker") {
            leaks.push((
                "their_blocker".into(),
                clip_order(&format!(
                    "Respect their blocker. Last game: {}",
                    first_sentence(line)
                )),
            ));
        }
    }

    leaks.truncate(6);
    leaks
}

/// Keep, rewrite, or drop existing orders from this recap, then fill empty slots.
pub fn refresh_orders(
    existing: &[StandingOrder],
    review: &MatchReview,
    reads: &[AutoRead],
) -> Vec<StandingOrder> {
    let incoming = leaks_from(review, reads);
    let game_id = review.game_id.as_str();
    let mut next = Vec::new();

    for order in existing {
        if let Some((_, text)) = incoming.iter().find(|(leak, _)| leak == &order.leak) {
            next.push(StandingOrder {
                leak: order.leak.clone(),
                text: text.clone(),
                source_game: game_id.to_string(),
                seen: order.seen.saturating_add(1),
                clean: 0,
            });
        } else {
            let clean = order.clean.saturating_add(1);
            if clean < DROP_AFTER_CLEAN {
                next.push(StandingOrder {
                    clean,
                    ..order.clone()
                });
            }
        }
    }

    for (leak, text) in incoming {
        if next.iter().any(|order| order.leak == leak) {
            continue;
        }
        next.push(StandingOrder::fresh(&leak, &text, game_id));
    }

    // New leaks outrank orders that are already on their last clean game.
    next.sort_by(|a, b| b.seen.cmp(&a.seen).then(a.clean.cmp(&b.clean)));
    next.truncate(MAX_ORDERS);
    next
}

fn first_sentence(line: &str) -> String {
    let cut = line
        .find(['.', '!', '?'])
        .map(|i| i + 1)
        .unwrap_or(line.len());
    line[..cut].trim().to_string()
}

fn clip_order(text: &str) -> String {
    const MAX: usize = 160;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    let end = trimmed
        .char_indices()
        .take(MAX.saturating_sub(1))
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(MAX);
    format!("{}…", &trimmed[..end])
}

/// Clip an automatic read so the open game cannot grow without bound.
pub fn clip_auto_read(line: &str) -> String {
    const MAX: usize = 200;
    let trimmed = line.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    let end = trimmed
        .char_indices()
        .take(MAX.saturating_sub(1))
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(MAX);
    format!("{}…", &trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matchup::Outcome;

    fn review(outcome: Outcome, don: u32) -> MatchReview {
        MatchReview {
            game_id: "g1".into(),
            finished_at: "2026-01-01T00:00:00Z".into(),
            outcome: Some(outcome),
            your_leader: "OP13-001".into(),
            your_leader_name: "Rayleigh".into(),
            their_leader: "OP17-079".into(),
            their_leader_name: "Loki".into(),
            last_turn: 8,
            your_life: 2,
            their_life: 0,
            your_life_lost: 3,
            their_life_lost: 5,
            first_damage_turn: Some(4),
            first_strike_turn: Some(4),
            your_widest_board: 2,
            their_widest_board: 2,
            leftover_don: don,
            leftover_hand: 2,
            you_played: vec!["OP13-079".into()],
        }
    }

    #[test]
    fn leftover_don_becomes_an_order() {
        let leaks = leaks_from(&review(Outcome::Won, 4), &[]);
        assert!(
            leaks.iter().any(|(k, t)| k == "leftover_don" && t.contains("4")),
            "{leaks:?}"
        );
    }

    #[test]
    fn a_repeated_leak_is_kept_and_rewritten() {
        let first = refresh_orders(&[], &review(Outcome::Won, 4), &[]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].seen, 1);

        let mut next_review = review(Outcome::Won, 5);
        next_review.game_id = "g2".into();
        let second = refresh_orders(&first, &next_review, &[]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].seen, 2);
        assert_eq!(second[0].clean, 0);
        assert!(second[0].text.contains('5'), "{}", second[0].text);
    }

    #[test]
    fn two_clean_games_drop_the_order() {
        let first = refresh_orders(&[], &review(Outcome::Won, 4), &[]);
        let mut clean = review(Outcome::Won, 0);
        clean.game_id = "g2".into();
        let held = refresh_orders(&first, &clean, &[]);
        assert_eq!(held.len(), 1, "one clean game still keeps it");
        assert_eq!(held[0].clean, 1);

        clean.game_id = "g3".into();
        let dropped = refresh_orders(&held, &clean, &[]);
        assert!(
            dropped.iter().all(|o| o.leak != "leftover_don"),
            "two clean games should drop it: {dropped:?}"
        );
    }

    #[test]
    fn a_matchup_keeps_at_most_three_orders() {
        let mut messy = review(Outcome::Lost, 4);
        messy.their_life = 4;
        messy.leftover_hand = 6;
        messy.first_strike_turn = None;
        messy.their_widest_board = 4;
        messy.your_widest_board = 0;
        messy.first_damage_turn = Some(2);
        let orders = refresh_orders(&[], &messy, &[]);
        assert!(orders.len() <= MAX_ORDERS, "{}", orders.len());
        assert!(!orders.is_empty());
    }

    #[test]
    fn a_blocker_read_on_a_loss_becomes_an_order() {
        let mut loss = review(Outcome::Lost, 0);
        loss.their_life = 2;
        let reads = vec![AutoRead {
            turn: 6,
            line: "Their blocker is up. Don't swing into it unless you can pay.".into(),
        }];
        let leaks = leaks_from(&loss, &reads);
        assert!(
            leaks.iter().any(|(k, _)| k == "their_blocker"),
            "{leaks:?}"
        );
    }
}
