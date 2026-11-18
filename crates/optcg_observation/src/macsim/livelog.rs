//! Following the game through the log panel on the simulator's screen.
//!
//! The panel shows the last dozen or so lines of the game log. Read with text
//! recognition on a timer, it gives overlapping snapshots of a scrolling list.
//! [`LogFeed`] turns those into a stream of new messages, and [`LiveGame`]
//! turns messages into what the HUD needs: Leaders, whose turn it is, combat,
//! and how the game ended.
//!
//! Only public information is used. The log can name the opponent's hand or
//! what they drew; those lines are skipped, and the opponent's tag is kept only
//! in memory to tell the two players apart.

use crate::bridge_protocol::{BrowserCombatSnapshot, BrowserGameSnapshot, BrowserPlayerSnapshot, ObservedCard};

/// Lowercase letters and digits only, so small recognition differences do not matter.
fn key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_tag_prefix(line: &str) -> bool {
    // "[Name#12345] ..." starts a message; "[OP17-082]" does not.
    let Some(rest) = line.strip_prefix('[') else {
        return false;
    };
    rest.find(']').is_some_and(|end| rest[..end].contains('#'))
}

/// Lines that begin a message of their own even without a "[player]" prefix.
fn is_standalone(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("attack fails")
        || lower.starts_with("version is")
        || lower.ends_with("has connected")
        || lower.contains("] vs ")
        || lower.contains(" hit for ")
        || lower.starts_with("waiting for a connection")
}

/// "[OP09- 089]" (an id the panel wrapped mid-way) -> "[OP09-089]".
fn fix_split_ids(message: &str) -> String {
    let chars: Vec<char> = message.chars().collect();
    let mut out = String::with_capacity(message.len());
    let mut i = 0;
    while i < chars.len() {
        out.push(chars[i]);
        let is_split = chars[i] == '-'
            && i > 0
            && chars[i - 1].is_ascii_alphanumeric()
            && chars.get(i + 1) == Some(&' ')
            && chars.get(i + 2..i + 5).is_some_and(|d| d.iter().all(|c| c.is_ascii_digit()));
        if is_split {
            i += 1; // drop the stray space
        }
        i += 1;
    }
    out
}

/// Join lines that were wrapped by the panel back into whole messages.
pub fn merge_wrapped(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if is_tag_prefix(line) || is_standalone(line) || out.is_empty() {
            out.push(line.to_string());
        } else if let Some(last) = out.last_mut() {
            last.push(' ');
            last.push_str(line);
        }
    }
    out.iter().map(|m| fix_split_ids(m)).collect()
}

/// Turns repeated snapshots of a scrolling list into the new lines only.
#[derive(Debug, Default)]
pub struct LogFeed {
    seen: Vec<String>,
}

impl LogFeed {
    /// `window` is what the panel shows now (oldest first). Returns what is new
    /// since the last call, matching the end of what was already seen against
    /// the start of the window.
    pub fn update(&mut self, window: Vec<String>) -> Vec<String> {
        let keys: Vec<String> = window.iter().map(|m| key(m)).collect();
        let old: Vec<String> = self.seen.iter().map(|m| key(m)).collect();
        let max = old.len().min(keys.len());
        let overlap = (0..=max)
            .rev()
            .find(|&k| old[old.len() - k..] == keys[..k])
            .unwrap_or(0);
        let fresh: Vec<String> = window[overlap..].to_vec();
        self.seen.extend(fresh.iter().cloned());
        // Remember only what could still be on screen.
        let keep = 80;
        if self.seen.len() > keep {
            self.seen.drain(..self.seen.len() - keep);
        }
        fresh
    }
}

/// Every card id in a message, in order ("OP17-082").
fn card_ids(text: &str) -> Vec<String> {
    crate::art::card_id_candidates(text)
}

fn tag_and_rest(message: &str) -> Option<(String, &str)> {
    let rest = message.strip_prefix('[')?;
    let end = rest.find(']')?;
    let tag = &rest[..end];
    tag.contains('#').then(|| (normalize_tag(tag), rest[end + 1..].trim_start()))
}

pub fn normalize_tag(tag: &str) -> String {
    tag.chars()
        .filter(|c| !matches!(c, '\u{200b}' | '\u{200c}' | '\u{feff}'))
        .collect::<String>()
        .trim()
        .to_lowercase()
}

