//! Watching games and writing down what the opponent showed you.
//!
//! This runs on every state update with no prompting, which is the only way it
//! could work: nobody is going to stop mid-game to log the card that was just
//! played. The cost of that is having to be careful, since an idle HUD sitting
//! on a default position must not be recorded as a game anyone played.

use crate::ledger::{ScoutingLedger, Tempo};
use crate::matchup::LifeTrack;
use optcg_core::GameState;
use std::collections::HashMap;

/// The opponent's seat. Matches the app's convention throughout.
const OPPONENT: usize = 1;
const YOU: usize = 0;

/// Accumulates readings from the live position into a ledger.
pub struct Scout {
    ledger: ScoutingLedger,
    /// The game currently being watched.
    watching: Option<String>,
}

impl Scout {
    pub fn new(ledger: ScoutingLedger) -> Self {
        Self {
            ledger,
            watching: None,
        }
    }

    pub fn ledger(&self) -> &ScoutingLedger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut ScoutingLedger {
        &mut self.ledger
    }

    /// Take a reading from the live position.
    ///
    /// Returns whether anything was learned, which is the app's cue to persist.
    /// Most updates teach nothing, so this is usually false and saving on every
    /// update would be pointless disk traffic.
    pub fn observe(&mut self, state: &GameState, now: &str) -> bool {
        let opponent = &state.players[OPPONENT];
        let game_id = state.game_id.to_string();
        let leader_id = opponent.leader.card_id.trim();

        let before = self.fingerprint();

        if self.watching.as_deref() != Some(game_id.as_str()) {
            self.ledger
                .begin_game(&game_id, leader_id, &opponent.deck_name, now);
            self.watching = Some(game_id);
        }
        // Either leader is often read a beat after the first cards are, and a
        // game with sightings but no leader is thrown away when it closes.
        self.ledger.backfill_leader(leader_id, &opponent.deck_name);
        let you = &state.players[YOU];
        self.ledger
            .record_your_leader(you.leader.card_id.trim(), &you.deck_name);
        self.ledger.record_life(you.life, opponent.life);

        for (card_id, copies) in visible_copies(state) {
            self.ledger.record_card(&card_id, copies, state.turn_number);
        }
        self.record_tempo(state);

        // Tempo alone moves on every update, idle or not, because the turn
        // counter is part of it. Only a game that has shown a card, or reached
        // a result, counts as having taught us something worth saving.
        self.fingerprint() != before
            && self
                .ledger
                .open
                .as_ref()
                .is_some_and(|game| game.counts_as_played())
    }

    /// Fold the game being watched into its profile.
    ///
    /// Worth calling on shutdown so a finished session is not left waiting for
    /// a next game that may never come.
    pub fn close(&mut self, now: &str) {
        self.ledger.close_open_game(now);
        self.watching = None;
    }

    fn record_tempo(&mut self, state: &GameState) {
        let opponent = &state.players[OPPONENT];
        let turn = state.turn_number;
        let mut tempo = self.ledger.open_tempo();

        let board = opponent.characters.len() as u32;
        if board > 0 && tempo.first_board_turn.is_none() {
            tempo.first_board_turn = Some(turn);
        }
        tempo.widest_board = tempo.widest_board.max(board);

        // Life is never reported as damage taken, so it has to be read as a
        // fall from the highest total seen this game. The life track already
        // holds that high-water mark, and keeping a second copy here would be
        // one more thing to reset on a new game.
        let life = self.life_track();
        let taken = life.your_high.saturating_sub(life.your_last);
        if taken > tempo.life_taken {
            tempo.life_taken = taken;
            if tempo.first_damage_turn.is_none() {
                tempo.first_damage_turn = Some(turn);
            }
        }

        tempo.last_turn = tempo.last_turn.max(turn);
        self.ledger.record_tempo(tempo);
    }

    fn life_track(&self) -> LifeTrack {
        self.ledger
            .open
            .as_ref()
            .map(|game| game.life.clone())
            .unwrap_or_default()
    }

