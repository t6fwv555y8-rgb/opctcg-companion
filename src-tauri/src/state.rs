use crate::dto::ObservationStatusDto;
use crate::dto::{
    ConnectionStatusDto, DeckCollectionDto, DeckInfoDto, GameStateDto, KnownCardDto,
    OverlaySettings, PastedDeckDto, SavedDeckDto, StateUpdatePayload,
};
use optcg_database::Database;
use optcg_rules::{
    BeamSearch, BeamSearchConfig, CombatMath, DeckCollection, DeckProfile, DeckStrategyBrief,
    DeckStrategyCoach, MctsConfig, MctsEngine, PastedDeckList, RulesEngine, SavedDeck, MAX_DECKS,
};
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;

/// What the Game plan is computed from, cached per matchup.
#[derive(Clone)]
struct StrategyData {
    key: String,
    you: Option<optcg_rules::LeaderStats>,
    opponent: Option<optcg_rules::LeaderStats>,
    comparison: Option<optcg_rules::DeckComparison>,
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
    pasted_deck_path: PathBuf,
    collection_path: PathBuf,
    /// The leader the saved-deck match was last run for, so the library is
    /// only searched when the game shows a different leader.
    last_matched_leader: Mutex<String>,
    /// Tournament top-cut lists from `scripts/sync_meta.py`, when downloaded.
    meta: RwLock<Option<optcg_rules::MetaData>>,
    meta_path: PathBuf,
    /// Standard-format ban list, pair bans and rotation (`banlist.json`).
    banlist: RwLock<Option<optcg_rules::BanList>>,
    banlist_path: PathBuf,
    /// Watches the observed game and says when a match ended.
    match_tracker: Mutex<optcg_rules::MatchTracker>,
    /// Winning-list statistics for the current matchup, rebuilt only when the
    /// Leaders, your list, or the downloaded data change.
    strategy_cache: Mutex<Option<StrategyData>>,
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
            pasted_deck_path,
            collection_path,
            last_matched_leader: Mutex::new(String::new()),
            meta: RwLock::new(None),
            meta_path: data_dir.join("meta_decks.json"),
            banlist: RwLock::new(None),
            banlist_path: data_dir.join("banlist.json"),
            match_tracker: Mutex::new(optcg_rules::MatchTracker::default()),
            strategy_cache: Mutex::new(None),
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
        let yours = self.deck_info_for(gs.player_one(), true);
        let opponent = self.deck_info_for(gs.player_two(), false);
        (yours, opponent)
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
            max_decks: MAX_DECKS,
        }
    }

    /// Save a deck list into the collection and make it active.
    ///
    /// `id` targets an existing deck; without it the deck is matched by name so
    /// re-pasting the same list updates it rather than creating a duplicate.
    pub fn save_deck(
        &self,
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
            let deck_id = collection.upsert(
                id,
                &display_name,
                raw.trim(),
                list.leader_id.clone(),
                list.total_cards,
                &now_rfc3339(),
            )?;
            let saved = collection
                .get(&deck_id)
                .cloned()
                .ok_or_else(|| "Saved deck vanished after write".to_string())?;
            self.persist_collection(&collection);
            saved
        };

        self.activate_locally(&saved);
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
        self.save_deck(None, None, raw)?;
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

    /// Winning-list statistics for both Leaders and your list's comparison.
    fn strategy_data(&self, you: &DeckInfoDto, opponent: &DeckInfoDto) -> StrategyData {
        let meta = self.meta.read();
        let banlist = self.banlist.read();
        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            you.leader_id,
            opponent.leader_id,
            you.name,
            you.list_total_cards,
            meta.as_ref().map(|m| m.generated_at.as_str()).unwrap_or(""),
            banlist.as_ref().map(|b| b.last_updated.as_str()).unwrap_or("")
        );
        if let Some(cached) = self.strategy_cache.lock().as_ref() {
            if cached.key == key {
                return cached.clone();
            }
        }
        let repo = self.repo();
        let stats = |leader: &str| {
            let meta = meta.as_ref()?;
            (!leader.is_empty()).then(|| {
                optcg_rules::leader_stats(meta, &repo, banlist.as_ref(), leader, None)
            })
        };
        let comparison = meta.as_ref().and_then(|meta| {
            if you.leader_id.is_empty() {
                return None;
            }
            let cards: std::collections::HashMap<String, u8> = you
                .list_entries
                .iter()
                .filter(|e| e.card_type != "leader" && !e.card_id.eq_ignore_ascii_case(&you.leader_id))
                .map(|e| (e.card_id.to_uppercase(), e.quantity))
                .collect();
            Some(optcg_rules::meta::compare(
                meta,
                &you.leader_id,
                &cards,
                &repo,
                None,
                None,
                banlist.as_ref(),
            ))
        });
        let data = StrategyData {
            key,
            you: stats(&you.leader_id),
            opponent: stats(&opponent.leader_id),
            comparison,
        };
        *self.strategy_cache.lock() = Some(data.clone());
        data
    }

    fn build_brief(
        &self,
        gs: &optcg_core::GameState,
        your_deck: &DeckInfoDto,
        opponent_deck: &DeckInfoDto,
    ) -> DeckStrategyBrief {
        let data = self.strategy_data(your_deck, opponent_deck);
        let repo = self.repo();
        let ctx = optcg_rules::StrategyContext {
            repo: &repo,
            you: data.you.as_ref(),
            opponent: data.opponent.as_ref(),
            comparison: data.comparison.as_ref(),
        };
        DeckStrategyCoach::brief(
            gs,
            &self.profile_from_deck(your_deck),
            &self.profile_from_deck(opponent_deck),
            &ctx,
        )
    }

    /// Rebuild the Game plan for the current matchup.
    pub fn refresh_deck_strategy(&self) -> DeckStrategyBrief {
        let gs = self.game_state.read();
        let your_deck = self.deck_info_for(gs.player_one(), true);
        let opponent_deck = self.deck_info_for(gs.player_two(), false);
        let brief = self.build_brief(&gs, &your_deck, &opponent_deck);
        *self.deck_strategy.write() = Some(brief.clone());
        brief
    }

    /// The Game plan for right now. It includes what is in your hand and on
    /// the board, so it is rebuilt each time; the heavy statistics behind it
    /// are cached per matchup.
    fn ensure_deck_strategy(
        &self,
        your_deck: &DeckInfoDto,
        opponent_deck: &DeckInfoDto,
        gs: &optcg_core::GameState,
    ) -> DeckStrategyBrief {
        let brief = self.build_brief(gs, your_deck, opponent_deck);
        *self.deck_strategy.write() = Some(brief.clone());
        brief
    }

    fn deck_info_for(&self, player: &optcg_core::PlayerState, is_you: bool) -> DeckInfoDto {
        let repo = self.repo();
        let mut leader_id = player.observed_leader().unwrap_or_default().to_string();
        let pasted = if is_you {
            // A saved list built around a different Leader is not the deck
            // being played; showing it would coach the wrong deck.
            self.pasted_deck.read().clone().filter(|list| {
                match (list.leader_id.as_deref(), leader_id.as_str()) {
                    (Some(listed), observed) if !observed.is_empty() => listed == observed,
                    _ => true,
                }
            })
        } else {
            None
        };

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

        let (
            from_paste,
            list_entries,
            list_total_cards,
            list_warnings,
            paste_name,
        ) = if let Some(ref p) = pasted {
            // Merge paste into known for display
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
            (
                true,
                p.entries.clone(),
                p.total_cards,
                p.warnings.clone(),
                p.name.clone(),
            )
        } else {
            (false, Vec::new(), 0, Vec::new(), None)
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
            from_paste,
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
        *self.strategy_cache.lock() = None;
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

    /// Follow the deck the game is actually playing.
    ///
    /// The saved deck that is "active" is only a guess made before the match.
    /// Once the game shows your Leader, switch to the saved deck built around
    /// that Leader; when none matches, `deck_info_for` stops presenting the
    /// active list as yours. Runs at most once per observed Leader, so a deck
    /// you pick by hand afterwards is not overridden.
    pub fn follow_game_leader(&self) {
        let observed = self
            .game_state
            .read()
            .player_one()
            .observed_leader()
            .map(str::to_string)
            .unwrap_or_default();
        if observed.is_empty() {
            return;
        }
        {
            let mut last = self.last_matched_leader.lock();
            if *last == observed {
                return;
            }
            *last = observed.clone();
        }
        let already = self
            .pasted_deck
            .read()
            .as_ref()
            .and_then(|list| list.leader_id.clone());
        if already.as_deref() == Some(observed.as_str()) {
            return;
        }
        let candidate = {
            let repo = self.repo();
            let collection = self.deck_collection.read();
            collection
                .decks
                .iter()
                .find(|deck| {
                    // Older saves predate the full card list and may have no
                    // leader recorded; read it from the list itself.
                    let leader = deck
                        .leader_id
                        .clone()
                        .or_else(|| PastedDeckList::parse(&deck.raw, &repo).leader_id);
                    leader.as_deref() == Some(observed.as_str())
                })
                .map(|deck| deck.id.clone())
        };
        if let Some(id) = candidate {
            tracing::info!(leader = %observed, deck = %id, "switching to the saved deck for this Leader");
            let _ = self.activate_deck(&id);
        }
    }

    pub fn build_update_payload(
        &self,
        observation: Option<ObservationStatusDto>,
    ) -> StateUpdatePayload {
        self.follow_game_leader();
        let gs = self.game_state.read();
        let repo = self.repo();
        let combat_analysis = CombatMath::analyze_current_combat(&gs, &repo);
        let your_deck = self.deck_info_for(gs.player_one(), true);
        let opponent_deck = self.deck_info_for(gs.player_two(), false);
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
        let strategy = if eligibility.eligible || eligibility.mode != optcg_observation::AnalysisMode::Paused
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

        let pasted_deck = self
            .pasted_deck
            .read()
            .as_ref()
            .map(PastedDeckDto::from);
        let deck_collection = self.deck_collection_dto();

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
        let dir = std::env::temp_dir().join(format!(
            "optcg-state-{tag}-{}-{n}",
            std::process::id()
        ));
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

    fn active_list_name(state: &AppState) -> Option<String> {
        state
            .pasted_deck
            .read()
            .as_ref()
            .and_then(|list| list.name.clone())
    }

    const SUPER: &str = "Deck: Super Luffy\nLeader: OP17-079\n4x ST01-003";

    fn show_leader(state: &AppState, leader_id: &str) {
        state.game_state.write().player_one_mut().set_leader_id(leader_id);
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
        state.save_deck(None, Some("Mine"), &format!("Leader: ST01-001\n{deck}")).unwrap();
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
    fn the_deck_follows_the_leader_the_game_shows() {
        let dir = temp_data_dir("follow");
        let state = app_state(&dir);
        state.save_deck(None, None, LUFFY).unwrap();
        state.save_deck(None, None, SUPER).unwrap(); // most recent is active
        assert_eq!(active_list_name(&state).as_deref(), Some("Super Luffy"));

        show_leader(&state, "ST01-001");
        state.follow_game_leader();
        assert_eq!(active_list_name(&state).as_deref(), Some("Red Luffy Aggro"));

        let (yours, _) = state.deck_infos();
        assert_eq!(yours.name, "Red Luffy Aggro");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_saved_list_for_another_leader_is_not_presented_as_yours() {
        let dir = temp_data_dir("mismatch");
        let state = app_state(&dir);
        state.save_deck(None, None, SUPER).unwrap();

        // The game shows a Leader none of the saved decks were built around.
        show_leader(&state, "ST01-001");
        state.follow_game_leader();
        let (yours, _) = state.deck_infos();
        assert!(!yours.from_paste, "the mismatched list must not be shown");
        assert!(yours.list_entries.is_empty());
        assert_ne!(yours.name, "Super Luffy");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_a_deck_stores_and_activates_it() {
        let dir = temp_data_dir("save");
        let state = app_state(&dir);

        let saved = state.save_deck(None, None, LUFFY).unwrap();
        assert_eq!(saved.name, "Red Luffy Aggro", "name comes from the Deck: line");
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
            .save_deck(None, None, "Leader: ST01-001\n4x ST01-002")
            .unwrap();
        assert_eq!(saved.name, "Red Monkey.D.Luffy");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn explicit_name_overrides_the_list_name() {
        let dir = temp_data_dir("explicit");
        let state = app_state(&dir);

        let saved = state.save_deck(None, Some("Tournament build"), LUFFY).unwrap();
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

        let first = state.save_deck(None, None, LUFFY).unwrap();
        let again = state
            .save_deck(None, None, &format!("{LUFFY}\n2x ST01-005"))
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
        state.save_deck(None, None, LUFFY).unwrap();

        assert!(state.save_deck(None, None, "   ").is_err());
        assert!(state.save_deck(None, None, "!!!! ???").is_err());
        assert_eq!(state.deck_collection_dto().decks.len(), 1);
        assert_eq!(active_list_name(&state).as_deref(), Some("Red Luffy Aggro"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn activating_switches_the_deck_behind_coaching() {
        let dir = temp_data_dir("activate");
        let state = app_state(&dir);

        let luffy = state.save_deck(None, None, LUFFY).unwrap();
        let zoro = state.save_deck(None, None, ZORO).unwrap();
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
        let saved = state.save_deck(None, None, LUFFY).unwrap();

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
        state.save_deck(None, None, LUFFY).unwrap();
        let zoro = state.save_deck(None, None, ZORO).unwrap();

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
        let luffy = state.save_deck(None, None, LUFFY).unwrap();
        let zoro = state.save_deck(None, None, ZORO).unwrap();

        state.delete_deck(&luffy.id).unwrap();
        assert_eq!(
            state.deck_collection_dto().active_id.as_deref(),
            Some(zoro.id.as_str())
        );
        assert_eq!(active_list_name(&state).as_deref(), Some("Zoro Rush"));

        assert!(state.delete_deck(&luffy.id).is_err(), "deleting twice errors");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_the_last_deck_clears_the_active_list() {
        let dir = temp_data_dir("delete-last");
        let state = app_state(&dir);
        let saved = state.save_deck(None, None, LUFFY).unwrap();

        state.delete_deck(&saved.id).unwrap();
        assert!(state.deck_collection_dto().decks.is_empty());
        assert!(state.pasted_deck.read().is_none());
        assert!(!state.build_update_payload(None).your_deck.from_paste);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clearing_deselects_but_keeps_the_deck_saved() {
        let dir = temp_data_dir("clear");
        let state = app_state(&dir);
        state.save_deck(None, None, LUFFY).unwrap();

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
            state.save_deck(None, None, LUFFY).unwrap();
            let zoro = state.save_deck(None, None, ZORO).unwrap();
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
            state.save_deck(None, None, LUFFY).unwrap();
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