#[derive(Debug, Clone, Default)]
struct Side {
    leader: Option<String>,
    /// Cards this player has played or revealed (public).
    seen: Vec<String>,
}

/// The game as far as the log has told us.
#[derive(Debug, Clone, Default)]
pub struct LiveGame {
    /// Your tag, once known (from the name label on the screen).
    me: Option<String>,
    sides: std::collections::HashMap<String, Side>,
    /// Who chose turn order, and what they chose.
    chose: Option<(String, bool)>,
    /// How many turns have ended.
    turns_ended: u32,
    /// Whose turn it is by the log (a tag), when known.
    active: Option<String>,
    attacker_owner: Option<String>,
    attacker: Option<String>,
    target: Option<String>,
    attacker_power: Option<u32>,
    target_power: Option<u32>,
    winner: Option<String>,
    conceded: Option<String>,
}

impl LiveGame {
    pub fn set_me(&mut self, tag: &str) {
        let tag = normalize_tag(tag);
        if !tag.is_empty() {
            self.me = Some(tag);
        }
    }

    pub fn me(&self) -> Option<&str> {
        self.me.as_deref()
    }

    /// Feed one new message.
    pub fn apply(&mut self, message: &str) {
        let lower = message.to_lowercase();
        // Combat lines have no player prefix.
        if lower.contains("] vs ") {
            let ids = card_ids(message);
            let powers: Vec<u32> = message
                .split('[')
                .filter_map(|part| part.split(']').next())
                .filter_map(|p| p.trim().parse::<u32>().ok())
                .collect();
            if let (Some(a), Some(b)) = (ids.first(), ids.get(1)) {
                self.attacker = Some(a.clone());
                self.target = Some(b.clone());
            }
            self.attacker_power = powers.first().copied();
            self.target_power = powers.get(1).copied();
            return;
        }
        if lower.starts_with("attack fails") || lower.contains(" hit for ") {
            self.clear_combat();
            return;
        }
        let Some((tag, rest)) = tag_and_rest(message) else {
            return;
        };
        let side = self.sides.entry(tag.clone()).or_default();
        let rest_lower = rest.to_lowercase();
        if rest_lower.starts_with("leader is") {
            if let Some(id) = card_ids(rest).into_iter().next() {
                side.leader = Some(id.clone());
                push_unique(&mut side.seen, id);
            }
        } else if let Some(choice) = rest_lower.strip_prefix("chose to go ") {
            if self.chose.is_none() {
                self.chose = Some((tag, choice.trim().starts_with("first")));
            }
        } else if rest_lower.starts_with("end turn") {
            self.turns_ended += 1;
            self.clear_combat();
            // The turn passes to the other player.
            self.active = self.other_than(&tag);
        } else if rest_lower.starts_with("draw 1 card") || rest_lower.starts_with("draw 2 don") || rest_lower.starts_with("draw 1 don") {
            self.active = Some(tag);
        } else if rest_lower.starts_with("deploy ") {
            if let Some(id) = card_ids(rest).into_iter().next() {
                push_unique(&mut self.sides.entry(tag).or_default().seen, id);
            }
        } else if rest_lower.contains(" attacking ") {
            let ids = card_ids(rest);
            self.attacker_owner = Some(tag);
            self.attacker = ids.first().cloned();
            self.target = ids.get(1).cloned();
            self.attacker_power = None;
            self.target_power = None;
        } else if rest_lower.starts_with("wins!") {
            self.winner = Some(tag);
        } else if rest_lower.starts_with("concedes!") {
            self.conceded = Some(tag);
        }
    }

    fn clear_combat(&mut self) {
        self.attacker_owner = None;
        self.attacker = None;
        self.target = None;
        self.attacker_power = None;
        self.target_power = None;
    }

    fn other_than(&self, tag: &str) -> Option<String> {
        self.sides.keys().find(|t| t.as_str() != tag).cloned()
    }

    fn is_me(&self, tag: &str) -> bool {
        self.me.as_deref() == Some(tag)
    }

