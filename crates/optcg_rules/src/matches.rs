//! Recording the matches you play, locally.
//!
//! [`MatchTracker`] watches the observed game and says when a match has ended.
//! [`MatchStore`] keeps finished matches in a JSON-lines file on your machine.
//! Only what the simulator shows both players is recorded: Leaders, public
//! cards, turn count, and the result. Hidden information is never stored.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// One finished match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchRecord {
    pub id: String,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub your_leader: String,
    pub opponent_leader: String,
    /// `None` when the recorder joined mid-game and could not tell.
    pub went_first: Option<bool>,
    pub turns: u32,
    /// "win" or "loss"; `None` while unknown (the HUD asks you).
    pub result: Option<String>,
    /// Where the result came from: "page" (the simulator said so) or "you".
    #[serde(default)]
    pub result_source: String,
    #[serde(default)]
    pub your_deck: String,
    /// Opponent cards that were visible on the table, never their hand.
    #[serde(default)]
    pub opponent_cards_seen: Vec<String>,
    #[serde(default)]
    pub your_cards_seen: Vec<String>,
}

/// What the tracker needs from the current game state.
#[derive(Debug, Clone, Default)]
pub struct Observed {
    pub now_ms: i64,
    pub your_leader: Option<String>,
    pub opponent_leader: Option<String>,
    pub turn: u32,
    /// 0 when it is your turn.
    pub active_player: u8,
    pub your_deck: String,
    pub your_cards: Vec<String>,
    pub opponent_cards: Vec<String>,
    /// "win", "loss" or "over" once the page reported the end of the game.
    pub outcome: Option<String>,
}

/// What an observation changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Step {
    /// A match that just ended.
    pub finished: Option<MatchRecord>,
    /// The result of the last finished match became known after it was recorded.
    pub result_update: Option<(String, String)>,
    /// A new match started: the caller should clear the previous outcome.
    pub new_game: bool,
}

#[derive(Debug, Clone)]
struct Active {
    started_ms: i64,
    your_leader: String,
    opponent_leader: String,
    went_first: Option<bool>,
    max_turn: u32,
    your_deck: String,
    your_cards: Vec<String>,
    opponent_cards: Vec<String>,
}

#[derive(Debug, Clone)]
struct Finished {
    id: String,
    your_leader: String,
    opponent_leader: String,
    max_turn: u32,
    had_result: bool,
}

/// A match that dropped this many turns below the previous peak is a new game.
const TURN_DROP_FOR_NEW_GAME: u32 = 2;
/// Matches shorter than this with no result are noise (a reconnect, a lobby).
const MIN_TURNS_TO_KEEP: u32 = 2;

#[derive(Debug, Default)]
pub struct MatchTracker {
    active: Option<Active>,
    finished: Option<Finished>,
}

fn merge(into: &mut Vec<String>, from: &[String]) {
    for id in from {
        if !id.is_empty() && !into.contains(id) {
            into.push(id.clone());
        }
    }
}

