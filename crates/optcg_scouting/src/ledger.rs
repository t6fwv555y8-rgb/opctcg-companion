//! What we have learned about the decks across the table, kept between games.
//!
//! An opponent never shows you their list. What they do show you, a card at a
//! time, is the part of it they had to play to win. This module keeps those
//! sightings, so the fifth game against a leader starts with four games of
//! evidence instead of nothing.

use crate::matchup::{FinishedGame, LifeTrack, MatchupLedger, Outcome};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped when matchup records joined the file. Older files load: every field
/// added since is defaulted.
pub const LEDGER_VERSION: u32 = 2;

/// Cap on remembered leaders. Reached only by someone who has played hundreds
/// of distinct matchups, and it keeps the file from growing without limit.
pub const MAX_PROFILES: usize = 200;

/// Cap on remembered cards per leader. A deck is fifty cards, so a profile far
/// past that is mostly noise from misreads.
pub const MAX_CARDS_PER_PROFILE: usize = 120;

/// One card, as seen in a single game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sighting {
    pub card_id: String,
    /// Most copies visible at once. A floor on how many they run, never a
    /// count of the deck: two in the trash and one on board is three, but a
    /// fourth may simply have stayed in the deck all game.
    pub copies: u32,
    /// Earliest turn it appeared, which is what tells you when a card matters.
    pub first_turn: u32,
}

/// Measurements taken from one game, used to describe how a deck actually
/// played rather than what its name suggests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tempo {
    /// Earliest turn they had a character out.
    pub first_board_turn: Option<u32>,
    /// Most characters they held at once.
    pub widest_board: u32,
    /// Life they took off you.
    pub life_taken: u32,
    /// Turn you first lost life to them.
    pub first_damage_turn: Option<u32>,
    /// Last turn observed, so a game abandoned on turn two is not read as a
    /// game that went long.
    pub last_turn: u32,
}

/// A game in progress, or the last one seen.
///
/// Held separately from the profile it will fold into so that a game counts
/// once however many state updates it produces, and so closing the app
/// mid-game does not lose what was seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGame {
    /// The simulator's id for this game, used to notice a new one starting.
    pub game_id: String,
    pub leader_id: String,
    pub leader_name: String,
    pub started_at: String,
    pub sightings: Vec<Sighting>,
    pub tempo: Tempo,
    /// Your own leader, needed to file the result under a matchup rather than
    /// just against an opponent. Defaulted so files written before matchup
    /// records existed still load.
    #[serde(default)]
    pub your_leader_id: String,
    #[serde(default)]
    pub your_leader_name: String,
    /// Life on both sides, from which the result is inferred.
    #[serde(default)]
    pub life: LifeTrack,
}

impl OpenGame {
    fn new(game_id: &str, leader_id: &str, leader_name: &str, now: &str) -> Self {
        Self {
            game_id: game_id.to_string(),
            leader_id: leader_id.to_string(),
            leader_name: leader_name.to_string(),
            started_at: now.to_string(),
            sightings: Vec::new(),
            tempo: Tempo::default(),
            your_leader_id: String::new(),
            your_leader_name: String::new(),
            life: LifeTrack::default(),
        }
    }

    /// Whether this game saw enough to be worth remembering.
    ///
    /// An idle HUD sitting on a default position must not count as a game
    /// played, or every profile would be diluted by matches that never
    /// happened.
    pub fn is_substantive(&self) -> bool {
        !self.sightings.is_empty()
    }

    /// Whether this belongs in the matchup record.
    ///
    /// Looser than `is_substantive` by design. A game that ended in a decided
    /// result is a game you played even if the opponent's cards went
    /// unobserved, and requiring sightings would drop those — biasing the
    /// record towards whichever kind of game happens to show more cards.
    /// An idle HUD still cannot qualify: it never sees life fall from a
    /// positive reading to zero.
    pub fn counts_as_played(&self) -> bool {
        self.is_substantive() || self.life.outcome().is_some()
    }

    /// The result this game reached, if it reached one.
    pub fn outcome(&self) -> Option<Outcome> {
        self.life.outcome()
    }

    /// Note `copies` of a card visible on `turn`.
    fn record_card(&mut self, card_id: &str, copies: u32, turn: u32) {
        if card_id.is_empty() || copies == 0 {
            return;
        }
        match self.sightings.iter_mut().find(|s| s.card_id == card_id) {
            Some(seen) => {
                seen.copies = seen.copies.max(copies);
                seen.first_turn = seen.first_turn.min(turn);
            }
            None => {
                if self.sightings.len() >= MAX_CARDS_PER_PROFILE {
                    return;
                }
                self.sightings.push(Sighting {
                    card_id: card_id.to_string(),
                    copies,
                    first_turn: turn,
                });
            }
        }
    }
}

