use crate::dto::ObservationStatusDto;
use crate::dto::{
    ConnectionStatusDto, DeckCollectionDto, DeckInfoDto, DeckOrigin, GameStateDto, KnownCardDto,
    MatchReviewDto, MatchupReportDto, OverlaySettings, PastedDeckDto, SavedDeckDto, ScoutedCardDto,
    ScoutingReportDto, StateUpdatePayload,
};
use optcg_database::Database;
use optcg_rules::{
    BeamSearch, BeamSearchConfig, CombatMath, DeckCollection, DeckProfile, DeckStrategyBrief,
    DeckStrategyCoach, MctsConfig, MctsEngine, PastedDeckList, RulesEngine, SavedDeck, Side,
    MAX_DECKS,
};
use optcg_scouting::{DeckMap, MatchupRead, Scout, ScoutingLedger, StrategyRead};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// A side's deck as the app currently understands it.
struct ResolvedDeck {
    list: Option<PastedDeckList>,
    origin: DeckOrigin,
    /// The saved deck the list came from, when it came from one.
    deck_id: Option<String>,
}

pub struct AppState {
    pub database: Database,
    pub game_state: Arc<RwLock<optcg_core::GameState>>,
    pub beam: BeamSearch,
    pub mcts: MctsEngine,
    pub overlay: RwLock<OverlaySettings>,
    /// Cached deck strategy brief; refreshed on demand or when matchup changes.
    pub deck_strategy: RwLock<Option<DeckStrategyBrief>>,
    /// Parsed list for the active deck, used by all deck-aware coaching.
    pub pasted_deck: RwLock<Option<PastedDeckList>>,
    /// The user's saved deck library.
    pub deck_collection: RwLock<DeckCollection>,
    /// Lists parsed out of saved decks, keyed by deck id. Resolving a side runs
    /// on every state update, and parsing hits the card DB for each entry.
    list_cache: RwLock<HashMap<String, PastedDeckList>>,
    /// What every opponent has shown us, accumulated across games.
    pub scout: RwLock<Scout>,
    pasted_deck_path: PathBuf,
    collection_path: PathBuf,
    scouting_path: PathBuf,
    /// Tournament top-cut lists from `scripts/sync_meta.py`, when downloaded.
    meta: RwLock<Option<optcg_rules::MetaData>>,
    meta_path: PathBuf,
    /// Standard-format ban list, pair bans and rotation (`banlist.json`).
    banlist: RwLock<Option<optcg_rules::BanList>>,
    banlist_path: PathBuf,
    /// Watches the observed game and says when a match ended.
    match_tracker: Mutex<optcg_rules::MatchTracker>,
    /// Finished matches, kept on this machine only.
    match_store: optcg_rules::MatchStore,
}

impl AppState {
    pub fn new(
        database: Database,
        game_state: Arc<RwLock<optcg_core::GameState>>,
        data_dir: PathBuf,
    ) -> Self {
        let pasted_deck_path = data_dir.join("pasted_deck.txt");
        let collection_path = data_dir.join("deck_collection.json");
        let scouting_path = data_dir.join("scouting.json");
        let scout = Scout::new(ScoutingLedger::load(&scouting_path));
        let mut state = Self {
            database,
            game_state,
            beam: BeamSearch::new(BeamSearchConfig {
                beam_width: 4,
                max_depth: 3,
            }),
            mcts: MctsEngine::new(MctsConfig {
                iterations: 100,
                ..Default::default()
            }),
            overlay: RwLock::new(OverlaySettings {
                click_through: false,
                opacity: 0.92,
            }),
            deck_strategy: RwLock::new(None),
            pasted_deck: RwLock::new(None),
            deck_collection: RwLock::new(DeckCollection::default()),
            list_cache: RwLock::new(HashMap::new()),
            scout: RwLock::new(scout),
            pasted_deck_path,
            collection_path,
            scouting_path,
            meta: RwLock::new(None),
            meta_path: data_dir.join("meta_decks.json"),
            banlist: RwLock::new(None),
            banlist_path: data_dir.join("banlist.json"),
            match_tracker: Mutex::new(optcg_rules::MatchTracker::default()),
            match_store: optcg_rules::MatchStore::new(data_dir.join("matches.jsonl")),
        };
        state.reload_meta();
        state.load_collection_from_disk();
        state
    }