impl MatchTracker {
    pub fn observe(&mut self, o: Observed) -> Step {
        let mut step = Step::default();
        let (Some(you), Some(opp)) = (o.your_leader.clone(), o.opponent_leader.clone()) else {
            return step; // both Leaders must have been seen before a match counts
        };

        // A finished match stays "finished" until the game visibly starts over.
        if let Some(done) = &self.finished {
            let same_leaders = done.your_leader == you && done.opponent_leader == opp;
            let restarted = o.turn + TURN_DROP_FOR_NEW_GAME <= done.max_turn;
            if same_leaders && !restarted {
                // Same game. The result may only now have appeared.
                if !done.had_result {
                    if let Some(result @ ("win" | "loss")) = o.outcome.as_deref() {
                        step.result_update = Some((done.id.clone(), result.to_string()));
                        if let Some(done) = self.finished.as_mut() {
                            done.had_result = true;
                        }
                    }
                }
                return step;
            }
            self.finished = None;
            step.new_game = true;
        }

        // A change of Leader, or the turn counter falling back, ends the previous match.
        if let Some(active) = &self.active {
            let changed = active.your_leader != you || active.opponent_leader != opp;
            let restarted = o.turn + TURN_DROP_FOR_NEW_GAME <= active.max_turn;
            if changed || restarted {
                step.finished = self.close(o.now_ms, None);
                step.new_game = true;
            }
        }

        if self.active.is_none() {
            self.active = Some(Active {
                started_ms: o.now_ms,
                your_leader: you,
                opponent_leader: opp,
                // Only knowable if we watched from the first turn.
                went_first: (o.turn <= 1).then_some(o.active_player == 0),
                max_turn: o.turn,
                your_deck: o.your_deck.clone(),
                your_cards: Vec::new(),
                opponent_cards: Vec::new(),
            });
        }

        if let Some(active) = self.active.as_mut() {
            active.max_turn = active.max_turn.max(o.turn);
            if active.your_deck.is_empty() {
                active.your_deck = o.your_deck.clone();
            }
            merge(&mut active.your_cards, &o.your_cards);
            merge(&mut active.opponent_cards, &o.opponent_cards);
        }

        if let Some(outcome) = o.outcome.as_deref() {
            // The game is over; the very first record of it ends the match.
            let result = match outcome {
                "win" | "loss" => Some(outcome.to_string()),
                _ => None,
            };
            if let Some(record) = self.close(o.now_ms, result) {
                step.finished = Some(record);
            }
        }
        step
    }

    /// End the current match now (for example when the page goes away).
    pub fn end_now(&mut self, now_ms: i64) -> Option<MatchRecord> {
        self.close(now_ms, None)
    }

    fn close(&mut self, now_ms: i64, result: Option<String>) -> Option<MatchRecord> {
        let active = self.active.take()?;
        if active.max_turn < MIN_TURNS_TO_KEEP && result.is_none() {
            return None;
        }
        let id = format!("{}-{}", active.started_ms, now_ms);
        self.finished = Some(Finished {
            id: id.clone(),
            your_leader: active.your_leader.clone(),
            opponent_leader: active.opponent_leader.clone(),
            max_turn: active.max_turn,
            had_result: result.is_some(),
        });
        let from_page = result.is_some();
        Some(MatchRecord {
            id,
            started_ms: active.started_ms,
            ended_ms: now_ms,
            your_leader: active.your_leader,
            opponent_leader: active.opponent_leader,
            went_first: active.went_first,
            turns: active.max_turn,
            result,
            result_source: if from_page { "page".into() } else { String::new() },
            your_deck: active.your_deck,
            opponent_cards_seen: active.opponent_cards,
            your_cards_seen: active.your_cards,
        })
    }
}

/// Finished matches on disk, one JSON object per line.
#[derive(Debug, Clone)]
pub struct MatchStore {
    path: PathBuf,
}

