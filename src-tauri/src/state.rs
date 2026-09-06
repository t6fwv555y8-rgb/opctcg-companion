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
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

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

    /// Rebuild detailed deck-vs-deck strategy for the current matchup.
    pub fn refresh_deck_strategy(&self) -> DeckStrategyBrief {
        let gs = self.game_state.read();
        let your_deck = self.deck_info_for(gs.player_one(), true);
        let opponent_deck = self.deck_info_for(gs.player_two(), false);
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

    fn deck_info_for(&self, player: &optcg_core::PlayerState, is_you: bool) -> DeckInfoDto {
        let repo = self.repo();
        let mut leader_id = player.leader.card_id.clone();
        let pasted = if is_you {
            self.pasted_deck.read().clone()
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

    pub fn build_update_payload(
        &self,
        observation: Option<ObservationStatusDto>,
    ) -> StateUpdatePayload {
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