    pub fn repo(&self) -> optcg_database::CardRepository<'_> {
        optcg_database::CardRepository::new(&self.database)
    }

    /// Restore the saved collection at boot, importing the pre-collection
    /// `pasted_deck.txt` the first time so an upgrading user keeps their deck.
    fn load_collection_from_disk(&mut self) {
        let mut collection = DeckCollection::load(&self.collection_path);
        if !self.collection_path.exists() {
            if let Some(list) = self.legacy_pasted_deck() {
                let name = self.deck_display_name(&list);
                if collection
                    .upsert(
                        None,
                        &name,
                        &list.raw,
                        list.leader_id.clone(),
                        list.total_cards,
                        &now_rfc3339(),
                    )
                    .is_ok()
                {
                    let _ = collection.save(&self.collection_path);
                }
            }
        }

        let active = collection.active().cloned();
        *self.deck_collection.get_mut() = collection;
        // Strategy is left unbuilt here; the first state payload builds it lazily.
        if let Some(deck) = active {
            self.load_active_list(&deck);
        }
    }

    fn legacy_pasted_deck(&self) -> Option<PastedDeckList> {
        let raw = std::fs::read_to_string(&self.pasted_deck_path).ok()?;
        if raw.trim().is_empty() {
            return None;
        }
        let list = PastedDeckList::parse(raw.trim(), &self.repo());
        if list.is_empty() {
            None
        } else {
            Some(list)
        }
    }

    /// Name to show for a list that carries no explicit `Deck:` line.
    fn deck_display_name(&self, list: &PastedDeckList) -> String {
        if let Some(name) = list.name.as_ref() {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
        if let Some(def) = list
            .leader_id
            .as_ref()
            .and_then(|id| self.repo().get_by_id(id).ok())
        {
            return if def.color.is_empty() {
                def.name
            } else {
                format!("{} {}", def.color, def.name)
            };
        }
        "Untitled deck".to_string()
    }

    fn parse_deck_raw(&self, raw: &str) -> Result<PastedDeckList, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("Deck list is empty".into());
        }
        let list = PastedDeckList::parse(trimmed, &self.repo());
        if list.entries.is_empty() {
            return Err(if list.warnings.is_empty() {
                "Could not parse any cards from the pasted list".into()
            } else {
                format!(
                    "Could not parse cards. Warnings: {}",
                    list.warnings.join("; ")
                )
            });
        }
        Ok(list)
    }

    /// Re-parse a saved deck into the active list. Parsing on activation rather
    /// than caching entries means card database updates apply to old saves.
    fn load_active_list(&self, deck: &SavedDeck) {
        let mut list = PastedDeckList::parse(deck.raw.trim(), &self.repo());
        if list.is_empty() {
            return;
        }
        // The collection name is authoritative, so renames show up in the HUD.
        list.name = Some(deck.name.clone());
        *self.pasted_deck.write() = Some(list);
    }

    fn activate_locally(&self, deck: &SavedDeck) {
        self.load_active_list(deck);
        let _ = self.refresh_deck_strategy();
    }

    fn persist_collection(&self, collection: &DeckCollection) {
        if let Err(e) = collection.save(&self.collection_path) {
            tracing::warn!(error = %e, "failed to persist deck collection");
        }
    }

    /// Deck identity for both sides, as shown in the HUD.
    pub fn deck_infos(&self) -> (DeckInfoDto, DeckInfoDto) {
        let gs = self.game_state.read();
        let yours = self.deck_info_for(gs.player_one(), Side::You);
        let opponent = self.deck_info_for(gs.player_two(), Side::Opponent);
        (yours, opponent)
    }

    /// Attach a saved list to a side, or pass `None` to read that side from
    /// play instead.
    pub fn set_deck_source(&self, side: Side, deck_id: Option<&str>) -> Result<(), String> {
        {
            let mut collection = self.deck_collection.write();
            if !collection.attach(side, deck_id) {
                return Err(format!(
                    "No saved deck with id '{}'",
                    deck_id.unwrap_or_default()
                ));
            }
            self.persist_collection(&collection);
        }
        // Your side keeps `pasted_deck` in step, since the HUD and the legacy
        // paste commands both read it.
        if side == Side::You {
            match self.deck_collection.read().active().cloned() {
                Some(deck) => self.load_active_list(&deck),
                None => *self.pasted_deck.write() = None,
            }
        }
        let _ = self.refresh_deck_strategy();
        Ok(())
    }

    /// Read the live position for anything it says about the opponent's deck.
    ///
    /// Called on every observed update and cheap when nothing is new: the scout
    /// only reports having learned something when a card or a tempo
    /// measurement actually moved, and only then is the ledger written out.
    pub fn scout_position(&self) {
        let learned = {
            let state = self.game_state.read();
            let mut scout = self.scout.write();
            scout.observe(&state, &now_rfc3339())
        };
        if learned {
            self.persist_scouting();
        }
    }

    /// Keep an automatic read on the open game so the recap can write orders.
    pub fn record_coach_read(&self, turn: u32, line: &str) {
        let learned = {
            let mut scout = self.scout.write();
            scout.record_auto_read(turn, line)
        };
        if learned {
            self.persist_scouting();
        }
    }

    /// Standing orders for this pairing, if any games have written them.
    pub fn standing_orders_for(&self, your_leader: &str, their_leader: &str) -> Vec<String> {
        if your_leader.is_empty() || their_leader.is_empty() {
            return Vec::new();
        }
        self.scout
            .read()
            .ledger()
            .matchups
            .record(your_leader, their_leader)
            .map(|record| record.orders.iter().map(|o| o.text.clone()).collect())
            .unwrap_or_default()
    }

    /// Fold the game being watched into its profile and save.
    ///
    /// Worth doing on shutdown, so a session that ends without another game
    /// starting still adds up to something.
    pub fn close_scouting_game(&self) {
        self.scout.write().close(&now_rfc3339());
        self.persist_scouting();
    }

    /// Everything learned about a leader, including the game under way.
    pub fn scouting_for(&self, leader_id: &str) -> Option<optcg_scouting::LeaderProfile> {
        if leader_id.is_empty() {
            return None;
        }
        self.scout.read().ledger().merged_profile(leader_id)
    }

    /// The scouting read on a leader, shaped for the HUD.
    pub fn scouting_report(&self, leader_id: &str) -> Option<ScoutingReportDto> {
        let profile = self.scouting_for(leader_id)?;
        let map = DeckMap::from_profile(&profile)?;
        let read = StrategyRead::from_profile(&profile);
        let repo = self.repo();

        // The recorded name is the simulator's deck label, which is often not
        // visible; the leader's own card name is the better fallback.
        let leader_name = if map.leader_name.trim().is_empty() {
            repo.get_by_id(&map.leader_id)
                .map(|def| def.name)
                .unwrap_or_else(|_| map.leader_id.clone())
        } else {
            map.leader_name.clone()
        };

        Some(ScoutingReportDto {
            leader_id: map.leader_id.clone(),
            leader_name,
            games: map.games,
            reliability: map.reliability.label().to_string(),
            pace: read
                .as_ref()
                .map(|read| read.pace.label().to_string())
                .unwrap_or_else(|| "not yet established".into()),
            mapped_copies: map.mapped_copies(),
            cards: map
                .cards
                .iter()
                .map(|card| ScoutedCardDto {
                    card_id: card.card_id.clone(),
                    name: repo
                        .get_by_id(&card.card_id)
                        .map(|def| def.name)
                        .unwrap_or_else(|_| card.card_id.clone()),
                    games_seen: card.games_seen,
                    confidence: card.confidence,
                    likely_copies: card.likely_copies,
                    earliest_turn: card.earliest_turn,
                })
                .collect(),
            notes: read.map(|read| read.notes).unwrap_or_default(),
        })
    }

    /// Your deck's record against a leader, shaped for the HUD.
    ///
    /// Unlike the deck map, this is not merged with the game under way: the
    /// current game has no result yet, and it is the one being played.
    pub fn matchup_report(
        &self,
        your_leader: &str,
        their_leader: &str,
    ) -> Option<MatchupReportDto> {
        if your_leader.is_empty() || their_leader.is_empty() {
            return None;
        }
        let read = {
            let scout = self.scout.read();
            let record = scout.ledger().matchups.record(your_leader, their_leader)?;
            MatchupRead::from_record(record)?
        };

        // The stored name is the simulator's deck label, often blank; the
        // leader's own card name is the better fallback.
        let leader_name = if read.their_leader_name.trim().is_empty() {
            self.repo()
                .get_by_id(&read.their_leader)
                .map(|def| def.name)
                .unwrap_or_else(|_| read.their_leader.clone())
        } else {
            read.their_leader_name.clone()
        };

        Some(MatchupReportDto {
            their_leader_id: read.their_leader.clone(),
            their_leader_name: leader_name,
            wins: read.wins,
            losses: read.losses,
            unfinished: read.unfinished,
            standing: read.standing.label().to_string(),
            win_rate: read.win_rate,
            notes: read.notes,
            orders: read.orders,
        })
    }

    /// How the last finished game went, named for the HUD.
    pub fn latest_review(&self) -> Option<MatchReviewDto> {
        let (review, standing) = {
            let scout = self.scout.read();
            let review = scout.ledger().latest_review()?.clone();
            let standing = scout
                .ledger()
                .matchups
                .record(&review.your_leader, &review.their_leader)
                .filter(|r| r.finished() > 0)
                .map(|r| (r.wins, r.losses));
            Some((review, standing))
        }?;
        let repo = self.repo();
        let name = |id: &str, stored: &str| {
            if !stored.trim().is_empty() {
                return stored.to_string();
            }
            repo.get_by_id(id)
                .map(|def| def.name)
                .unwrap_or_else(|_| id.to_string())
        };
        let you_played = review
            .you_played
            .iter()
            .map(|id| {
                repo.get_by_id(id)
                    .map(|def| format!("{} ({id})", def.name))
                    .unwrap_or_else(|_| id.clone())
            })
            .collect();
        let mut notes = review.notes();
        if let Some((wins, losses)) = standing {
            notes.push(format!("That's {wins}-{losses} against this leader."));
        }
        Some(MatchReviewDto {
            game_id: review.game_id.clone(),
            outcome: review.outcome.map(|o| match o {
                optcg_scouting::Outcome::Won => "won".into(),
                optcg_scouting::Outcome::Lost => "lost".into(),
            }),
            headline: review.headline(),
            cheer: review.cheer(),
            your_leader: name(&review.your_leader, &review.your_leader_name),
            their_leader: name(&review.their_leader, &review.their_leader_name),
            last_turn: review.last_turn,
            your_life: review.your_life,
            their_life: review.their_life,
            notes,
            you_played,
        })
    }

    fn persist_scouting(&self) {
        let ledger = self.scout.read();
        if let Err(e) = ledger.ledger().save(&self.scouting_path) {
            tracing::warn!(error = %e, "failed to persist scouting ledger");
        }
    }

    /// The list to coach a side with, and how confident we get to be about it.
    ///
    /// A side with a list attached uses it outright. Otherwise the side is read
    /// from play: a leader we hold exactly one saved list for brings that list
    /// back as a presumed reading, and anything else leaves us with just the
    /// leader and whatever cards the table has revealed.
    fn resolve_deck(&self, side: Side, leader_id: &str, observed: bool) -> ResolvedDeck {
        // A placeholder leader is not a read. Attaching or presuming a list
        // against it would paint the wrong deck on the scoreboard.
        if !observed || leader_id.is_empty() {
            let attached = {
                let collection = self.deck_collection.read();
                collection
                    .attached(side)
                    .map(|deck| (deck.id.clone(), deck.name.clone(), deck.raw.clone()))
            };
            if let Some((id, name, raw)) = attached {
                if let Some(list) = self.parsed_list(&id, &name, &raw) {
                    return ResolvedDeck {
                        list: Some(list),
                        origin: DeckOrigin::Attached,
                        deck_id: Some(id),
                    };
                }
            }
            return ResolvedDeck {
                list: None,
                origin: DeckOrigin::Observed,
                deck_id: None,
            };
        }
        let (attached, presumed) = {
            let collection = self.deck_collection.read();
            let attached = collection
                .attached(side)
                .map(|deck| (deck.id.clone(), deck.name.clone(), deck.raw.clone()));
            let presumed = collection
                .presumed_for(side, leader_id)
                .map(|deck| (deck.id.clone(), deck.name.clone(), deck.raw.clone()));
            (attached, presumed)
        };

        for (candidate, origin) in [
            (attached, DeckOrigin::Attached),
            (presumed, DeckOrigin::Presumed),
        ] {
            let Some((id, name, raw)) = candidate else {
                continue;
            };
            if let Some(list) = self.parsed_list(&id, &name, &raw) {
                return ResolvedDeck {
                    list: Some(list),
                    origin,
                    deck_id: Some(id),
                };
            }
        }
        ResolvedDeck {
            list: None,
            origin: DeckOrigin::Observed,
            deck_id: None,
        }
    }

    /// Parse a saved deck's raw text, reusing the last parse of that same text.
    ///
    /// Parsing rather than storing entries means card database updates reach
    /// lists saved long ago; the cache keys on the raw text so an edit is
    /// always re-read.
    fn parsed_list(&self, id: &str, name: &str, raw: &str) -> Option<PastedDeckList> {
        let raw = raw.trim();
        if let Some(hit) = self.list_cache.read().get(id) {
            if hit.raw == raw && hit.name.as_deref() == Some(name) {
                return Some(hit.clone());
            }
        }
        let mut list = PastedDeckList::parse(raw, &self.repo());
        if list.is_empty() {
            return None;
        }
        // The collection name is authoritative, so renames show up in the HUD.
        list.name = Some(name.to_string());
        self.list_cache.write().insert(id.to_string(), list.clone());
        Some(list)
    }

    /// Last built strategy brief, without forcing a rebuild.
    pub fn cached_deck_strategy(&self) -> Option<DeckStrategyBrief> {
        self.deck_strategy.read().clone()
    }

    pub fn deck_collection_dto(&self) -> DeckCollectionDto {
        let collection = self.deck_collection.read();
        let repo = self.repo();
        let active_id = collection.active_id.clone();
        let decks = collection
            .decks
            .iter()
            .map(|deck| {
                let leader = deck
                    .leader_id
                    .as_ref()
                    .and_then(|id| repo.get_by_id(id).ok());
                SavedDeckDto {
                    id: deck.id.clone(),
                    name: deck.name.clone(),
                    raw: deck.raw.clone(),
                    leader_id: deck.leader_id.clone(),
                    leader_name: leader.as_ref().map(|def| def.name.clone()),
                    leader_color: leader
                        .as_ref()
                        .map(|def| def.color.clone())
                        .filter(|color| !color.is_empty()),
                    total_cards: deck.total_cards,
                    is_active: active_id.as_deref() == Some(deck.id.as_str()),
                    updated_at: deck.updated_at.clone(),
                }
            })
            .collect();

        DeckCollectionDto {
            decks,
            active_id,
            opponent_id: collection.opponent_id.clone(),
            max_decks: MAX_DECKS,
        }
    }

    /// Save a deck list into the collection and attach it to `side`.
    ///
    /// `id` targets an existing deck; without it the deck is matched by name so
    /// re-pasting the same list updates it rather than creating a duplicate.
    ///
    /// Saving a list for the opponent must not disturb your own side, which is
    /// what makes it possible to keep their lists around at all.
    pub fn save_deck(
        &self,
        side: Side,
        id: Option<&str>,
        name: Option<&str>,
        raw: &str,
    ) -> Result<SavedDeck, String> {
        let list = self.parse_deck_raw(raw)?;
        let display_name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(explicit) => explicit.to_string(),
            None => self.deck_display_name(&list),
        };

        let saved = {
            let mut collection = self.deck_collection.write();
            let previously_yours = collection.active_id.clone();
            let deck_id = collection.upsert(
                id,
                &display_name,
                raw.trim(),
                list.leader_id.clone(),
                list.total_cards,
                &now_rfc3339(),
            )?;
            // `upsert` claims your side for the new deck, which is right when
            // you are saving your own list and wrong when you are saving
            // theirs.
            if side == Side::Opponent {
                collection.active_id = previously_yours;
                collection.opponent_id = Some(deck_id.clone());
            }
            let saved = collection
                .get(&deck_id)
                .cloned()
                .ok_or_else(|| "Saved deck vanished after write".to_string())?;
            self.persist_collection(&collection);
            saved
        };

        match side {
            Side::You => self.activate_locally(&saved),
            Side::Opponent => {
                let _ = self.refresh_deck_strategy();
            }
        }
        Ok(saved)
    }

    pub fn activate_deck(&self, id: &str) -> Result<SavedDeck, String> {
        let deck = {
            let mut collection = self.deck_collection.write();
            if !collection.set_active(id) {
                return Err(format!("No saved deck with id '{id}'"));
            }
            let deck = collection
                .get(id)
                .cloned()
                .ok_or_else(|| format!("No saved deck with id '{id}'"))?;
            self.persist_collection(&collection);
            deck
        };

        self.activate_locally(&deck);
        Ok(deck)
    }

    pub fn delete_deck(&self, id: &str) -> Result<(), String> {
        let (was_active, promoted) = {
            let mut collection = self.deck_collection.write();
            let was_active = collection.active_id.as_deref() == Some(id);
            if !collection.remove(id) {
                return Err(format!("No saved deck with id '{id}'"));
            }
            let promoted = if was_active {
                collection.active().cloned()
            } else {
                None
            };
            self.persist_collection(&collection);
            (was_active, promoted)
        };

        if was_active {
            match promoted {
                Some(deck) => self.activate_locally(&deck),
                None => self.clear_active_deck(),
            }
        }
        Ok(())
    }

    pub fn rename_deck(&self, id: &str, name: &str) -> Result<SavedDeck, String> {
        let (deck, is_active) = {
            let mut collection = self.deck_collection.write();
            collection.rename(id, name)?;
            let deck = collection
                .get(id)
                .cloned()
                .ok_or_else(|| format!("No saved deck with id '{id}'"))?;
            let is_active = collection.active_id.as_deref() == Some(id);
            self.persist_collection(&collection);
            (deck, is_active)
        };

        if is_active {
            self.activate_locally(&deck);
        }
        Ok(deck)
    }

    /// Deselect the active deck without deleting anything from the collection.
    pub fn clear_active_deck(&self) {
        {
            let mut collection = self.deck_collection.write();
            collection.active_id = None;
            self.persist_collection(&collection);
        }
        *self.pasted_deck.write() = None;
        *self.deck_strategy.write() = None;
        let _ = self.refresh_deck_strategy();
    }

    pub fn set_pasted_deck(&self, raw: &str) -> Result<PastedDeckList, String> {
        self.save_deck(Side::You, None, None, raw)?;
        self.pasted_deck
            .read()
            .clone()
            .ok_or_else(|| "Deck list could not be activated".to_string())
    }

    pub fn clear_pasted_deck(&self) {
        self.clear_active_deck();
    }

    fn profile_from_deck(&self, deck: &DeckInfoDto) -> DeckProfile {
        DeckProfile {
            name: deck.name.clone(),
            leader_id: deck.leader_id.clone(),
            leader_name: deck.leader_name.clone(),
            leader_color: deck.leader_color.clone(),
            leader_text: deck.leader_text.clone(),
            known_card_ids: deck.known_cards.iter().map(|c| c.card_id.clone()).collect(),
            known_card_names: deck.known_cards.iter().map(|c| c.name.clone()).collect(),
            list_entries: deck.list_entries.clone(),
            list_total_cards: deck.list_total_cards,
        }
    }

    /// Rebuild detailed deck-vs-deck strategy for the current matchup.
    pub fn refresh_deck_strategy(&self) -> DeckStrategyBrief {
        let gs = self.game_state.read();
        let your_deck = self.deck_info_for(gs.player_one(), Side::You);
        let opponent_deck = self.deck_info_for(gs.player_two(), Side::Opponent);
        let brief = DeckStrategyCoach::brief(
            &gs,
            &self.profile_from_deck(&your_deck),
            &self.profile_from_deck(&opponent_deck),
        );
        *self.deck_strategy.write() = Some(brief.clone());
        brief
    }

    fn ensure_deck_strategy(
        &self,
        your_deck: &DeckInfoDto,
        opponent_deck: &DeckInfoDto,
        gs: &optcg_core::GameState,
    ) -> DeckStrategyBrief {
        let paste_sig = your_deck.list_total_cards;
        let matchup = format!(
            "{} vs {}|paste:{paste_sig}",
            your_deck.name, opponent_deck.name
        );
        let mut cache = self.deck_strategy.write();
        let needs_refresh = match cache.as_ref() {
            None => true,
            // Remap: store matchup field without paste sig; compare via list presence + name.
            Some(existing) => {
                existing.matchup != format!("{} vs {}", your_deck.name, opponent_deck.name)
                    || existing.list_notes.is_empty() != your_deck.list_entries.is_empty()
            }
        };
        let _ = matchup; // used for clarity above
        if needs_refresh {
            let brief = DeckStrategyCoach::brief(
                gs,
                &self.profile_from_deck(your_deck),
                &self.profile_from_deck(opponent_deck),
            );
            *cache = Some(brief.clone());
            brief
        } else {
            cache.as_ref().unwrap().clone()
        }
    }

    fn deck_info_for(&self, player: &optcg_core::PlayerState, side: Side) -> DeckInfoDto {
        let repo = self.repo();
        let table_read = player.leader.observed;
        let table_id = if table_read {
            player.leader.card_id.clone()
        } else {
            String::new()
        };
        let ResolvedDeck {
            list: pasted,
            origin,
            deck_id,
        } = self.resolve_deck(side, &table_id, table_read);

        // The table leader wins. An attached list may name a leader for Setup
        // and the coach before kickoff — the scoreboard ignores that until
        // `leader_observed` is set.
        let mut leader_id = table_id;
        if leader_id.is_empty() && origin == DeckOrigin::Attached {
            if let Some(id) = pasted.as_ref().and_then(|p| p.leader_id.clone()) {
                leader_id = id;
            }
        }

        let printed = player.leader_name.trim();
        let (leader_name, leader_color, leader_text) = match repo.get_by_id(&leader_id) {
            Ok(def) => {
                let name = if def.name.is_empty() && !printed.is_empty() {
                    printed.to_string()
                } else if def.name.is_empty() {
                    leader_id.clone()
                } else {
                    def.name
                };
                (name, def.color, def.rules_text)
            }
            Err(_) => {
                if let Some(def) = (!printed.is_empty())
                    .then(|| repo.search_by_name(printed, 6).ok())
                    .flatten()
                    .and_then(|hits| {
                        hits.into_iter().find(|d| {
                            d.card_type == optcg_core::CardType::Leader
                                && d.name.eq_ignore_ascii_case(printed)
                        })
                    })
                {
                    (def.name, def.color, def.rules_text)
                } else if !printed.is_empty() {
                    (printed.to_string(), String::new(), String::new())
                } else if leader_id.is_empty() {
                    ("Unknown leader".into(), String::new(), String::new())
                } else {
                    (leader_id.clone(), String::new(), String::new())
                }
            }
        };

        let mut known_cards: Vec<KnownCardDto> = player
            .known_cards
            .iter()
            .filter(|id| !id.is_empty())
            .map(|id| match repo.get_by_id(id) {
                Ok(def) => KnownCardDto {
                    card_id: id.clone(),
                    name: def.name,
                    card_type: format!("{:?}", def.card_type).to_lowercase(),
                    color: def.color,
                },
                Err(_) => KnownCardDto {
                    card_id: id.clone(),
                    name: id.clone(),
                    card_type: "unknown".into(),
                    color: String::new(),
                },
            })
            .collect();

        let (list_entries, list_total_cards, list_warnings, paste_name) = match pasted {
            Some(p) => {
                // A list you attached by hand is something you know, so it
                // joins the known cards. A presumed list is a guess, and
                // folding a guess in here would let the coach talk about cards
                // this opponent may never have drafted, let alone revealed.
                if origin == DeckOrigin::Attached {
                    for e in &p.entries {
                        if !known_cards.iter().any(|k| k.card_id == e.card_id) {
                            known_cards.push(KnownCardDto {
                                card_id: e.card_id.clone(),
                                name: e.name.clone(),
                                card_type: e.card_type.clone(),
                                color: e.color.clone(),
                            });
                        }
                    }
                }
                (p.entries, p.total_cards, p.warnings, p.name)
            }
            None => (Vec::new(), 0, Vec::new(), None),
        };

        known_cards.sort_by(|a, b| a.name.cmp(&b.name));

        // A saved list name is the Setup / coach label. The scoreboard reads
        // `leader_name` / `leader_id` from the table, not this field.
        let name = paste_name
            .filter(|n| !n.trim().is_empty())
            .or_else(|| {
                let n = player.deck_name.trim();
                (!n.is_empty()).then(|| n.to_string())
            })
            .unwrap_or_else(|| {
                if !leader_color.is_empty()
                    && leader_name != "Unknown leader"
                    && !leader_name.is_empty()
                {
                    format!("{leader_color} {leader_name}")
                } else if leader_name != "Unknown leader" && !leader_name.is_empty() {
                    leader_name.clone()
                } else if !leader_id.is_empty() {
                    format!("Deck · {leader_id}")
                } else {
                    "Deck unknown".into()
                }
            });

        DeckInfoDto {
            name,
            leader_id,
            leader_name,
            leader_color,
            leader_text,
            known_cards,
            origin,
            deck_id,
            list_entries,
            list_total_cards,
            list_warnings,
        }
    }

    /// Feed the current game to the match recorder. Returns true when the
    /// saved matches changed, so the HUD can refresh its history.
    pub fn track_match(&self) -> bool {
        let observed = {
            let gs = self.game_state.read();
            optcg_rules::Observed {
                now_ms: chrono::Utc::now().timestamp_millis(),
                your_leader: gs.player_one().observed_leader().map(str::to_string),
                opponent_leader: gs.player_two().observed_leader().map(str::to_string),
                turn: gs.turn_number,
                active_player: gs.active_player,
                your_deck: String::new(),
                your_cards: gs.player_one().known_cards.clone(),
                // Public cards only: what has been on the table, never a hand.
                opponent_cards: gs.player_two().known_cards.clone(),
                outcome: gs.outcome.clone(),
            }
        };
        let mut observed = observed;
        if let (Some(leader), Some(list)) = (&observed.your_leader, self.pasted_deck.read().as_ref()) {
            if list.leader_id.as_deref() == Some(leader.as_str()) {
                observed.your_deck = list.name.clone().unwrap_or_default();
            }
        }

        let step = self.match_tracker.lock().observe(observed);
        let mut changed = false;
        if let Some(record) = &step.finished {
            match self.match_store.append(record) {
                Ok(()) => changed = true,
                Err(e) => tracing::warn!(error = %e, "could not save the match"),
            }
        }
        if let Some((id, result)) = &step.result_update {
            changed |= self
                .match_store
                .set_result(id, Some(result), "page")
                .unwrap_or(false);
        }
        if step.new_game {
            // The last game's outcome must not leak into the next one.
            self.game_state.write().outcome = None;
        }
        changed
    }

    /// Where OPTCGSim keeps its decks and combat logs on this computer.
    fn optcgsim_dir() -> Option<PathBuf> {
        if let Ok(custom) = std::env::var("OPTCG_SIM_DIR") {
            return Some(PathBuf::from(custom));
        }
        dirs::data_dir().map(|d| d.join("com.Batsu.OPTCGSim"))
    }

    /// Add your OPTCGSim decks to your deck lists. Your active deck is left as it is.
    pub fn import_optcgsim_decks(&self) -> Result<crate::dto::ImportSummaryDto, String> {
        let dir = Self::optcgsim_dir()
            .filter(|d| d.is_dir())
            .ok_or("OPTCGSim's folder was not found on this computer.")?;
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .collect();
        files.sort();

        let mut out = crate::dto::ImportSummaryDto { found: files.len(), ..Default::default() };
        let previously_active = self.deck_collection.read().active_id.clone();
        for path in files {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").trim().to_string();
            // The simulator's built-in starter decks are not yours.
            let is_starter = name.len() > 7 && name.starts_with("ST") && name.get(4..7) == Some(" - ");
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            if name.is_empty() || is_starter || raw.trim().is_empty() {
                out.skipped += 1;
                continue;
            }
            let Ok(list) = self.parse_deck_raw(&raw) else {
                out.skipped += 1;
                continue;
            };
            if list.total_cards < 40 {
                out.skipped += 1; // an unfinished deck
                continue;
            }
            let existing = self
                .deck_collection
                .read()
                .decks
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(&name))
                .map(|d| d.raw.trim() == raw.trim());
            if existing == Some(true) {
                out.already_saved += 1;
                continue;
            }
            let result = self.deck_collection.write().upsert(
                None,
                &name,
                raw.trim(),
                list.leader_id.clone(),
                list.total_cards,
                &now_rfc3339(),
            );
            match result {
                Ok(_) => out.imported += 1,
                Err(_) => out.skipped += 1,
            }
        }
        {
            let mut collection = self.deck_collection.write();
            collection.active_id = previously_active;
            self.persist_collection(&collection);
        }
        out.message = format!(
            "Imported {} deck{} from OPTCGSim ({} already saved, {} skipped: starter decks, empty or unfinished lists).",
            out.imported,
            if out.imported == 1 { "" } else { "s" },
            out.already_saved,
            out.skipped
        );
        Ok(out)
    }

    /// Add finished OPTCGSim games to your match history from its saved logs.
    ///
    /// Only what both players could see is read; the opponent's hand and name
    /// in those logs are ignored.
    pub fn import_optcgsim_games(&self) -> Result<crate::dto::ImportSummaryDto, String> {
        let logs_dir = Self::optcgsim_dir()
            .map(|d| d.join("CombatLogs"))
            .filter(|d| d.is_dir())
            .ok_or("OPTCGSim's combat logs were not found on this computer.")?;
        // (normalised start time, text, modified time). A game saved both by
        // hand and automatically shares a start time; keep the longer copy.
        let mut games: std::collections::BTreeMap<String, (String, i64)> = std::collections::BTreeMap::new();
        for dir in [logs_dir.clone(), logs_dir.join("AutoSaved")] {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for path in entries.flatten().map(|e| e.path()).filter(|p| p.is_file()) {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").replace(':', ".");
                let Ok(bytes) = std::fs::read(&path) else { continue };
                let text = String::from_utf8_lossy(&bytes).to_string();
                let modified = path
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                match games.get(&stem) {
                    Some((old, _)) if old.len() >= text.len() => {}
                    _ => {
                        games.insert(stem, (text, modified));
                    }
                }
            }
        }

        let mut out = crate::dto::ImportSummaryDto { found: games.len(), ..Default::default() };
        let texts: Vec<&str> = games.values().map(|(t, _)| t.as_str()).collect();
        let Some(me) = optcg_rules::optcgsim_log::detect_player(&texts) else {
            out.message = "Could not tell which player is you (needs at least two saved games).".into();
            return Ok(out);
        };
        let have: std::collections::HashSet<String> =
            self.match_store.load().into_iter().map(|r| r.id).collect();
        let mut fresh = Vec::new();
        for (stem, (text, modified)) in &games {
            let id = format!("sim-{stem}");
            if have.contains(&id) {
                out.already_saved += 1;
                continue;
            }
            let Some(game) = optcg_rules::optcgsim_log::parse_combat_log(text, &me) else {
                out.skipped += 1;
                continue;
            };
            let started = stem_to_millis(stem).unwrap_or(*modified);
            fresh.push(optcg_rules::MatchRecord {
                id,
                started_ms: started,
                ended_ms: (*modified).max(started),
                your_leader: game.your_leader,
                opponent_leader: game.opponent_leader,
                went_first: game.went_first,
                turns: game.turns,
                result_source: if game.result.is_some() { "OPTCGSim log".into() } else { String::new() },
                result: game.result,
                your_deck: String::new(),
                opponent_cards_seen: game.opponent_cards_seen,
                your_cards_seen: game.your_cards_seen,
            });
        }
        fresh.sort_by_key(|r| r.started_ms);
        for record in &fresh {
            match self.match_store.append(record) {
                Ok(()) => out.imported += 1,
                Err(_) => out.skipped += 1,
            }
        }
        let unknown = fresh.iter().filter(|r| r.result.is_none()).count();
        out.message = format!(
            "Imported {} game{} from OPTCGSim ({} already saved, {} skipped). {} had no result in the log: set them in My games.",
            out.imported,
            if out.imported == 1 { "" } else { "s" },
            out.already_saved,
            out.skipped,
            unknown
        );
        Ok(out)
    }

    /// Recent matches, newest first, and totals with Leader names.
    pub fn match_history(&self, limit: usize) -> crate::dto::MatchHistoryDto {
        let mut records = self.match_store.load();
        let stats = optcg_rules::summarize(&records);
        records.reverse();
        records.truncate(limit);
        let repo = self.repo();
        let mut names = std::collections::BTreeMap::new();
        let ids = stats
            .by_your_leader
            .iter()
            .chain(stats.by_opponent_leader.iter())
            .map(|t| t.leader.clone())
            .chain(records.iter().flat_map(|r| [r.your_leader.clone(), r.opponent_leader.clone()]));
        for id in ids {
            if !names.contains_key(&id) {
                let name = repo.get_by_id(&id).map(|d| d.name).unwrap_or_else(|_| id.clone());
                names.insert(id, name);
            }
        }
        crate::dto::MatchHistoryDto { recent: records, stats, names }
    }

    pub fn set_match_result(&self, id: &str, result: Option<&str>) -> Result<(), String> {
        match self.match_store.set_result(id, result, "you") {
            Ok(true) => Ok(()),
            Ok(false) => Err("No such match".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete_match(&self, id: &str) -> Result<(), String> {
        match self.match_store.delete(id) {
            Ok(true) => Ok(()),
            Ok(false) => Err("No such match".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// (Re)read the downloaded tournament lists. Returns how many lists loaded.
    pub fn reload_meta(&self) -> usize {
        *self.banlist.write() = optcg_rules::BanList::load(&self.banlist_path);
        let loaded = optcg_rules::MetaData::load(&self.meta_path);
        let count = loaded.as_ref().map(|m| m.decks.len()).unwrap_or(0);
        *self.meta.write() = loaded;
        count
    }

    /// Leaders with enough winning lists to build a deck from.
    pub fn generator_leaders(&self, format: Option<&str>) -> Vec<(String, String, usize)> {
        let meta = self.meta.read();
        match meta.as_ref() {
            Some(meta) => optcg_rules::generatable_leaders(meta, &self.repo(), format),
            None => Vec::new(),
        }
    }

    /// A 50-card list built from the winning lists for one build of a Leader.
    pub fn generate_deck(
        &self,
        leader_id: &str,
        format: Option<&str>,
        variant: Option<usize>,
    ) -> Result<optcg_rules::GeneratedDeck, String> {
        let meta = self.meta.read();
        let Some(meta) = meta.as_ref() else {
            return Err("No winning-deck data yet. Run scripts/sync_meta.py, then tap Reload.".into());
        };
        let banlist = self.banlist.read();
        optcg_rules::generate(meta, &self.repo(), banlist.as_ref(), leader_id, format, variant)
    }

    /// One line on what tournament top-cut lists with this Leader play, for the
    /// coach. `None` without enough data (so the coach is never told something
    /// thin as if it were a trend).
    pub fn winners_summary(&self, leader_id: &str) -> Option<String> {
        if leader_id.is_empty() {
            return None;
        }
        let meta = self.meta.read();
        let meta = meta.as_ref()?;
        let banlist = self.banlist.read();
        let cmp = optcg_rules::meta::compare(
            meta,
            leader_id,
            &std::collections::HashMap::new(),
            &self.repo(),
            None,
            None,
            banlist.as_ref(),
        );
        if !cmp.enough_data || cmp.popular.is_empty() {
            return None;
        }
        let cards = cmp
            .popular
            .iter()
            .take(12)
            .map(|c| {
                format!(
                    "{} ({} of {} lists, usually {:.0}x)",
                    c.name, c.in_lists, cmp.lists, c.avg_copies
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "{} recent tournament top-cut lists ({}): {cards}",
            cmp.lists, cmp.format
        ))
    }

    /// How your active deck compares with what is winning, or why it cannot.
    pub fn compare_active_deck_to_meta(
        &self,
        format: Option<&str>,
        events: Option<&str>,
    ) -> Result<optcg_rules::DeckComparison, String> {
        let meta = self.meta.read();
        let Some(meta) = meta.as_ref() else {
            return Err(
                "No winning-deck data yet. Run scripts/sync_meta.py, then tap Reload.".into(),
            );
        };
        let pasted = self.pasted_deck.read().clone();
        let observed = self
            .game_state
            .read()
            .player_one()
            .observed_leader()
            .map(str::to_string)
            .unwrap_or_default();
        // The game's Leader wins over a saved list built around another one.
        let list = pasted.filter(|l| {
            observed.is_empty() || l.leader_id.as_deref() == Some(observed.as_str())
        });
        let leader = list
            .as_ref()
            .and_then(|l| l.leader_id.clone())
            .or_else(|| (!observed.is_empty()).then_some(observed));
        let Some(leader) = leader else {
            return Err("Waiting to see your Leader in a game, or add your deck list.".into());
        };
        let cards: std::collections::HashMap<String, u8> = list
            .map(|l| {
                l.entries
                    .iter()
                    .filter(|e| e.card_type != "leader" && !e.card_id.eq_ignore_ascii_case(&leader))
                    .map(|e| (e.card_id.to_uppercase(), e.quantity))
                    .collect()
            })
            .unwrap_or_default();
        let banlist = self.banlist.read();
        Ok(optcg_rules::meta::compare(
            meta,
            &leader,
            &cards,
            &self.repo(),
            format,
            events,
            banlist.as_ref(),
        ))
    }

    pub fn build_update_payload(
        &self,
        observation: Option<ObservationStatusDto>,
    ) -> StateUpdatePayload {
        let gs = self.game_state.read();
        let repo = self.repo();
        let combat_analysis = CombatMath::analyze_current_combat(&gs, &repo);
        let combat_coach = CombatMath::do_this(&gs, Some(&repo), combat_analysis.as_ref())
            .or_else(|| CombatMath::table_do_this(&gs, &repo));
        let your_deck = self.deck_info_for(gs.player_one(), Side::You);
        let opponent_deck = self.deck_info_for(gs.player_two(), Side::Opponent);
        let mut deck_strategy = self.ensure_deck_strategy(&your_deck, &opponent_deck, &gs);
        deck_strategy.standing_orders =
            self.standing_orders_for(&your_deck.leader_id, &opponent_deck.leader_id);
        if let Some(ref battle) = combat_coach {
            if !battle.steps.is_empty() {
                deck_strategy.this_turn = battle.steps.clone();
            }
        }
        let mut phase_coach = if let Some(ref battle) = combat_coach {
            battle.line.clone()
        } else {
            RulesEngine::phase_coach(&gs)
        };
        // Make coaching deck-specific when we know the matchup — not during a swing.
        if combat_coach.is_none() && (your_deck.leader_id != "" || opponent_deck.leader_id != "") {
            let you = if your_deck.name != "Deck unknown" {
                your_deck.name.as_str()
            } else {
                "your deck"
            };
            let opp = if opponent_deck.name != "Deck unknown" {
                opponent_deck.name.as_str()
            } else {
                "opponent"
            };
            phase_coach = format!("{phase_coach} ({you} vs {opp})");
        }

        let sync_confidence = observation
            .as_ref()
            .map(|o| match o.sync_state {
                crate::dto::SyncStateDto::Synced => 0.9,
                crate::dto::SyncStateDto::Partial => 0.7,
                crate::dto::SyncStateDto::Recovering => 0.55,
                crate::dto::SyncStateDto::Degraded => 0.4,
                crate::dto::SyncStateDto::Desynced => 0.2,
            })
            .unwrap_or(0.55);

        let source_connected = observation
            .as_ref()
            .map(|o| {
                !matches!(
                    o.hud_state,
                    crate::dto::HudOperatingState::Lost | crate::dto::HudOperatingState::Searching
                )
            })
            .unwrap_or(true);

        let eligibility = optcg_observation::AnalysisEligibility::evaluate(
            sync_confidence,
            true,
            gs.player_one().life > 0 || gs.player_two().life > 0,
            gs.combat.active,
            source_connected,
        );

        // Always outline options for the current step when we have any usable state.
        // Full "best move" confidence still respects eligibility.
        let mut options = RulesEngine::rank_actions(&gs, &repo).unwrap_or_default();
        if options.len() > 8 {
            options.truncate(8);
        }

        // Prefer beam-search top line when eligible — richer multi-step sequencing.
        let strategy = if eligibility.eligible
            || eligibility.mode != optcg_observation::AnalysisMode::Paused
        {
            if let Ok(beam) = self.beam.recommend(&gs, &repo) {
                if let Some(top) = beam.first() {
                    Some(optcg_rules::StrategyRecommendation {
                        action: optcg_rules::Action {
                            action_type: top.action.action_type.clone(),
                            actor: gs.active_player,
                            card_id: top.action.card_id.clone(),
                            target: top.action.target_id.clone(),
                            target_player: top.action.target_player,
                            cost: top.action.cost,
                            description: if top.sequence.len() > 1 {
                                format!("Line: {}", top.sequence.join(" → "))
                            } else {
                                top.action.description.clone()
                            },
                        },
                        score: top.score,
                        confidence: if eligibility.eligible { 0.75 } else { 0.45 },
                        reasoning: if top.sequence.len() > 1 {
                            format!(
                                "Suggested sequence for this phase: {}. {} | {}",
                                top.sequence.join(" → "),
                                phase_coach,
                                deck_strategy.vs_opponent
                            )
                        } else {
                            format!("{} | {}", phase_coach, deck_strategy.your_plan)
                        },
                    })
                } else {
                    options.first().cloned()
                }
            } else {
                options.first().cloned()
            }
        } else {
            None
        };

        let mut latency_ms = gs.connection.latency_ms;
        if let Some(ref obs) = observation {
            latency_ms = obs.latency.total_latency_ms;
        }

        let mut connection = ConnectionStatusDto::from_state(&gs);
        if let Some(ref obs) = observation {
            if let Some(ref src) = obs.active_source {
                connection = connection.with_source_label(Some(src));
            }
        }

        let pasted_deck = self.pasted_deck.read().as_ref().map(PastedDeckDto::from);
        let deck_collection = self.deck_collection_dto();

        // Only worth showing while their list is still a question. Once one is
        // attached, an estimate of a deck we hold in full is just noise.
        let scouting = (opponent_deck.origin != DeckOrigin::Attached)
            .then(|| self.scouting_report(&opponent_deck.leader_id))
            .flatten();

        // Shown whatever the deck sources say: knowing their list does not tell
        // you how your own deck has gone against it.
        let matchup = self.matchup_report(&your_deck.leader_id, &opponent_deck.leader_id);
        let review = self.latest_review();

        StateUpdatePayload {
            game_state: GameStateDto::from(&*gs),
            connection,
            combat_analysis,
            combat_coach,
            strategy,
            options,
            phase_coach,
            deck_strategy: Some(deck_strategy),
            your_deck,
            opponent_deck,
            pasted_deck,
            deck_collection,
            scouting,
            matchup,
            review,
            latency_ms,
            observation,
        }
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_database::AssetParser;
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    const LUFFY: &str = "Deck: Red Luffy Aggro\nLeader: ST01-001\n4x ST01-002\n4x ST01-003";
    const ZORO: &str = "Deck: Zoro Rush\nLeader: ST01-001\n4x ST01-003";

    /// Each test gets its own data dir so collections never cross-contaminate.
    fn temp_data_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("optcg-state-{tag}-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn app_state(data_dir: &Path) -> AppState {
        let database = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&database).unwrap();
        AppState::new(
            database,
            Arc::new(RwLock::new(optcg_core::GameState::new())),
            data_dir.to_path_buf(),
        )
    }

    /// Their leader is Luffy's other printing, so it can be told apart from the
    /// leader both fixture lists are built on.
    const ELBAPH: &str = "Deck: Black Elbaph Luffy\nLeader: OP17-079\n4x OP17-080\n4x OP17-081";

    /// An app state whose game state the test can drive, for the readings that
    /// depend on what is on the table.
    fn app_state_with_board(data_dir: &Path) -> (AppState, Arc<RwLock<optcg_core::GameState>>) {
        let database = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&database).unwrap();
        let board = Arc::new(RwLock::new(optcg_core::GameState::new()));
        let state = AppState::new(database, Arc::clone(&board), data_dir.to_path_buf());
        (state, board)
    }

    fn set_opponent_leader(board: &Arc<RwLock<optcg_core::GameState>>, card_id: &str) {
        board.write().player_two_mut().set_leader_id(card_id);
    }

    #[test]
    fn the_opponent_is_read_from_play_rather_than_given_your_list() {
        let dir = temp_data_dir("opponent-default");
        let (state, board) = app_state_with_board(&dir);
        set_opponent_leader(&board, "ST01-001");
        state.save_deck(Side::You, None, None, LUFFY).unwrap();

        let (_, opponent) = state.deck_infos();
        assert_eq!(
            opponent.origin,
            DeckOrigin::Observed,
            "saving your own deck must not put a list in the opponent's hands"
        );
        assert!(opponent.list_entries.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unread_table_does_not_invent_starter_luffy() {
        let dir = temp_data_dir("no-invent-luffy");
        let (state, _) = app_state_with_board(&dir);
        let (you, them) = state.deck_infos();
        assert!(
            you.leader_id.is_empty(),
            "placeholder ST01-001 is not a read: {}",
            you.leader_id
        );
        assert!(
            them.leader_id.is_empty(),
            "placeholder ST01-001 is not a read: {}",
            them.leader_id
        );
        assert_ne!(you.leader_name, "Monkey.D.Luffy");
        assert_ne!(them.leader_name, "Monkey.D.Luffy");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_attached_list_does_not_name_an_unread_leader() {
        let dir = temp_data_dir("no-paint-list");
        let (state, _) = app_state_with_board(&dir);
        state.save_deck(Side::You, None, None, LUFFY).unwrap();
        let dto = GameStateDto::from(&*state.game_state.read());
        assert!(
            !dto.player_one.leader_observed && dto.player_one.leader_id.is_empty(),
            "a saved list is not the card on the table: {:?}",
            dto.player_one.leader_id
        );
        assert!(
            !dto.player_two.leader_observed && dto.player_two.leader_id.is_empty(),
            "the far side stays unread: {:?}",
            dto.player_two.leader_id
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_list_can_be_attached_to_the_opponent() {
        let dir = temp_data_dir("opponent-attach");
        let (state, board) = app_state_with_board(&dir);
        set_opponent_leader(&board, "OP17-079");
        let theirs = state.save_deck(Side::You, None, None, ELBAPH).unwrap();

        state
            .set_deck_source(Side::Opponent, Some(&theirs.id))
            .unwrap();

        let (_, opponent) = state.deck_infos();
        assert_eq!(opponent.origin, DeckOrigin::Attached);
        assert_eq!(opponent.deck_id.as_deref(), Some(theirs.id.as_str()));
        assert!(
            opponent
                .list_entries
                .iter()
                .any(|e| e.card_id == "OP17-080"),
            "the attached list should reach the HUD: {:?}",
            opponent.list_entries
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn their_leader_brings_back_the_one_list_saved_for_it() {
        let dir = temp_data_dir("opponent-presumed");
        let (state, board) = app_state_with_board(&dir);
        // The point of the feature: their list was recorded in an earlier game,
        // and this game starts with the opponent read from play.
        state.save_deck(Side::Opponent, None, None, ELBAPH).unwrap();
        state.set_deck_source(Side::Opponent, None).unwrap();
        set_opponent_leader(&board, "OP17-079");

        let (_, opponent) = state.deck_infos();
        assert_eq!(
            opponent.origin,
            DeckOrigin::Presumed,
            "a leader we have exactly one list for should be recognised"
        );
        assert!(opponent
            .list_entries
            .iter()
            .any(|e| e.card_id == "OP17-081"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_presumed_list_does_not_become_a_card_we_claim_to_have_seen() {
        let dir = temp_data_dir("presumed-not-known");
        let (state, board) = app_state_with_board(&dir);
        state.save_deck(Side::Opponent, None, None, ELBAPH).unwrap();
        state.set_deck_source(Side::Opponent, None).unwrap();
        set_opponent_leader(&board, "OP17-079");

        let (_, opponent) = state.deck_infos();
        assert_eq!(opponent.origin, DeckOrigin::Presumed);
        assert!(
            opponent
                .known_cards
                .iter()
                .all(|card| card.card_id == "OP17-079"),
            "guessed list cards must stay out of what the table has revealed: {:?}",
            opponent.known_cards
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn two_lists_on_one_leader_leave_the_side_unread() {
        let dir = temp_data_dir("ambiguous-leader");
        let (state, board) = app_state_with_board(&dir);
        // Both fixtures are built on ST01-001, and neither side claims one.
        state.save_deck(Side::Opponent, None, None, LUFFY).unwrap();
        state.save_deck(Side::Opponent, None, None, ZORO).unwrap();
        state.set_deck_source(Side::Opponent, None).unwrap();
        set_opponent_leader(&board, "ST01-001");

        let (_, opponent) = state.deck_infos();
        assert_eq!(
            opponent.origin,
            DeckOrigin::Observed,
            "picking one of two lists would invent the opponent's deck"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_their_list_leaves_your_own_side_alone() {
        let dir = temp_data_dir("save-opponent");
        let (state, board) = app_state_with_board(&dir);
        board.write().player_one_mut().set_leader_id("ST01-001");
        set_opponent_leader(&board, "OP17-079");
        let yours = state.save_deck(Side::You, None, None, LUFFY).unwrap();

        state.save_deck(Side::Opponent, None, None, ELBAPH).unwrap();

        let (mine, theirs) = state.deck_infos();
        assert_eq!(
            mine.deck_id.as_deref(),
            Some(yours.id.as_str()),
            "recording their list must not change what you are playing"
        );
        assert_eq!(mine.origin, DeckOrigin::Attached);
        assert_eq!(theirs.origin, DeckOrigin::Attached);
        assert_eq!(theirs.name, "Black Elbaph Luffy");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn your_side_can_be_handed_back_to_being_read_from_play() {
        let dir = temp_data_dir("detach-you");
        let (state, board) = app_state_with_board(&dir);
        set_opponent_leader(&board, "OP17-079");
        state.save_deck(Side::You, None, None, ELBAPH).unwrap();
        board.write().player_one_mut().set_leader_id("ST01-001");

        state.set_deck_source(Side::You, None).unwrap();

        let (yours, _) = state.deck_infos();
        assert_eq!(yours.origin, DeckOrigin::Observed);
        assert!(
            state.pasted_deck.read().is_none(),
            "detaching your side should clear the active list the HUD reads"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attaching_an_unknown_list_is_an_error_not_a_silent_detach() {
        let dir = temp_data_dir("attach-unknown");
        let (state, board) = app_state_with_board(&dir);
        set_opponent_leader(&board, "OP17-079");
        let theirs = state.save_deck(Side::You, None, None, ELBAPH).unwrap();
        state
            .set_deck_source(Side::Opponent, Some(&theirs.id))
            .unwrap();

        assert!(state
            .set_deck_source(Side::Opponent, Some("no-such-deck"))
            .is_err());
        assert_eq!(state.deck_infos().1.origin, DeckOrigin::Attached);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_side_assignment_survives_a_restart() {
        let dir = temp_data_dir("assignment-persist");
        let theirs = {
            let (state, board) = app_state_with_board(&dir);
            set_opponent_leader(&board, "OP17-079");
            let theirs = state.save_deck(Side::You, None, None, ELBAPH).unwrap();
            state
                .set_deck_source(Side::Opponent, Some(&theirs.id))
                .unwrap();
            theirs.id
        };

        let (restarted, board) = app_state_with_board(&dir);
        set_opponent_leader(&board, "OP17-079");
        assert_eq!(
            restarted.deck_collection_dto().opponent_id.as_deref(),
            Some(theirs.as_str())
        );
        assert_eq!(restarted.deck_infos().1.origin, DeckOrigin::Attached);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Play out a game where the opponent shows `board` on `turn`.
    fn scout_game(
        state: &AppState,
        board: &Arc<RwLock<optcg_core::GameState>>,
        game: u128,
        leader: &str,
        cards: &[&str],
        turn: u32,
    ) {
        {
            let mut gs = board.write();
            gs.game_id = uuid::Uuid::from_u128(game);
            gs.turn_number = turn;
            gs.player_two_mut().set_leader_id(leader);
            gs.player_two_mut().characters = cards
                .iter()
                .map(|id| optcg_core::CardInstance::new(*id, 1, optcg_core::Zone::Character))
                .collect();
        }
        state.scout_position();
    }

    #[test]
    fn watching_a_game_builds_a_read_on_their_leader() {
        let dir = temp_data_dir("scout-basic");
        let (state, board) = app_state_with_board(&dir);

        scout_game(&state, &board, 1, "OP17-079", &["OP17-080"], 3);

        let report = state
            .scouting_report("OP17-079")
            .expect("the game in progress is already evidence");
        assert_eq!(report.games, 1);
        assert_eq!(report.reliability, "thin");
        assert!(report.cards.iter().any(|c| c.card_id == "OP17-080"));
        assert_eq!(
            report.cards[0].name, "Usopp",
            "the card DB should name what was seen"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_card_shown_every_game_climbs_to_full_confidence() {
        let dir = temp_data_dir("scout-confidence");
        let (state, board) = app_state_with_board(&dir);

        for game in 1..=4 {
            scout_game(&state, &board, game, "OP17-079", &["OP17-080"], 3);
        }

        let report = state.scouting_report("OP17-079").unwrap();
        let card = report
            .cards
            .iter()
            .find(|c| c.card_id == "OP17-080")
            .unwrap();
        assert_eq!(report.games, 4);
        assert!(
            (card.confidence - 1.0).abs() < 1e-6,
            "four of four games should read as certain as this gets: {}",
            card.confidence
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scouting_survives_a_restart() {
        let dir = temp_data_dir("scout-persist");
        {
            let (state, board) = app_state_with_board(&dir);
            scout_game(&state, &board, 1, "OP17-079", &["OP17-080"], 3);
            state.close_scouting_game();
        }

        let (restarted, _) = app_state_with_board(&dir);
        let report = restarted
            .scouting_report("OP17-079")
            .expect("the ledger should come back off disk");
        assert_eq!(report.games, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_idle_hud_records_no_games() {
        let dir = temp_data_dir("scout-idle");
        let (state, board) = app_state_with_board(&dir);
        board.write().player_two_mut().leader.card_id = "OP17-079".into();

        for _ in 0..5 {
            state.scout_position();
        }

        assert!(
            state.scouting_report("OP17-079").is_none(),
            "sitting on a default position is not a game played"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_finished_game_shows_up_in_the_matchup_payload() {
        let dir = temp_data_dir("matchup-win");
        let (state, board) = app_state_with_board(&dir);
        {
            let mut gs = board.write();
            gs.game_id = uuid::Uuid::from_u128(1);
            gs.turn_number = 6;
            gs.player_one_mut().set_leader_id("ST01-001");
            gs.player_two_mut().set_leader_id("OP17-079");
            gs.player_two_mut().characters = vec![optcg_core::CardInstance::new(
                "OP17-080",
                1,
                optcg_core::Zone::Character,
            )];
            gs.player_one_mut().life = 3;
            gs.player_two_mut().life = 5;
            gs.player_one_mut().don_active = 4;
        }
        state.scout_position();
        board.write().player_two_mut().life = 0;
        state.scout_position();
        state.close_scouting_game();

        let report = state
            .matchup_report("ST01-001", "OP17-079")
            .expect("a finished game belongs to a matchup");
        assert_eq!((report.wins, report.losses), (1, 0));
        assert!(
            report.orders.iter().any(|o| o.contains("DON")),
            "leftover DON should become a standing order: {:?}",
            report.orders
        );
        assert_eq!(report.standing, "too early to call");
        assert!(state.build_update_payload(None).matchup.is_some());
        let review = state
            .build_update_payload(None)
            .review
            .expect("a finished game has a recap");
        assert_eq!(review.outcome.as_deref(), Some("won"));
        assert!(
            review.headline.to_ascii_lowercase().contains("won on turn"),
            "{}",
            review.headline
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scouting_is_dropped_once_their_list_is_known() {
        let dir = temp_data_dir("scout-vs-attached");
        let (state, board) = app_state_with_board(&dir);
        scout_game(&state, &board, 1, "OP17-079", &["OP17-080"], 3);
        assert!(state.build_update_payload(None).scouting.is_some());

        let theirs = state.save_deck(Side::Opponent, None, None, ELBAPH).unwrap();
        state
            .set_deck_source(Side::Opponent, Some(&theirs.id))
            .unwrap();

        assert!(
            state.build_update_payload(None).scouting.is_none(),
            "an estimate of a deck we hold in full is noise"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scouting_reads_the_leader_actually_across_the_table() {
        let dir = temp_data_dir("scout-per-leader");
        let (state, board) = app_state_with_board(&dir);
        scout_game(&state, &board, 1, "OP17-079", &["OP17-080"], 3);
        scout_game(&state, &board, 2, "ST01-001", &["ST01-002"], 3);

        let report = state
            .scouting_report("ST01-001")
            .expect("the leader in front of us");
        assert!(report.cards.iter().all(|c| c.card_id != "OP17-080"));
        assert_eq!(
            state.scouting_report("OP17-079").unwrap().games,
            1,
            "the earlier opponent's history stays theirs"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    fn active_list_name(state: &AppState) -> Option<String> {
        state
            .pasted_deck
            .read()
            .as_ref()
            .and_then(|list| list.name.clone())
    }

    #[test]
    fn a_game_that_ends_is_recorded_once_and_the_result_can_be_set_later() {
        let dir = temp_data_dir("matches");
        let state = app_state(&dir);
        {
            let mut gs = state.game_state.write();
            gs.player_one_mut().set_leader_id("OP17-079");
            gs.player_two_mut().set_leader_id("OP09-062");
            gs.turn_number = 1;
        }
        assert!(!state.track_match(), "still playing");
        state.game_state.write().turn_number = 6;
        assert!(!state.track_match());
        state.game_state.write().outcome = Some("over".into());
        assert!(state.track_match(), "the end of the game was saved");
        assert!(!state.track_match(), "and not saved twice");

        let history = state.match_history(10);
        assert_eq!(history.recent.len(), 1);
        assert_eq!(history.recent[0].your_leader, "OP17-079");
        assert_eq!(history.recent[0].result, None);
        assert_eq!(history.stats.unknown, 1);

        let id = history.recent[0].id.clone();
        state.set_match_result(&id, Some("win")).unwrap();
        let history = state.match_history(10);
        assert_eq!(history.recent[0].result.as_deref(), Some("win"));
        assert_eq!(history.recent[0].result_source, "you");
        assert_eq!(history.stats.wins, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn optcgsim_decks_and_games_are_imported_without_touching_the_active_deck() {
        let dir = temp_data_dir("sim-import");
        let sim = dir.join("sim");
        std::fs::create_dir_all(sim.join("CombatLogs/AutoSaved")).unwrap();
        let deck = (1..=13).map(|i| format!("4xST01-{i:03}")).collect::<Vec<_>>().join("\n");
        std::fs::write(sim.join("MY DECK.txt"), format!("1xST01-001\n{deck}")).unwrap();
        std::fs::write(sim.join("ST01 - Strawhat.txt"), "1xST01-001").unwrap();
        std::fs::write(sim.join("EMPTY.txt"), "").unwrap();
        let log = |me: &str, foe: &str| {
            format!(
                "[{me}] Leader is A [OP01-001]\n[{foe}] Leader is B [OP02-001]\n[{me}] Chose to go First\n\
                 [{me}] End Turn\n[{foe}] End Turn\n[{me}] End Turn\n[{foe}] Concedes!\n"
            )
        };
        std::fs::write(sim.join("CombatLogs/2026-09-01T10:00:00.txt"), log("Me#1", "A#2")).unwrap();
        // The same game saved automatically: must not be counted twice.
        std::fs::write(sim.join("CombatLogs/AutoSaved/2026-09-01T10.00.00.log"), log("Me#1", "A#2")).unwrap();
        std::fs::write(sim.join("CombatLogs/AutoSaved/2026-09-02T10.00.00.log"), log("Me#1", "B#3")).unwrap();
        std::env::set_var("OPTCG_SIM_DIR", &sim);

        let state = app_state(&dir);
        state.save_deck(Side::You, None, Some("Mine"), &format!("Leader: ST01-001\n{deck}")).unwrap();
        let active_before = state.deck_collection_dto().active_id;

        let decks = state.import_optcgsim_decks().unwrap();
        assert_eq!(decks.imported, 1, "{}", decks.message);
        assert_eq!(state.deck_collection_dto().active_id, active_before, "active deck unchanged");
        assert_eq!(state.import_optcgsim_decks().unwrap().already_saved, 1, "second run adds nothing");

        let games = state.import_optcgsim_games().unwrap();
        assert_eq!(games.imported, 2, "the duplicate save collapses: {}", games.message);
        let history = state.match_history(10);
        assert_eq!(history.stats.wins, 2);
        assert_eq!(history.recent[0].went_first, Some(true));
        assert_eq!(state.import_optcgsim_games().unwrap().imported, 0, "no repeats");
        std::env::remove_var("OPTCG_SIM_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_a_deck_stores_and_activates_it() {
        let dir = temp_data_dir("save");
        let state = app_state(&dir);

        let saved = state.save_deck(Side::You, None, None, LUFFY).unwrap();
        assert_eq!(
            saved.name, "Red Luffy Aggro",
            "name comes from the Deck: line"
        );
        assert_eq!(saved.leader_id.as_deref(), Some("ST01-001"));
        assert_eq!(saved.total_cards, 8);

        let dto = state.deck_collection_dto();
        assert_eq!(dto.decks.len(), 1);
        assert_eq!(dto.active_id.as_deref(), Some(saved.id.as_str()));
        assert!(dto.decks[0].is_active);
        assert_eq!(
            dto.decks[0].leader_name.as_deref(),
            Some("Monkey.D.Luffy"),
            "leader should be resolved from the card database"
        );
        assert_eq!(active_list_name(&state).as_deref(), Some("Red Luffy Aggro"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deck_name_falls_back_to_leader_when_list_is_unnamed() {
        let dir = temp_data_dir("fallback");
        let state = app_state(&dir);

        let saved = state
            .save_deck(Side::You, None, None, "Leader: ST01-001\n4x ST01-002")
            .unwrap();
        assert_eq!(saved.name, "Red Monkey.D.Luffy");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn explicit_name_overrides_the_list_name() {
        let dir = temp_data_dir("explicit");
        let state = app_state(&dir);

        let saved = state
            .save_deck(Side::You, None, Some("Tournament build"), LUFFY)
            .unwrap();
        assert_eq!(saved.name, "Tournament build");
        assert_eq!(
            active_list_name(&state).as_deref(),
            Some("Tournament build"),
            "the collection name is what the HUD shows"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resaving_the_same_deck_updates_instead_of_duplicating() {
        let dir = temp_data_dir("resave");
        let state = app_state(&dir);

        let first = state.save_deck(Side::You, None, None, LUFFY).unwrap();
        let again = state
            .save_deck(Side::You, None, None, &format!("{LUFFY}\n2x ST01-005"))
            .unwrap();

        assert_eq!(first.id, again.id);
        assert_eq!(state.deck_collection_dto().decks.len(), 1);
        assert_eq!(again.total_cards, 10);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_rejected_paste_leaves_the_collection_untouched() {
        let dir = temp_data_dir("reject");
        let state = app_state(&dir);
        state.save_deck(Side::You, None, None, LUFFY).unwrap();

        assert!(state.save_deck(Side::You, None, None, "   ").is_err());
        assert!(state.save_deck(Side::You, None, None, "!!!! ???").is_err());
        assert_eq!(state.deck_collection_dto().decks.len(), 1);
        assert_eq!(active_list_name(&state).as_deref(), Some("Red Luffy Aggro"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn activating_switches_the_deck_behind_coaching() {
        let dir = temp_data_dir("activate");
        let state = app_state(&dir);

        let luffy = state.save_deck(Side::You, None, None, LUFFY).unwrap();
        let zoro = state.save_deck(Side::You, None, None, ZORO).unwrap();
        assert_eq!(active_list_name(&state).as_deref(), Some("Zoro Rush"));

        state.activate_deck(&luffy.id).unwrap();
        assert_eq!(active_list_name(&state).as_deref(), Some("Red Luffy Aggro"));
        assert_eq!(
            state.deck_collection_dto().active_id.as_deref(),
            Some(luffy.id.as_str())
        );

        assert!(state.activate_deck("does-not-exist").is_err());
        assert_eq!(
            state.deck_collection_dto().active_id.as_deref(),
            Some(luffy.id.as_str()),
            "a failed activation must not disturb the active deck"
        );
        assert!(!zoro.id.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renaming_the_active_deck_updates_the_hud_name() {
        let dir = temp_data_dir("rename");
        let state = app_state(&dir);
        let saved = state.save_deck(Side::You, None, None, LUFFY).unwrap();

        state.rename_deck(&saved.id, "Red Luffy v2").unwrap();
        assert_eq!(active_list_name(&state).as_deref(), Some("Red Luffy v2"));
        assert_eq!(
            state.build_update_payload(None).your_deck.name,
            "Red Luffy v2",
            "the rename should reach the deck identity shown in the HUD"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_the_active_deck_promotes_another() {
        let dir = temp_data_dir("delete-active");
        let state = app_state(&dir);
        state.save_deck(Side::You, None, None, LUFFY).unwrap();
        let zoro = state.save_deck(Side::You, None, None, ZORO).unwrap();

        state.delete_deck(&zoro.id).unwrap();
        let dto = state.deck_collection_dto();
        assert_eq!(dto.decks.len(), 1);
        assert_eq!(dto.active_id.as_deref(), Some("red-luffy-aggro"));
        assert_eq!(
            active_list_name(&state).as_deref(),
            Some("Red Luffy Aggro"),
            "coaching should follow the promoted deck"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_an_inactive_deck_keeps_the_active_one() {
        let dir = temp_data_dir("delete-inactive");
        let state = app_state(&dir);
        let luffy = state.save_deck(Side::You, None, None, LUFFY).unwrap();
        let zoro = state.save_deck(Side::You, None, None, ZORO).unwrap();

        state.delete_deck(&luffy.id).unwrap();
        assert_eq!(
            state.deck_collection_dto().active_id.as_deref(),
            Some(zoro.id.as_str())
        );
        assert_eq!(active_list_name(&state).as_deref(), Some("Zoro Rush"));

        assert!(
            state.delete_deck(&luffy.id).is_err(),
            "deleting twice errors"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_the_last_deck_clears_the_active_list() {
        let dir = temp_data_dir("delete-last");
        let state = app_state(&dir);
        let saved = state.save_deck(Side::You, None, None, LUFFY).unwrap();

        state.delete_deck(&saved.id).unwrap();
        assert!(state.deck_collection_dto().decks.is_empty());
        assert!(state.pasted_deck.read().is_none());
        assert_eq!(
            state.build_update_payload(None).your_deck.origin,
            DeckOrigin::Observed
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clearing_deselects_but_keeps_the_deck_saved() {
        let dir = temp_data_dir("clear");
        let state = app_state(&dir);
        state.save_deck(Side::You, None, None, LUFFY).unwrap();

        state.clear_active_deck();
        let dto = state.deck_collection_dto();
        assert_eq!(dto.decks.len(), 1, "clearing must not delete saved decks");
        assert_eq!(dto.active_id, None);
        assert!(!dto.decks[0].is_active);
        assert!(state.pasted_deck.read().is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collection_and_active_deck_survive_a_restart() {
        let dir = temp_data_dir("restart");
        {
            let state = app_state(&dir);
            state.save_deck(Side::You, None, None, LUFFY).unwrap();
            let zoro = state.save_deck(Side::You, None, None, ZORO).unwrap();
            state.activate_deck(&zoro.id).unwrap();
        }

        let restarted = app_state(&dir);
        let dto = restarted.deck_collection_dto();
        assert_eq!(dto.decks.len(), 2);
        assert_eq!(dto.active_id.as_deref(), Some("zoro-rush"));
        assert_eq!(active_list_name(&restarted).as_deref(), Some("Zoro Rush"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deselected_state_survives_a_restart() {
        let dir = temp_data_dir("restart-clear");
        {
            let state = app_state(&dir);
            state.save_deck(Side::You, None, None, LUFFY).unwrap();
            state.clear_active_deck();
        }

        let restarted = app_state(&dir);
        assert_eq!(restarted.deck_collection_dto().decks.len(), 1);
        assert_eq!(restarted.deck_collection_dto().active_id, None);
        assert!(restarted.pasted_deck.read().is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_pasted_deck_is_imported_once() {
        let dir = temp_data_dir("legacy");
        std::fs::write(dir.join("pasted_deck.txt"), LUFFY).unwrap();

        let state = app_state(&dir);
        let dto = state.deck_collection_dto();
        assert_eq!(dto.decks.len(), 1, "the old pasted deck should be adopted");
        assert_eq!(dto.decks[0].name, "Red Luffy Aggro");
        assert!(dto.decks[0].is_active);
        assert!(dir.join("deck_collection.json").exists());

        // Once a collection exists, the legacy file must not be re-imported
        // over the top of the user's own edits.
        state.clear_active_deck();
        let restarted = app_state(&dir);
        assert_eq!(restarted.deck_collection_dto().decks.len(), 1);
        assert_eq!(restarted.deck_collection_dto().active_id, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_legacy_file_is_ignored() {
        let dir = temp_data_dir("legacy-empty");
        std::fs::write(dir.join("pasted_deck.txt"), "\n  \n").unwrap();

        let state = app_state(&dir);
        assert!(state.deck_collection_dto().decks.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_pasted_deck_saves_into_the_collection() {
        let dir = temp_data_dir("compat");
        let state = app_state(&dir);

        let list = state.set_pasted_deck(LUFFY).unwrap();
        assert_eq!(list.total_cards, 8);
        assert_eq!(state.deck_collection_dto().decks.len(), 1);

        state.clear_pasted_deck();
        assert!(state.pasted_deck.read().is_none());
        assert_eq!(
            state.deck_collection_dto().decks.len(),
            1,
            "the legacy clear path should deselect, not delete"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn do_this_on_the_payload_follows_the_open_swing() {
        let dir = temp_data_dir("combat-do-this");
        let (state, board) = app_state_with_board(&dir);
        {
            let mut gs = board.write();
            let mut sanji =
                optcg_core::CardInstance::new("ST01-012", 1, optcg_core::Zone::Character);
            sanji.attached_don = 3;
            gs.players[1].characters.push(sanji);
            gs.players[0].hand_count = 0;
            gs.combat = optcg_core::CombatState {
                active: true,
                attacker_id: Some("ST01-012".into()),
                attacker_player: Some(1),
                target_id: Some("leader".into()),
                target_player: Some(0),
                target_is_leader: true,
                ..optcg_core::CombatState::default()
            };
        }
        let payload = state.build_update_payload(None);
        let battle = payload
            .combat_coach
            .expect("an open swing should produce a battle line");
        assert!(
            battle.line.to_lowercase().contains("lethal"),
            "expected lethal instruction, got {}",
            battle.line
        );
        assert!(
            battle.line.contains("Sanji") || battle.line.contains("ST01-012"),
            "expected the attacker named, got {}",
            battle.line
        );
        assert_eq!(payload.phase_coach, battle.line);
        assert!(!battle.steps.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn do_this_names_the_cards_on_the_table_between_swings() {
        let dir = temp_data_dir("table-do-this");
        let (state, board) = app_state_with_board(&dir);
        {
            let mut gs = board.write();
            gs.phase = optcg_core::Phase::Main;
            gs.players[0].characters.push(optcg_core::CardInstance::new(
                "ST01-012",
                0,
                optcg_core::Zone::Character,
            ));
            gs.players[1].characters.push(optcg_core::CardInstance::new(
                "ST01-002",
                1,
                optcg_core::Zone::Character,
            ));
        }
        let payload = state.build_update_payload(None);
        let battle = payload
            .combat_coach
            .expect("a readable board should name the cards");
        let blob = format!(
            "{} {} {} {}",
            battle.line,
            battle.steps.join(" "),
            battle.you.join(" "),
            battle.them.join(" ")
        );
        assert!(
            blob.contains("Sanji") || blob.contains("ST01-012"),
            "{blob}"
        );
        assert!(
            blob.contains("Usopp") || blob.contains("ST01-002"),
            "{blob}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// "2026-08-26T01.36.33" (a log's file name) as milliseconds, read as local-clock digits in UTC.
fn stem_to_millis(stem: &str) -> Option<i64> {
    let (date, time) = stem.split_once('T')?;
    let day = optcg_rules::variants::day_number(date)?;
    let mut parts = time.split('.');
    let h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let s: i64 = parts.next().unwrap_or("0").parse().ok()?;
    Some(day * 86_400_000 + (h * 3600 + m * 60 + s) * 1000)
}