impl MatchStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Vec<MatchRecord> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|line| serde_json::from_str::<MatchRecord>(line).ok())
            .collect()
    }

    pub fn append(&self, record: &MatchRecord) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(record)?)
    }

    /// Set a match's result (from the simulator or from you). Returns false if
    /// no such match is stored.
    pub fn set_result(&self, id: &str, result: Option<&str>, source: &str) -> std::io::Result<bool> {
        let mut records = self.load();
        let Some(found) = records.iter_mut().find(|r| r.id == id) else {
            return Ok(false);
        };
        found.result = result.map(str::to_string);
        found.result_source = if result.is_some() { source.to_string() } else { String::new() };
        self.rewrite(&records)?;
        Ok(true)
    }

    pub fn delete(&self, id: &str) -> std::io::Result<bool> {
        let mut records = self.load();
        let before = records.len();
        records.retain(|r| r.id != id);
        if records.len() == before {
            return Ok(false);
        }
        self.rewrite(&records)?;
        Ok(true)
    }

    /// Replace the file safely: write a temp file, then move it into place.
    fn rewrite(&self, records: &[MatchRecord]) -> std::io::Result<()> {
        let tmp = self.path.with_extension("jsonl.tmp");
        {
            let mut file = std::fs::File::create(&tmp)?;
            for record in records {
                writeln!(file, "{}", serde_json::to_string(record)?)?;
            }
        }
        std::fs::rename(tmp, &self.path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Wins and losses for one Leader.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub leader: String,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub unknown: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchStats {
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub unknown: u32,
    pub first_games: u32,
    pub first_wins: u32,
    pub second_games: u32,
    pub second_wins: u32,
    pub by_your_leader: Vec<Tally>,
    pub by_opponent_leader: Vec<Tally>,
}

fn tally(records: &[MatchRecord], key: impl Fn(&MatchRecord) -> &str) -> Vec<Tally> {
    let mut map: BTreeMap<&str, Tally> = BTreeMap::new();
    for r in records {
        let t = map.entry(key(r)).or_insert_with(|| Tally {
            leader: key(r).to_string(),
            ..Default::default()
        });
        t.games += 1;
        match r.result.as_deref() {
            Some("win") => t.wins += 1,
            Some("loss") => t.losses += 1,
            _ => t.unknown += 1,
        }
    }
    let mut out: Vec<Tally> = map.into_values().collect();
    out.sort_by(|a, b| b.games.cmp(&a.games).then(a.leader.cmp(&b.leader)));
    out
}

/// Totals and per-Leader results. Matches with an unknown result count as
/// games but never as wins or losses.
pub fn summarize(records: &[MatchRecord]) -> MatchStats {
    let mut stats = MatchStats {
        games: records.len() as u32,
        by_your_leader: tally(records, |r| r.your_leader.as_str()),
        by_opponent_leader: tally(records, |r| r.opponent_leader.as_str()),
        ..Default::default()
    };
    for r in records {
        let won = r.result.as_deref() == Some("win");
        match r.result.as_deref() {
            Some("win") => stats.wins += 1,
            Some("loss") => stats.losses += 1,
            _ => stats.unknown += 1,
        }
        if r.result.is_some() {
            match r.went_first {
                Some(true) => {
                    stats.first_games += 1;
                    stats.first_wins += u32::from(won);
                }
                Some(false) => {
                    stats.second_games += 1;
                    stats.second_wins += u32::from(won);
                }
                None => {}
            }
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(turn: u32, outcome: Option<&str>) -> Observed {
        Observed {
            now_ms: 1_000 + i64::from(turn) * 10,
            your_leader: Some("OP17-079".into()),
            opponent_leader: Some("OP09-062".into()),
            turn,
            active_player: 0,
            your_deck: "Super Luffy".into(),
            your_cards: vec!["OP17-081".into()],
            opponent_cards: vec!["OP09-078".into()],
            outcome: outcome.map(str::to_string),
        }
    }

    #[test]
    fn a_match_is_recorded_when_the_page_says_it_is_over() {
        let mut t = MatchTracker::default();
        assert_eq!(t.observe(obs(1, None)), Step::default());
        t.observe(obs(2, None));
        t.observe(obs(5, None));
        let step = t.observe(obs(6, Some("win")));
        let record = step.finished.expect("finished");
        assert_eq!(record.result.as_deref(), Some("win"));
        assert_eq!(record.result_source, "page");
        assert_eq!(record.turns, 6);
        assert_eq!(record.went_first, Some(true), "watched from turn 1");
        assert_eq!(record.opponent_cards_seen, vec!["OP09-078"]);
    }

    #[test]
    fn nothing_is_recorded_until_both_leaders_were_seen() {
        let mut t = MatchTracker::default();
        let mut o = obs(3, Some("win"));
        o.opponent_leader = None;
        assert_eq!(t.observe(o), Step::default());
    }

    #[test]
    fn joining_mid_game_does_not_guess_who_went_first() {
        let mut t = MatchTracker::default();
        t.observe(obs(4, None));
        let record = t.observe(obs(5, Some("loss"))).finished.unwrap();
        assert_eq!(record.went_first, None);
    }

    #[test]
    fn the_same_finished_game_is_not_recorded_twice() {
        let mut t = MatchTracker::default();
        t.observe(obs(1, None));
        t.observe(obs(4, None));
        assert!(t.observe(obs(5, Some("win"))).finished.is_some());
        assert_eq!(t.observe(obs(5, Some("win"))), Step::default());
    }

    #[test]
    fn a_late_result_updates_the_match_already_recorded() {
        let mut t = MatchTracker::default();
        t.observe(obs(1, None));
        t.observe(obs(4, None));
        let record = t.observe(obs(5, Some("over"))).finished.unwrap();
        assert_eq!(record.result, None, "unknown until the page or you say");
        let step = t.observe(obs(5, Some("loss")));
        assert_eq!(step.result_update, Some((record.id, "loss".to_string())));
    }

    #[test]
    fn a_new_game_after_a_finished_one_starts_fresh() {
        let mut t = MatchTracker::default();
        t.observe(obs(1, None));
        t.observe(obs(6, Some("win")));
        // Same Leaders, turn counter back at 1: a rematch.
        let step = t.observe(obs(1, None));
        assert!(step.new_game);
        let record = t.observe(obs(5, Some("loss"))).finished.unwrap();
        assert_eq!(record.result.as_deref(), Some("loss"));
        assert_eq!(record.turns, 5);
    }

    #[test]
    fn leaving_a_game_unfinished_is_kept_with_an_unknown_result() {
        let mut t = MatchTracker::default();
        t.observe(obs(1, None));
        t.observe(obs(7, None));
        let mut next = obs(1, None);
        next.opponent_leader = Some("OP13-004".into());
        let step = t.observe(next);
        let record = step.finished.expect("previous match closed");
        assert_eq!((record.turns, record.result), (7, None));
        assert!(step.new_game);
    }

    #[test]
    fn blips_shorter_than_two_turns_are_not_kept() {
        let mut t = MatchTracker::default();
        t.observe(obs(1, None));
        assert!(t.end_now(5_000).is_none());
    }

    fn record(id: &str, leader: &str, opp: &str, result: Option<&str>, first: Option<bool>) -> MatchRecord {
        MatchRecord {
            id: id.into(),
            started_ms: 0,
            ended_ms: 1,
            your_leader: leader.into(),
            opponent_leader: opp.into(),
            went_first: first,
            turns: 6,
            result: result.map(str::to_string),
            result_source: String::new(),
            your_deck: String::new(),
            opponent_cards_seen: vec![],
            your_cards_seen: vec![],
        }
    }

    #[test]
    fn the_store_round_trips_and_edits_results() {
        let dir = std::env::temp_dir().join(format!("optcg-matches-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = MatchStore::new(dir.join("matches.jsonl"));
        assert!(store.load().is_empty());
        store.append(&record("a", "L1", "O1", None, Some(true))).unwrap();
        store.append(&record("b", "L1", "O2", Some("win"), Some(false))).unwrap();
        assert_eq!(store.load().len(), 2);
        assert!(store.set_result("a", Some("loss"), "you").unwrap());
        let loaded = store.load();
        assert_eq!(loaded[0].result.as_deref(), Some("loss"));
        assert_eq!(loaded[0].result_source, "you");
        assert!(!store.set_result("zzz", Some("win"), "you").unwrap());
        assert!(store.delete("b").unwrap());
        assert_eq!(store.load().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stats_count_unknown_results_as_games_but_not_as_wins_or_losses() {
        let records = vec![
            record("1", "L1", "O1", Some("win"), Some(true)),
            record("2", "L1", "O1", Some("loss"), Some(false)),
            record("3", "L1", "O2", None, Some(true)),
            record("4", "L2", "O2", Some("win"), Some(false)),
        ];
        let s = summarize(&records);
        assert_eq!((s.games, s.wins, s.losses, s.unknown), (4, 2, 1, 1));
        assert_eq!((s.first_games, s.first_wins), (1, 1), "unknown results excluded");
        assert_eq!((s.second_games, s.second_wins), (2, 1));
        let l1 = s.by_your_leader.iter().find(|t| t.leader == "L1").unwrap();
        assert_eq!((l1.games, l1.wins, l1.losses, l1.unknown), (3, 1, 1, 1));
        assert_eq!(s.by_your_leader[0].leader, "L1", "most played first");
    }
}