/// What every game against one leader has added up to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderProfile {
    pub leader_id: String,
    pub leader_name: String,
    /// Games folded in. Every rate in the profile is over this.
    pub games: u32,
    pub first_seen: String,
    pub last_seen: String,
    pub cards: Vec<CardRecord>,
    pub tempo: TempoRecord,
}

/// One card's history against a leader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardRecord {
    pub card_id: String,
    /// Games this card turned up in. Against `LeaderProfile::games`, this is
    /// the whole basis for calling a card a staple or a one-off.
    pub games_seen: u32,
    /// Most copies ever seen at once.
    pub max_copies: u32,
    /// Summed per-game copies, kept so an average survives more games.
    pub total_copies: u32,
    /// Earliest turn it has ever appeared.
    pub earliest_turn: u32,
}

/// Tempo measurements summed over every game against a leader.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoRecord {
    /// Games in which they had a board at all, the denominator for board rates.
    pub games_with_board: u32,
    pub summed_first_board_turn: u32,
    pub widest_board: u32,
    pub summed_life_taken: u32,
    pub games_with_damage: u32,
    pub summed_first_damage_turn: u32,
    pub summed_last_turn: u32,
}

impl LeaderProfile {
    fn new(leader_id: &str, leader_name: &str, now: &str) -> Self {
        Self {
            leader_id: leader_id.to_string(),
            leader_name: leader_name.to_string(),
            games: 0,
            first_seen: now.to_string(),
            last_seen: now.to_string(),
            cards: Vec::new(),
            tempo: TempoRecord::default(),
        }
    }

    fn fold_in(&mut self, game: &OpenGame, now: &str) {
        self.games = self.games.saturating_add(1);
        self.last_seen = now.to_string();
        if !game.leader_name.is_empty() {
            self.leader_name = game.leader_name.clone();
        }

        for sighting in &game.sightings {
            match self
                .cards
                .iter_mut()
                .find(|c| c.card_id == sighting.card_id)
            {
                Some(record) => {
                    record.games_seen = record.games_seen.saturating_add(1);
                    record.max_copies = record.max_copies.max(sighting.copies);
                    record.total_copies = record.total_copies.saturating_add(sighting.copies);
                    record.earliest_turn = record.earliest_turn.min(sighting.first_turn);
                }
                None => self.cards.push(CardRecord {
                    card_id: sighting.card_id.clone(),
                    games_seen: 1,
                    max_copies: sighting.copies,
                    total_copies: sighting.copies,
                    earliest_turn: sighting.first_turn,
                }),
            }
        }

        let tempo = &mut self.tempo;
        if let Some(turn) = game.tempo.first_board_turn {
            tempo.games_with_board = tempo.games_with_board.saturating_add(1);
            tempo.summed_first_board_turn = tempo.summed_first_board_turn.saturating_add(turn);
        }
        tempo.widest_board = tempo.widest_board.max(game.tempo.widest_board);
        tempo.summed_life_taken = tempo
            .summed_life_taken
            .saturating_add(game.tempo.life_taken);
        if let Some(turn) = game.tempo.first_damage_turn {
            tempo.games_with_damage = tempo.games_with_damage.saturating_add(1);
            tempo.summed_first_damage_turn = tempo.summed_first_damage_turn.saturating_add(turn);
        }
        tempo.summed_last_turn = tempo.summed_last_turn.saturating_add(game.tempo.last_turn);

        // Keep the busiest cards when a profile has collected more than a deck's
        // worth, since the rare tail is where misreads accumulate.
        if self.cards.len() > MAX_CARDS_PER_PROFILE {
            self.cards.sort_by(|a, b| {
                b.games_seen
                    .cmp(&a.games_seen)
                    .then(a.card_id.cmp(&b.card_id))
            });
            self.cards.truncate(MAX_CARDS_PER_PROFILE);
        }
    }
}

/// Everything learned about everyone, and the game currently under way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutingLedger {
    pub version: u32,
    pub profiles: Vec<LeaderProfile>,
    /// The game being watched. Folded into its profile when the next game
    /// starts, which is the only moment we know it finished.
    #[serde(default)]
    pub open: Option<OpenGame>,
    /// How your decks have fared against each leader. Defaulted so files
    /// written before this existed still load.
    #[serde(default)]
    pub matchups: MatchupLedger,
}