    /// A cheap stand-in for "did the ledger change", so the app only writes to
    /// disk when there is something new to write.
    ///
    /// Life belongs here in its own right: their life falling is what decides a
    /// win, and it moves nothing in tempo, which only measures damage done to
    /// you.
    fn fingerprint(&self) -> (usize, u32, Tempo, LifeTrack) {
        let open = self.ledger.open.as_ref();
        (
            open.map_or(0, |g| g.sightings.len()),
            open.map_or(0, |g| g.sightings.iter().map(|s| s.copies).sum()),
            open.map(|g| g.tempo.clone()).unwrap_or_default(),
            self.life_track(),
        )
    }
}

/// Every opponent card currently accounted for, and how many of it.
///
/// Board and trash are counted together because both are public and a card in
/// either is a card they own. The count is a floor on the copies they run: a
/// fourth copy sitting in the deck all game is invisible and stays that way.
fn visible_copies(state: &GameState) -> Vec<(String, u32)> {
    let opponent = &state.players[OPPONENT];
    let mut counts: HashMap<&str, u32> = HashMap::new();

    for card in opponent
        .characters
        .iter()
        .chain(opponent.trash.iter())
        .chain(opponent.hand.iter().filter(|card| card.known))
    {
        let id = card.card_id.trim();
        if !id.is_empty() {
            *counts.entry(id).or_insert(0) += 1;
        }
    }

    // `known_cards` carries ids the adapter saw without keeping the instances,
    // so it can only ever establish that a card exists, not how many.
    for id in &opponent.known_cards {
        let id = id.trim();
        if !id.is_empty() && id != opponent.leader.card_id {
            counts.entry(id).or_insert(1);
        }
    }

    counts
        .into_iter()
        .map(|(id, copies)| (id.to_string(), copies))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_core::{CardInstance, GameState, Zone};

    const NOW: &str = "2026-01-01T00:00:00Z";
    const LEADER: &str = "OP17-079";
    const YOUR_LEADER: &str = "ST01-001";

    fn scout() -> Scout {
        Scout::new(ScoutingLedger::default())
    }

    /// A position with the opponent on `LEADER` and the given board.
    ///
    /// The game id is pinned, since `GameState::new` mints a fresh one and two
    /// readings of the same game must not look like two games.
    fn position(turn: u32, board: &[&str]) -> GameState {
        let mut state = GameState::new();
        state.game_id = uuid::Uuid::nil();
        state.turn_number = turn;
        state.players[YOU].life = 4;
        state.players[YOU].leader.card_id = YOUR_LEADER.into();
        state.players[OPPONENT].leader.card_id = LEADER.into();
        state.players[OPPONENT].characters = board
            .iter()
            .map(|id| CardInstance::new(*id, 1, Zone::Character))
            .collect();
        state
    }

    #[test]
    fn an_idle_position_is_not_recorded_as_a_game() {
        let mut scout = scout();
        let mut idle = GameState::new();
        idle.players[OPPONENT].leader.card_id = LEADER.into();

        assert!(
            !scout.observe(&idle, NOW),
            "a position with nothing revealed teaches nothing"
        );
        scout.close(NOW);
        assert!(
            scout.ledger().profile(LEADER).is_none(),
            "an idle HUD must not invent a game"
        );
    }

    #[test]
    fn a_card_on_their_board_is_written_down() {
        let mut scout = scout();

        assert!(scout.observe(&position(3, &["OP17-080"]), NOW));
        scout.close(NOW);

        let profile = scout.ledger().profile(LEADER).expect("a game was recorded");
        assert_eq!(profile.games, 1);
        assert_eq!(profile.cards[0].card_id, "OP17-080");
        assert_eq!(profile.cards[0].earliest_turn, 3);
    }

    #[test]
    fn two_copies_on_board_are_counted_as_two() {
        let mut scout = scout();
        scout.observe(&position(4, &["OP17-080", "OP17-080"]), NOW);
        scout.close(NOW);

        assert_eq!(
            scout.ledger().profile(LEADER).unwrap().cards[0].max_copies,
            2
        );
    }

    #[test]
    fn the_trash_counts_towards_copies_they_own() {
        let mut scout = scout();
        let mut state = position(6, &["OP17-080"]);
        state.players[OPPONENT].trash = vec![CardInstance::new("OP17-080", 1, Zone::Trash)];

        scout.observe(&state, NOW);
        scout.close(NOW);

        assert_eq!(
            scout.ledger().profile(LEADER).unwrap().cards[0].max_copies,
            2,
            "one on board and one in the trash is two they own"
        );
    }

    #[test]
    fn an_unchanged_position_teaches_nothing_the_second_time() {
        let mut scout = scout();
        let state = position(3, &["OP17-080"]);

        assert!(scout.observe(&state, NOW));
        assert!(
            !scout.observe(&state, NOW),
            "re-reading the same position should not ask the app to save"
        );
    }

    #[test]
    fn a_new_game_id_folds_the_last_game_in() {
        let mut scout = scout();
        scout.observe(&position(3, &["OP17-080"]), NOW);

        let mut next = position(1, &["OP17-081"]);
        next.game_id = uuid::Uuid::new_v4();
        scout.observe(&next, NOW);

        let profile = scout.ledger().profile(LEADER).expect("first game folded");
        assert_eq!(profile.games, 1);
        assert_eq!(profile.cards[0].card_id, "OP17-080");
    }

    #[test]
    fn the_leader_is_backfilled_when_it_is_read_late() {
        let mut scout = scout();
        // Cards resolve before the leader does on a fresh connection.
        let mut early = position(2, &["OP17-080"]);
        early.players[OPPONENT].leader.card_id = String::new();
        scout.observe(&early, NOW);

        scout.observe(&position(3, &["OP17-080"]), NOW);
        scout.close(NOW);

        assert!(
            scout.ledger().profile(LEADER).is_some(),
            "a game whose leader arrived late is still a game against that leader"
        );
    }

    #[test]
    fn life_lost_is_recorded_as_damage_they_dealt() {
        let mut scout = scout();
        scout.observe(&position(3, &["OP17-080"]), NOW);

        let mut hit = position(4, &["OP17-080"]);
        hit.players[YOU].life = 2;
        scout.observe(&hit, NOW);
        scout.close(NOW);

        let tempo = &scout.ledger().profile(LEADER).unwrap().tempo;
        assert_eq!(tempo.summed_life_taken, 2);
        assert_eq!(
            tempo.summed_first_damage_turn, 4,
            "the turn the first life went is the tempo signal"
        );
    }

    #[test]
    fn their_widest_board_is_remembered_after_it_shrinks() {
        let mut scout = scout();
        scout.observe(&position(5, &["OP17-080", "OP17-081", "OP17-082"]), NOW);
        scout.observe(&position(6, &["OP17-080"]), NOW);
        scout.close(NOW);

        assert_eq!(
            scout.ledger().profile(LEADER).unwrap().tempo.widest_board,
            3
        );
    }

    #[test]
    fn losing_your_last_life_is_recorded_as_a_loss() {
        let mut scout = scout();
        scout.observe(&position(3, &["OP17-080"]), NOW);

        let mut lethal = position(6, &["OP17-080"]);
        lethal.players[YOU].life = 0;
        scout.observe(&lethal, NOW);
        scout.close(NOW);

        let record = scout
            .ledger()
            .matchups
            .record(YOUR_LEADER, LEADER)
            .expect("a finished game belongs to a matchup");
        assert_eq!((record.wins, record.losses), (0, 1));
        assert_eq!(record.average_length(), Some(6.0));
    }

    #[test]
    fn taking_their_last_life_is_recorded_as_a_win() {
        let mut scout = scout();
        scout.observe(&position(3, &["OP17-080"]), NOW);

        let mut lethal = position(8, &["OP17-080"]);
        lethal.players[OPPONENT].life = 0;
        scout.observe(&lethal, NOW);
        scout.close(NOW);

        let record = scout.ledger().matchups.record(YOUR_LEADER, LEADER).unwrap();
        assert_eq!((record.wins, record.losses), (1, 0));
        assert_eq!(
            record.average_life_left_on_win(),
            Some(4.0),
            "how much life a win was won with is what says whether it was comfortable"
        );
    }

    #[test]
    fn a_game_that_never_finished_is_not_recorded_as_a_loss() {
        let mut scout = scout();
        scout.observe(&position(3, &["OP17-080"]), NOW);
        // Neither side dies: a disconnect, or the app closing mid-game.
        scout.close(NOW);

        let record = scout.ledger().matchups.record(YOUR_LEADER, LEADER).unwrap();
        assert_eq!((record.wins, record.losses), (0, 0));
        assert_eq!(record.unfinished, 1);
        assert_eq!(
            record.win_rate(),
            None,
            "walking away from a game is not losing it"
        );
    }

    #[test]
    fn an_idle_hud_records_no_matchup_at_all() {
        let mut scout = scout();
        let mut idle = GameState::new();
        idle.game_id = uuid::Uuid::nil();
        idle.players[YOU].leader.card_id = YOUR_LEADER.into();
        idle.players[OPPONENT].leader.card_id = LEADER.into();

        for _ in 0..10 {
            scout.observe(&idle, NOW);
        }
        scout.close(NOW);

        assert!(
            scout.ledger().matchups.records.is_empty(),
            "leaving the HUD open must not manufacture a record: {:?}",
            scout.ledger().matchups.records
        );
    }

    #[test]
    fn a_result_is_recorded_even_when_they_showed_no_cards() {
        let mut scout = scout();
        // A game won without the adapter ever resolving one of their cards.
        // Dropping it would bias the record towards whichever kind of game
        // happens to reveal more.
        let mut opening = position(2, &[]);
        opening.players[OPPONENT].life = 5;
        scout.observe(&opening, NOW);
        let mut lethal = position(4, &[]);
        lethal.players[OPPONENT].life = 0;
        scout.observe(&lethal, NOW);
        scout.close(NOW);

        let record = scout.ledger().matchups.record(YOUR_LEADER, LEADER).unwrap();
        assert_eq!(record.wins, 1);
        assert!(
            scout.ledger().profile(LEADER).is_none(),
            "a game that revealed no cards still teaches nothing about their deck"
        );
    }

    #[test]
    fn the_matchup_is_keyed_on_your_leader_too() {
        let mut scout = scout();
        let mut other_deck = position(3, &["OP17-080"]);
        other_deck.players[YOU].leader.card_id = "OP01-002".into();
        other_deck.players[YOU].life = 0;
        scout.observe(&other_deck, NOW);
        scout.close(NOW);

        assert!(
            scout
                .ledger()
                .matchups
                .record(YOUR_LEADER, LEADER)
                .is_none(),
            "a result belongs to the deck that played it"
        );
        assert!(scout.ledger().matchups.record("OP01-002", LEADER).is_some());
    }

    #[test]
    fn results_accumulate_across_games() {
        let mut scout = scout();
        for n in 0..4 {
            let mut opening = position(3, &["OP17-080"]);
            opening.game_id = uuid::Uuid::from_u128(n);
            scout.observe(&opening, NOW);

            let mut end = position(7, &["OP17-080"]);
            end.game_id = uuid::Uuid::from_u128(n);
            // Win the first two, lose the rest.
            if n < 2 {
                end.players[OPPONENT].life = 0;
            } else {
                end.players[YOU].life = 0;
            }
            scout.observe(&end, NOW);
        }
        scout.close(NOW);

        let record = scout.ledger().matchups.record(YOUR_LEADER, LEADER).unwrap();
        assert_eq!((record.wins, record.losses), (2, 2));
        assert_eq!(record.win_rate(), Some(0.5));
    }

    #[test]
    fn their_leader_is_not_recorded_as_a_card_in_their_deck() {
        let mut scout = scout();
        let mut state = position(3, &["OP17-080"]);
        state.players[OPPONENT].known_cards = vec![LEADER.into(), "OP17-080".into()];

        scout.observe(&state, NOW);
        scout.close(NOW);

        let profile = scout.ledger().profile(LEADER).unwrap();
        assert!(
            !profile.cards.iter().any(|c| c.card_id == LEADER),
            "the leader is not one of the fifty: {:?}",
            profile.cards
        );
    }
}
