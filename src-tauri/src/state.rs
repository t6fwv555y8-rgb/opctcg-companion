use crate::dto::ObservationStatusDto;
use crate::dto::{
    ConnectionStatusDto, DeckCollectionDto, DeckInfoDto, DeckOrigin, GameStateDto, KnownCardDto,
    MatchupReportDto, OverlaySettings, PastedDeckDto, SavedDeckDto, ScoutedCardDto,
    ScoutingReportDto, StateUpdatePayload,
};
use optcg_database::Database;
use optcg_rules::{
    BeamSearch, BeamSearchConfig, CombatMath, DeckCollection, DeckProfile, DeckStrategyBrief,
    DeckStrategyCoach, MctsConfig, MctsEngine, PastedDeckList, RulesEngine, SavedDeck, Side,
    MAX_DECKS,
};
use optcg_scouting::{DeckMap, MatchupRead, Scout, ScoutingLedger, StrategyRead};
use parking_lot::RwLock;
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
        };
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
    fn resolve_deck(&self, side: Side, leader_id: &str) -> ResolvedDeck {
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
        let mut leader_id = player.leader.card_id.clone();
        let ResolvedDeck {
            list: pasted,
            origin,
            deck_id,
        } = self.resolve_deck(side, &leader_id);

        if leader_id.is_empty() {
            if let Some(ref p) = pasted {
                if let Some(ref lid) = p.leader_id {
                    leader_id = lid.clone();
                }
            }
        }

        let (leader_name, leader_color) = match repo.get_by_id(&leader_id) {
            Ok(def) => (def.name, def.color),
            Err(_) => {
                if leader_id.is_empty() {
                    ("Unknown leader".into(), String::new())
                } else {
                    (leader_id.clone(), String::new())
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

        let name = if let Some(n) = paste_name {
            if !n.trim().is_empty() {
                n
            } else if !player.deck_name.trim().is_empty() {
                player.deck_name.clone()
            } else if !leader_color.is_empty() && leader_name != "Unknown leader" {
                format!("{leader_color} {leader_name}")
            } else {
                "Pasted deck".into()
            }
        } else if !player.deck_name.trim().is_empty() {
            player.deck_name.clone()
        } else if !leader_color.is_empty() && leader_name != "Unknown leader" {
            format!("{leader_color} {leader_name}")
        } else if !leader_id.is_empty() {
            format!("Deck · {leader_id}")
        } else {
            "Deck unknown".into()
        };

        DeckInfoDto {
            name,
            leader_id,
            leader_name,
            leader_color,
            known_cards,
            origin,
            deck_id,
            list_entries,
            list_total_cards,
            list_warnings,
        }
    }

    pub fn build_update_payload(
        &self,
        observation: Option<ObservationStatusDto>,
    ) -> StateUpdatePayload {
        let gs = self.game_state.read();
        let repo = self.repo();
        let combat_analysis = CombatMath::analyze_current_combat(&gs, &repo);
        let your_deck = self.deck_info_for(gs.player_one(), Side::You);
        let opponent_deck = self.deck_info_for(gs.player_two(), Side::Opponent);
        let deck_strategy = self.ensure_deck_strategy(&your_deck, &opponent_deck, &gs);
        let mut phase_coach = RulesEngine::phase_coach(&gs);
        // Make coaching deck-specific when we know the matchup.
        if your_deck.leader_id != "" || opponent_deck.leader_id != "" {
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

        StateUpdatePayload {
            game_state: GameStateDto::from(&*gs),
            connection,
            combat_analysis,
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
        board.write().player_two_mut().leader.card_id = card_id.to_string();
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
            opponent.known_cards.is_empty(),
            "guessed cards must stay out of what the table has revealed: {:?}",
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
        board.write().player_one_mut().leader.card_id = "ST01-001".into();
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
        board.write().player_one_mut().leader.card_id = "ST01-001".into();

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
            gs.player_two_mut().leader.card_id = leader.to_string();
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
            gs.player_one_mut().leader.card_id = "ST01-001".into();
            gs.player_two_mut().leader.card_id = "OP17-079".into();
            gs.player_two_mut().characters = vec![optcg_core::CardInstance::new(
                "OP17-080",
                1,
                optcg_core::Zone::Character,
            )];
            gs.player_one_mut().life = 3;
            gs.player_two_mut().life = 5;
        }
        state.scout_position();
        board.write().player_two_mut().life = 0;
        state.scout_position();
        state.close_scouting_game();

        let report = state
            .matchup_report("ST01-001", "OP17-079")
            .expect("a finished game belongs to a matchup");
        assert_eq!((report.wins, report.losses), (1, 0));
        assert_eq!(report.standing, "too early to call");
        assert!(state.build_update_payload(None).matchup.is_some());

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
}