impl Default for ScoutingLedger {
    fn default() -> Self {
        Self {
            version: LEDGER_VERSION,
            profiles: Vec::new(),
            open: None,
            matchups: MatchupLedger::default(),
        }
    }
}

impl ScoutingLedger {
    /// Read a ledger from disk, falling back to an empty one. Learned history
    /// is a convenience, so a corrupt file must not stop the HUD booting.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(mut ledger) => {
                ledger.version = LEDGER_VERSION;
                ledger.prune();
                ledger
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "scouting ledger unreadable; starting empty");
                Self::default()
            }
        }
    }

    /// Write via temp file + rename, so a crash mid-write cannot truncate the
    /// history collected so far.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn profile(&self, leader_id: &str) -> Option<&LeaderProfile> {
        self.profiles.iter().find(|p| p.leader_id == leader_id)
    }

    /// Games recorded against `leader_id`, counting one in progress.
    pub fn games_against(&self, leader_id: &str) -> u32 {
        let folded = self.profile(leader_id).map_or(0, |p| p.games);
        let open = self
            .open
            .as_ref()
            .filter(|g| g.leader_id == leader_id && g.is_substantive())
            .map_or(0, |_| 1);
        folded + open
    }

    /// Start watching a game, folding away whichever one was open.
    ///
    /// Called when the game id across the table changes, which is the only
    /// signal available that the last game ended.
    pub fn begin_game(&mut self, game_id: &str, leader_id: &str, leader_name: &str, now: &str) {
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.game_id == game_id)
        {
            return;
        }
        self.close_open_game(now);
        self.open = Some(OpenGame::new(game_id, leader_id, leader_name, now));
    }

    /// Fold the open game into its profile and its matchup record.
    pub fn close_open_game(&mut self, now: &str) {
        let Some(game) = self.open.take() else {
            return;
        };
        if game.counts_as_played() && !game.leader_id.is_empty() {
            self.matchups.fold_in(
                &FinishedGame {
                    your_leader: game.your_leader_id.clone(),
                    your_leader_name: game.your_leader_name.clone(),
                    their_leader: game.leader_id.clone(),
                    their_leader_name: game.leader_name.clone(),
                    outcome: game.outcome(),
                    your_life: game.life.your_last,
                    their_life: game.life.their_last,
                    last_turn: game.tempo.last_turn,
                },
                now,
            );
        }
        if !game.is_substantive() || game.leader_id.is_empty() {
            return;
        }
        let position = self
            .profiles
            .iter()
            .position(|p| p.leader_id == game.leader_id);
        let index = match position {
            Some(index) => index,
            None => {
                if self.profiles.len() >= MAX_PROFILES {
                    // Forget whoever we have not seen in longest.
                    if let Some(oldest) = self
                        .profiles
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| a.last_seen.cmp(&b.last_seen))
                        .map(|(i, _)| i)
                    {
                        self.profiles.remove(oldest);
                    }
                }
                self.profiles
                    .push(LeaderProfile::new(&game.leader_id, &game.leader_name, now));
                self.profiles.len() - 1
            }
        };
        self.profiles[index].fold_in(&game, now);
    }

    /// Fill in the open game's leader once it has been read.
    ///
    /// Cards often resolve a beat before the leader does on a fresh connection,
    /// and a game that closes without a leader is thrown away, so the sightings
    /// from those first turns would be lost.
    pub fn backfill_leader(&mut self, leader_id: &str, leader_name: &str) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.leader_id.is_empty() && !leader_id.is_empty() {
            open.leader_id = leader_id.to_string();
        }
        if open.leader_name.is_empty() && !leader_name.is_empty() {
            open.leader_name = leader_name.to_string();
        }
    }

    /// Note which of your leaders is playing this game.
    ///
    /// Separate from `begin_game` because your leader, like theirs, is often
    /// read a beat after the game id is.
    pub fn record_your_leader(&mut self, leader_id: &str, leader_name: &str) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if !leader_id.is_empty() {
            open.your_leader_id = leader_id.to_string();
        }
        if open.your_leader_name.is_empty() && !leader_name.is_empty() {
            open.your_leader_name = leader_name.to_string();
        }
    }

    /// Take a life reading for both sides in the open game.
    pub fn record_life(&mut self, your_life: u32, their_life: u32) {
        if let Some(open) = self.open.as_mut() {
            open.life.observe(your_life, their_life);
        }
    }

    /// The result the open game has reached, if any.
    pub fn open_outcome(&self) -> Option<Outcome> {
        self.open.as_ref().and_then(OpenGame::outcome)
    }

    /// Note `copies` of a card visible on `turn` in the open game.
    pub fn record_card(&mut self, card_id: &str, copies: u32, turn: u32) {
        if let Some(open) = self.open.as_mut() {
            open.record_card(card_id, copies, turn);
        }
    }

    /// Replace the open game's tempo measurements.
    pub fn record_tempo(&mut self, tempo: Tempo) {
        if let Some(open) = self.open.as_mut() {
            open.tempo = tempo;
        }
    }

    pub fn open_tempo(&self) -> Tempo {
        self.open
            .as_ref()
            .map(|open| open.tempo.clone())
            .unwrap_or_default()
    }

    /// Everything known about a leader, counting the game under way.
    ///
    /// Reading mid-game matters: the cards they have played this game are the
    /// most relevant evidence there is, and waiting for the game to end to use
    /// them would be useless to someone deciding a turn.
    pub fn merged_profile(&self, leader_id: &str) -> Option<LeaderProfile> {
        let open = self
            .open
            .as_ref()
            .filter(|g| g.leader_id == leader_id && g.is_substantive());
        match (self.profile(leader_id), open) {
            (None, None) => None,
            (Some(profile), None) => Some(profile.clone()),
            (existing, Some(game)) => {
                let mut merged = existing.cloned().unwrap_or_else(|| {
                    LeaderProfile::new(leader_id, &game.leader_name, &game.started_at)
                });
                merged.fold_in(game, &game.started_at);
                Some(merged)
            }
        }
    }

    fn prune(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        self.profiles.retain(|profile| {
            if profile.leader_id.trim().is_empty() || seen.iter().any(|s| s == &profile.leader_id) {
                return false;
            }
            seen.push(profile.leader_id.clone());
            true
        });
        self.profiles.truncate(MAX_PROFILES);
        for profile in &mut self.profiles {
            profile.cards.truncate(MAX_CARDS_PER_PROFILE);
        }
        self.matchups.prune();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-01-01T00:00:00Z";
    const LATER: &str = "2026-01-02T00:00:00Z";

    /// Play out one game against a leader, seeing each of `cards` once.
    fn play_game(ledger: &mut ScoutingLedger, game_id: &str, leader: &str, cards: &[&str]) {
        ledger.begin_game(game_id, leader, "Monkey.D.Luffy", NOW);
        for card in cards {
            ledger.record_card(card, 1, 3);
        }
    }

    #[test]
    fn a_leader_is_unknown_until_a_game_is_folded_in() {
        let mut ledger = ScoutingLedger::default();
        play_game(&mut ledger, "g1", "OP17-079", &["OP17-080"]);

        assert!(
            ledger.profile("OP17-079").is_none(),
            "nothing is committed while the game is still being watched"
        );
        assert_eq!(
            ledger.games_against("OP17-079"),
            1,
            "but the game in progress still counts as evidence"
        );
    }

    #[test]
    fn the_next_game_starting_folds_the_last_one_in() {
        let mut ledger = ScoutingLedger::default();
        play_game(&mut ledger, "g1", "OP17-079", &["OP17-080"]);
        play_game(&mut ledger, "g2", "OP17-079", &["OP17-081"]);

        let profile = ledger.profile("OP17-079").expect("first game folded in");
        assert_eq!(profile.games, 1);
        assert_eq!(profile.cards.len(), 1);
        assert_eq!(profile.cards[0].card_id, "OP17-080");
    }

    #[test]
    fn a_card_seen_every_game_accumulates_a_high_rate() {
        let mut ledger = ScoutingLedger::default();
        for n in 0..4 {
            play_game(
                &mut ledger,
                &format!("g{n}"),
                "OP17-079",
                &["OP17-080", "OP17-081"],
            );
        }
        ledger.close_open_game(LATER);

        let profile = ledger.profile("OP17-079").unwrap();
        assert_eq!(profile.games, 4);
        let staple = profile
            .cards
            .iter()
            .find(|c| c.card_id == "OP17-080")
            .unwrap();
        assert_eq!(staple.games_seen, 4);
    }

    #[test]
    fn a_game_that_saw_nothing_is_not_counted_as_a_game() {
        let mut ledger = ScoutingLedger::default();
        // An idle HUD: a game id and a leader, but nothing ever revealed.
        ledger.begin_game("g1", "OP17-079", "Monkey.D.Luffy", NOW);
        ledger.begin_game("g2", "OP17-079", "Monkey.D.Luffy", NOW);
        ledger.close_open_game(LATER);

        assert!(
            ledger.profile("OP17-079").is_none(),
            "an idle position must not dilute a profile with games that never happened"
        );
    }

    #[test]
    fn repeated_updates_within_one_game_count_once() {
        let mut ledger = ScoutingLedger::default();
        ledger.begin_game("g1", "OP17-079", "Monkey.D.Luffy", NOW);
        // The same game id arriving many times is the normal case: one per
        // state update.
        for _ in 0..20 {
            ledger.begin_game("g1", "OP17-079", "Monkey.D.Luffy", NOW);
            ledger.record_card("OP17-080", 1, 3);
        }
        ledger.close_open_game(LATER);

        let profile = ledger.profile("OP17-079").unwrap();
        assert_eq!(profile.games, 1, "state updates are not games");
        assert_eq!(profile.cards[0].games_seen, 1);
    }

    #[test]
    fn copies_keep_the_largest_count_seen_at_once() {
        let mut ledger = ScoutingLedger::default();
        ledger.begin_game("g1", "OP17-079", "Monkey.D.Luffy", NOW);
        ledger.record_card("OP17-080", 1, 4);
        ledger.record_card("OP17-080", 3, 6);
        ledger.record_card("OP17-080", 2, 8);
        ledger.close_open_game(LATER);

        let record = &ledger.profile("OP17-079").unwrap().cards[0];
        assert_eq!(record.max_copies, 3);
        assert_eq!(
            record.earliest_turn, 4,
            "the earliest sighting is what says when the card matters"
        );
    }

    #[test]
    fn separate_leaders_keep_separate_histories() {
        let mut ledger = ScoutingLedger::default();
        play_game(&mut ledger, "g1", "OP17-079", &["OP17-080"]);
        play_game(&mut ledger, "g2", "ST01-001", &["ST01-002"]);
        ledger.close_open_game(LATER);

        assert_eq!(ledger.profile("OP17-079").unwrap().cards.len(), 1);
        assert_eq!(
            ledger.profile("ST01-001").unwrap().cards[0].card_id,
            "ST01-002"
        );
    }

    #[test]
    fn the_game_in_progress_is_readable_before_it_ends() {
        let mut ledger = ScoutingLedger::default();
        play_game(&mut ledger, "g1", "OP17-079", &["OP17-080"]);
        play_game(&mut ledger, "g2", "OP17-079", &["OP17-081"]);

        let merged = ledger
            .merged_profile("OP17-079")
            .expect("the open game is evidence too");
        assert_eq!(merged.games, 2);
        assert!(
            merged.cards.iter().any(|c| c.card_id == "OP17-081"),
            "what they played this game is the most relevant evidence there is"
        );
    }

    #[test]
    fn tempo_is_summed_across_games() {
        let mut ledger = ScoutingLedger::default();
        for n in 0..2 {
            play_game(&mut ledger, &format!("g{n}"), "OP17-079", &["OP17-080"]);
            ledger.record_tempo(Tempo {
                first_board_turn: Some(2),
                widest_board: 3,
                life_taken: 2,
                first_damage_turn: Some(3),
                last_turn: 7,
            });
        }
        ledger.close_open_game(LATER);

        let tempo = &ledger.profile("OP17-079").unwrap().tempo;
        assert_eq!(tempo.games_with_board, 2);
        assert_eq!(tempo.summed_first_board_turn, 4);
        assert_eq!(tempo.summed_life_taken, 4);
        assert_eq!(tempo.widest_board, 3);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("optcg-ledger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scouting.json");

        let mut ledger = ScoutingLedger::default();
        play_game(&mut ledger, "g1", "OP17-079", &["OP17-080"]);
        ledger.close_open_game(LATER);
        ledger.save(&path).unwrap();

        let loaded = ScoutingLedger::load(&path);
        assert_eq!(loaded.profile("OP17-079").unwrap().games, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unfinished_game_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("optcg-ledger-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scouting.json");

        let mut ledger = ScoutingLedger::default();
        play_game(&mut ledger, "g1", "OP17-079", &["OP17-080"]);
        ledger.save(&path).unwrap();

        let mut loaded = ScoutingLedger::load(&path);
        loaded.close_open_game(LATER);
        assert_eq!(
            loaded.profile("OP17-079").unwrap().games,
            1,
            "closing the app mid-game must not throw the game away"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_or_corrupt_file_loads_empty() {
        let dir = std::env::temp_dir().join(format!("optcg-ledger-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scouting.json");

        assert!(ScoutingLedger::load(&path).profiles.is_empty());
        std::fs::write(&path, "{ not json").unwrap();
        assert!(ScoutingLedger::load(&path).profiles.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