    /// Whose turn it is as the screen shows it ("End Turn" button visible means
    /// yours), which beats counting messages.
    pub fn snapshot(&self, screen_says_my_turn: Option<bool>, screen_turn_number: Option<u32>) -> BrowserGameSnapshot {
        let my_tag = self.me.clone();
        let opp_tag = self.sides.keys().find(|t| Some(t.as_str()) != my_tag.as_deref()).cloned();
        let side_of = |tag: &Option<String>| -> BrowserPlayerSnapshot {
            let side = tag.as_ref().and_then(|t| self.sides.get(t));
            BrowserPlayerSnapshot {
                leader_id: side.and_then(|s| s.leader.clone()),
                known_cards: side.map(|s| s.seen.clone()).unwrap_or_default(),
                ..Default::default()
            }
        };

        let active = match screen_says_my_turn {
            Some(true) => Some("self".to_string()),
            Some(false) => Some("opponent".to_string()),
            None => self.active.as_ref().map(|t| if self.is_me(t) { "self" } else { "opponent" }.to_string()),
        };

        let combat = self.attacker.as_ref().map(|attacker| {
            let owner = self.attacker_owner.as_ref().map(|t| if self.is_me(t) { "self" } else { "opponent" }.to_string());
            let defender_leader = match owner.as_deref() {
                Some("self") => opp_tag.as_ref().and_then(|t| self.sides.get(t)).and_then(|s| s.leader.clone()),
                Some(_) => my_tag.as_ref().and_then(|t| self.sides.get(t)).and_then(|s| s.leader.clone()),
                None => None,
            };
            let target_is_leader = self.target.as_ref().map(|t| Some(t) == defender_leader.as_ref());
            BrowserCombatSnapshot {
                attacker: Some(ObservedCard { card_id: Some(attacker.clone()), ..Default::default() }),
                target: self.target.as_ref().map(|t| ObservedCard { card_id: Some(t.clone()), ..Default::default() }),
                displayed_power: self.attacker_power,
                attacker_owner: owner.clone(),
                target_owner: owner.as_deref().map(|o| if o == "self" { "opponent" } else { "self" }.to_string()),
                target_is_leader,
            }
        });

        let result = match (&self.winner, &self.conceded) {
            (Some(w), _) => Some(if self.is_me(w) { "win" } else { "loss" }),
            (None, Some(c)) => Some(if self.is_me(c) { "loss" } else { "win" }),
            _ => None,
        };

        BrowserGameSnapshot {
            timestamp: 0,
            source: Some("optcgsim".into()),
            turn: screen_turn_number.or(Some(self.turns_ended + 1)),
            active_player: active.and_then(|a| match a.as_str() {
                "self" => Some("self".to_string()),
                _ => Some("opponent".to_string()),
            }),
            self_player: Some(side_of(&my_tag)),
            opponent: Some(side_of(&opp_tag)),
            combat,
            game_over: Some(result.is_some() || self.conceded.is_some() || self.winner.is_some()),
            result: result.map(str::to_string),
            ..Default::default()
        }
    }

    /// Who chose to go first, from your side: `Some(true)` if you go first.
    pub fn you_go_first(&self) -> Option<bool> {
        let (who, first) = self.chose.as_ref()?;
        Some(if self.is_me(who) { *first } else { !*first })
    }
}

