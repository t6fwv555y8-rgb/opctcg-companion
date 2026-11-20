//! Reading OPTCGSim's saved combat logs, for your match history.
//!
//! OPTCGSim saves a text log of each game. The log is written for debugging and
//! contains hidden information about the opponent (their hand, what they
//! drew). This reader deliberately ignores all of it. It keeps only what both
//! players could see (Leaders, cards on the board and in the trash, attacks)
//! plus your own cards, and it never stores the opponent's player name.

use std::collections::HashSet;

/// What a finished (or abandoned) game boils down to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGame {
    pub your_leader: String,
    pub opponent_leader: String,
    /// `None` when the log does not say who chose turn order.
    pub went_first: Option<bool>,
    pub turns: u32,
    /// "win" or "loss" when the log says so (a win or a concession).
    pub result: Option<String>,
    pub your_cards_seen: Vec<String>,
    /// Cards the opponent played to the board or sent to the trash.
    pub opponent_cards_seen: Vec<String>,
}

/// A player tag with the invisible characters the game inserts removed.
fn normalize_tag(tag: &str) -> String {
    tag.chars()
        .filter(|c| !matches!(c, '\u{200b}' | '\u{200c}' | '\u{feff}'))
        .collect::<String>()
        .trim()
        .to_lowercase()
}

/// `[Tag] message` -> (tag, message).
fn split_line(line: &str) -> Option<(String, &str)> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some((normalize_tag(&rest[..end]), rest[end + 1..].trim_start()))
}

fn looks_like_card_id(token: &str) -> bool {
    let Some((set, number)) = token.split_once('-') else {
        return false;
    };
    let letters = set.chars().take_while(|c| c.is_ascii_uppercase()).count();
    let digits = set.len() - letters;
    (1..=3).contains(&letters)
        && set.chars().skip(letters).all(|c| c.is_ascii_digit())
        && digits <= 2
        && number.len() == 3
        && number.chars().all(|c| c.is_ascii_digit())
}

/// Every card id in a line, in order, whichever of the log's formats it uses.
pub fn card_ids_in(line: &str) -> Vec<String> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|t| looks_like_card_id(t))
        .map(str::to_string)
        .collect()
}

fn push_unique(into: &mut Vec<String>, ids: impl IntoIterator<Item = String>) {
    for id in ids {
        if !into.contains(&id) {
            into.push(id);
        }
    }
}

/// Which tag is you? The one that appears in the most logs: your opponents change.
pub fn detect_player(logs: &[&str]) -> Option<String> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for log in logs {
        let mut seen: HashSet<String> = HashSet::new();
        for line in log.lines() {
            if let Some((tag, msg)) = split_line(line) {
                if msg.starts_with("Leader is") {
                    seen.insert(tag);
                }
            }
        }
        for tag in seen {
            *counts.entry(tag).or_default() += 1;
        }
    }
    let (tag, n) = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?;
    (n >= 2).then_some(tag)
}