fn push_unique(into: &mut Vec<String>, id: String) {
    if !into.contains(&id) {
        into.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn wrapped_lines_are_joined_and_standalone_lines_are_kept_apart() {
        let screen = lines(&[
            "[Me#1] Leader is Monkey",
            "D. Luffy [OP17-079]",
            "[Me#1] Sanji [OP17-082]:",
            "Trash Thousand Sunny [ST14-017]",
            "Nico Robin [OP09-062][6000] vs Kid [OP10-099][7000]",
            "Attack Fails",
        ]);
        let merged = merge_wrapped(&screen);
        assert_eq!(merged.len(), 4, "{merged:?}");
        assert_eq!(merged[0], "[Me#1] Leader is Monkey D. Luffy [OP17-079]");
        assert!(merged[1].ends_with("Trash Thousand Sunny [ST14-017]"));
        assert_eq!(merged[3], "Attack Fails");
    }

    #[test]
    fn a_card_id_split_across_two_lines_is_put_back_together() {
        let merged = merge_wrapped(&lines(&["[Foe#9] Laffitte [OP09-095]: Reveal and Draw Stronger [OP09-", "089]"]));
        assert!(merged[0].contains("[OP09-089]"), "{merged:?}");
        assert_eq!(merge_wrapped(&lines(&["[A#1] Trash 2 - 3 cards"]))[0], "[A#1] Trash 2 - 3 cards", "ordinary dashes are left alone");
    }

    #[test]
    fn a_scrolling_window_yields_only_what_is_new() {
        let mut feed = LogFeed::default();
        let a = lines(&["one", "two", "three"]);
        assert_eq!(feed.update(a.clone()).len(), 3, "first look shows everything");
        assert!(feed.update(a).is_empty(), "same screen again: nothing new");
        assert_eq!(feed.update(lines(&["two", "three", "four", "five"])), lines(&["four", "five"]));
        // Repeated lines are handled by matching the whole overlap, not single lines.
        let mut feed = LogFeed::default();
        feed.update(lines(&["A", "End Turn", "End Turn"]));
        assert_eq!(feed.update(lines(&["End Turn", "End Turn", "End Turn"])), lines(&["End Turn"]));
    }

    #[test]
    fn small_recognition_differences_do_not_create_duplicates() {
        let mut feed = LogFeed::default();
        feed.update(lines(&["[Me#1] Draw 1 Card", "[Me#1] Deploy Sanji [OP17-082]"]));
        let again = lines(&["[Me#1]  Draw 1 Card", "[Me#1] Deploy Sanji [OP17-082]", "[Me#1] End Turn"]);
        assert_eq!(feed.update(again), lines(&["[Me#1] End Turn"]));
    }

    #[test]
    fn the_game_reads_leaders_turn_and_combat_from_messages() {
        let mut game = LiveGame::default();
        game.set_me("Me#1");
        for m in [
            "[Me#1] Leader is Monkey D. Luffy [OP17-079]",
            "[Foe#9] Leader is Marshall D. Teach [OP09-081]",
            "[Me#1] Chose to go Second",
            "[Foe#9] Draw 1 Card",
            "[Foe#9] End Turn",
            "[Me#1] Draw 1 Card",
            "[Me#1] Deploy Sanji [OP17-082]",
            "[Me#1] Monkey D. Luffy [OP17-079] attacking Marshall D. Teach [OP09-081]",
            "Monkey D. Luffy [OP17-079][6000] vs Marshall D. Teach [OP09-081][5000]",
        ] {
            game.apply(m);
        }
        let snap = game.snapshot(None, None);
        assert_eq!(snap.self_player.as_ref().unwrap().leader_id.as_deref(), Some("OP17-079"));
        assert_eq!(snap.opponent.as_ref().unwrap().leader_id.as_deref(), Some("OP09-081"));
        assert_eq!(snap.active_player.as_deref(), Some("self"));
        assert_eq!(snap.turn, Some(2), "one turn has ended");
        assert_eq!(game.you_go_first(), Some(false));
        let combat = snap.combat.unwrap();
        assert_eq!(combat.attacker_owner.as_deref(), Some("self"));
        assert_eq!(combat.target_owner.as_deref(), Some("opponent"));
        assert_eq!(combat.target_is_leader, Some(true));
        assert_eq!(combat.displayed_power, Some(6000));
        assert!(snap.self_player.unwrap().known_cards.contains(&"OP17-082".to_string()));
    }

    #[test]
    fn the_screen_beats_the_log_for_turn_number_and_turn_owner() {
        let mut game = LiveGame::default();
        game.set_me("Me#1");
        game.apply("[Foe#9] End Turn");
        let snap = game.snapshot(Some(true), Some(7));
        assert_eq!(snap.turn, Some(7));
        assert_eq!(snap.active_player.as_deref(), Some("self"));
    }

    #[test]
    fn combat_ends_with_the_hit_or_the_failed_attack_and_the_game_ends_on_a_concession() {
        let mut game = LiveGame::default();
        game.set_me("Me#1");
        game.apply("[Foe#9] Teach [OP09-081] attacking Luffy [OP17-079]");
        assert!(game.snapshot(None, None).combat.is_some());
        game.apply("Attack Fails");
        assert!(game.snapshot(None, None).combat.is_none());
        game.apply("[Foe#9] Concedes!");
        let snap = game.snapshot(None, None);
        assert_eq!((snap.game_over, snap.result.as_deref()), (Some(true), Some("win")));
    }
}