/// Read one log as seen from `me` (a tag from [`detect_player`]).
pub fn parse_combat_log(text: &str, me: &str) -> Option<ParsedGame> {
    let me = normalize_tag(me);
    let mut your_leader = None;
    let mut opponent_leader = None;
    let mut chooser_first: Option<(String, bool)> = None;
    let mut turns = 0u32;
    let mut winner: Option<String> = None;
    let mut conceded: Option<String> = None;
    let mut yours = Vec::new();
    let mut theirs = Vec::new();

    for line in text.lines() {
        let Some((tag, msg)) = split_line(line) else {
            continue;
        };
        let mine = tag == me;
        if msg.starts_with("Leader is") {
            if let Some(id) = card_ids_in(msg).into_iter().next() {
                if mine {
                    your_leader.get_or_insert(id);
                } else {
                    opponent_leader.get_or_insert(id);
                }
            }
        } else if let Some(choice) = msg.strip_prefix("Chose to go ") {
            let first = choice.trim().to_lowercase().starts_with("first");
            chooser_first.get_or_insert((tag.clone(), first));
        } else if msg.starts_with("End Turn") {
            turns += 1;
        } else if msg.starts_with("Wins!") {
            winner.get_or_insert(tag.clone());
        } else if msg.starts_with("Concedes!") {
            conceded.get_or_insert(tag.clone());
        } else if msg.starts_with("Board:") || msg.starts_with("Trash:") || msg.starts_with("Deploy ") {
            // Public to both players.
            if mine {
                push_unique(&mut yours, card_ids_in(msg));
            } else if msg.starts_with("Deploy ") {
                push_unique(&mut theirs, card_ids_in(msg).into_iter().take(1));
            } else {
                push_unique(&mut theirs, card_ids_in(msg));
            }
        } else if mine && (msg.starts_with("Hand:") || msg.starts_with("Drew card from deck")) {
            // Your own private cards are yours to use. The opponent's are not read at all.
            push_unique(&mut yours, card_ids_in(msg));
        }
    }

    let (your_leader, opponent_leader) = (your_leader?, opponent_leader?);
    let result = match (winner, conceded) {
        (Some(w), _) => Some(if w == me { "win" } else { "loss" }),
        (None, Some(c)) => Some(if c == me { "loss" } else { "win" }),
        (None, None) => None,
    }
    .map(str::to_string);
    if turns < 2 && result.is_none() {
        return None; // a lobby or an abandoned room, not a game
    }
    let went_first = chooser_first.map(|(who, first)| if who == me { first } else { !first });
    Some(ParsedGame {
        your_leader,
        opponent_leader,
        went_first,
        turns,
        result,
        your_cards_seen: yours,
        opponent_cards_seen: theirs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both formats the simulator has used for card links.
    const OLD: &str = "\
Waiting for a Connection with Room ID:ABC123
Me\u{200b}#1 Has Connected
[Me\u{200b}#1] Leader is Eustass \"Captain\" Kid [\"OP10-099\">OP10-099]
[Foe#9] Leader is Nico Robin [\"OP09-062\">OP09-062]
[Me\u{200b}#1] Will select turn order
[Me\u{200b}#1] Chose to go First
[Me\u{200b}#1] Hand: [P-088,ST36-002]
[Foe#9] Hand: [SECRET-111,EB04-058]
[Foe#9] Drew card from deck: Secret Card [\"EB04-058\">EB04-058]
[Me\u{200b}#1] Deploy Killer [\"ST36-002\">ST36-002]
[Foe#9] Deploy Charlotte Pudding [\"OP17-109\">OP17-109]
[Foe#9] Board: [OP17-109]
[Foe#9] Trash: [OP16-108]
[Me\u{200b}#1] End Turn
[Foe#9] End Turn
[Me\u{200b}#1] End Turn
[Foe#9] Concedes!
";

    const NEW: &str = "\
[A#1] Leader is Monkey D. Luffy [<mark><link=\"OP17-079\">OP17-079</link></mark>]
[B#2] Leader is Enel [<mark><link=\"OP05-098\">OP05-098</link></mark>]
[B#2] Chose to go Second
[A#1] End Turn
[B#2] End Turn
[A#1] End Turn
[A#1] Wins!
";

    #[test]
    fn reads_leaders_order_turns_and_the_result_from_your_side() {
        let g = parse_combat_log(OLD, "me#1").unwrap();
        assert_eq!((g.your_leader.as_str(), g.opponent_leader.as_str()), ("OP10-099", "OP09-062"));
        assert_eq!(g.went_first, Some(true));
        assert_eq!(g.turns, 3);
        assert_eq!(g.result.as_deref(), Some("win"), "the opponent conceded");
    }

    #[test]
    fn reads_the_newer_link_format_and_works_out_turn_order_for_the_other_chooser() {
        let g = parse_combat_log(NEW, "A#1").unwrap();
        assert_eq!(g.your_leader, "OP17-079");
        assert_eq!(g.went_first, Some(true), "the opponent chose second, so you went first");
        assert_eq!(g.result.as_deref(), Some("win"));
        let lost = parse_combat_log(NEW, "B#2").unwrap();
        assert_eq!(lost.result.as_deref(), Some("loss"));
        assert_eq!(lost.went_first, Some(false));
    }

    #[test]
    fn the_opponents_hand_and_draws_are_never_read() {
        let g = parse_combat_log(OLD, "Me#1").unwrap();
        assert!(!g.opponent_cards_seen.contains(&"EB04-058".to_string()), "that was only in their hand/draw");
        assert!(g.opponent_cards_seen.contains(&"OP17-109".to_string()));
        assert!(g.opponent_cards_seen.contains(&"OP16-108".to_string()));
        assert!(g.your_cards_seen.contains(&"P-088".to_string()), "your own hand is yours");
    }

    #[test]
    fn a_log_that_never_became_a_game_is_ignored() {
        let lobby = "[Me#1] Leader is X [OP10-099]\n[Foe#9] Leader is Y [OP09-062]\n[Me#1] Will select turn order\n";
        assert!(parse_combat_log(lobby, "Me#1").is_none());
    }

    #[test]
    fn it_finds_you_as_the_tag_that_is_in_most_logs() {
        let logs = [OLD, NEW, "[Me\u{200b}#1] Leader is X [OP01-001]\n[Other#5] Leader is Y [OP02-001]"];
        assert_eq!(detect_player(&logs).as_deref(), Some("me#1"));
        assert_eq!(detect_player(&[OLD]), None, "one log is not enough to tell");
    }

    #[test]
    fn card_ids_are_found_in_every_notation() {
        assert_eq!(card_ids_in("Hand: [P-088,ST36-002,EB04-058]"), vec!["P-088", "ST36-002", "EB04-058"]);
        assert_eq!(card_ids_in("Kid [\"OP10-099\">OP10-099]"), vec!["OP10-099", "OP10-099"]);
        assert!(card_ids_in("Draw 1 Don").is_empty());
    }
}
