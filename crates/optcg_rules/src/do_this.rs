use crate::combat_math::{CombatAnalysis, CombatDoThis};
use optcg_core::{CardInstance, GameState, Phase, PlayerState};
use optcg_database::CardRepository;

/// What to do right now, named off the cards actually on the table.
pub fn battle_do_this(
    state: &GameState,
    repo: Option<&CardRepository<'_>>,
    analysis: Option<&CombatAnalysis>,
) -> Option<CombatDoThis> {
    if !state.combat.active && analysis.is_none() {
        return None;
    }

    let table = Table::read(state, repo);
    let defending = table.you_defending(analysis);
    let swing = table.swing_clause();

    if let Some(a) = analysis {
        let need = fmt_power(a.required_counter);
        if defending {
            if a.lethal_to_leader {
                let mut steps = table.your_blocker_steps();
                steps.extend(table.your_counter_steps(a.required_counter));
                steps.push(format!(
                    "You are at {} life. If this hits, you lose.",
                    table.you.life
                ));
                let line = match table.your_blockers().first() {
                    Some(blocker) => {
                        format!("{swing} Block with {blocker} or counter {need} — this is lethal.")
                    }
                    None => format!("{swing} Counter {need} or you lose — this is lethal."),
                };
                return Some(table.plan(line, steps));
            }
            if a.recommended_block || (state.combat.blocker_offered && !a.survives_without_counter)
            {
                let blocker = table
                    .your_blockers()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "a ready Blocker".into());
                let mut steps = vec![format!("Block with {blocker}.")];
                if a.required_counter > 0 {
                    steps.extend(table.your_counter_steps(a.required_counter));
                }
                return Some(table.plan(format!("{swing} Block with {blocker}."), steps));
            }
            if a.required_counter > 0 && !a.survives_without_counter {
                let mut steps = table.your_counter_steps(a.required_counter);
                steps.push("If you keep the cards, take the hit.".into());
                return Some(table.plan(format!("{swing} Counter {need} or take the hit."), steps));
            }
            if a.survives_without_counter {
                return Some(table.plan(
                    format!("{swing} They don't break through — take it."),
                    vec!["Don't spend a Blocker or counter here.".into()],
                ));
            }
        } else if a.lethal_to_leader {
            return Some(table.plan(
                format!("{swing} This is lethal — go through."),
                table.their_blocker_watch(),
            ));
        } else if a.required_counter > 0 {
            return Some(table.plan(
                format!("{swing} They need {need} to live."),
                table.their_blocker_watch(),
            ));
        } else {
            return Some(table.plan(
                format!("{swing} They don't break this — resolve."),
                vec!["Resolve this swing.".into()],
            ));
        }
    }

    if state.combat.blocker_offered {
        return Some(table.plan(
            format!("{swing} Blocker window — decide now."),
            table.your_blocker_steps(),
        ));
    }
    if defending {
        return Some(table.plan(
            format!("{swing} Block, counter, or take it."),
            table.your_blocker_steps(),
        ));
    }
    Some(table.plan(
        format!("{swing} Resolve this swing."),
        table.their_blocker_watch(),
    ))
}

/// When no attack is open: name the bodies on the table and the next swing.
pub fn table_do_this(state: &GameState, repo: &CardRepository<'_>) -> Option<CombatDoThis> {
    if state.combat.active {
        return None;
    }
    let you = &state.players[0];
    let them = &state.players[1];
    if you.leader.card_id.is_empty() && you.characters.is_empty() && them.characters.is_empty() {
        return None;
    }

    let table = Table::read(state, Some(repo));
    if table.you_roster().is_empty() && table.them_roster().is_empty() {
        return None;
    }

    let line = if state.active_player == 0 {
        table.your_next_swing().unwrap_or_else(|| {
            format!(
                "Your turn · {} life to their {} · {} active DON. {}",
                you.life,
                them.life,
                you.don_active,
                table.your_board_summary()
            )
        })
    } else {
        format!(
            "Their turn · you {} life, they {} · {}. Keep answers ready.",
            you.life,
            them.life,
            table.their_board_summary()
        )
    };

    let mut steps = Vec::new();
    if you.don_active > 0 && matches!(state.phase, Phase::Don | Phase::Main) {
        steps.push(table.don_step());
    }

    Some(table.plan(line, steps))
}

struct SideView {
    life: u32,
    don_active: u32,
    don_rested: u32,
    hand_count: u32,
    leader_id: String,
    leader_name: String,
    leader_text: String,
    leader_power: i32,
}

struct Table<'a> {
    state: &'a GameState,
    repo: Option<&'a CardRepository<'a>>,
    you: SideView,
    them: SideView,
}

impl<'a> Table<'a> {
    fn read(state: &'a GameState, repo: Option<&'a CardRepository<'a>>) -> Self {
        Self {
            state,
            repo,
            you: side_view(&state.players[0], repo),
            them: side_view(&state.players[1], repo),
        }
    }

    fn you_defending(&self, analysis: Option<&CombatAnalysis>) -> bool {
        if self.state.combat.target_player == Some(0) {
            return true;
        }
        if self.state.combat.target_player == Some(1) {
            return false;
        }
        if self.state.combat.attacker_player == Some(1) {
            return true;
        }
        if self.state.combat.attacker_player == Some(0) {
            return false;
        }
        analysis
            .is_some_and(|a| a.lethal_to_leader || a.recommended_block || a.required_counter > 0)
            && self.state.combat.target_is_leader
    }

    fn attacker_idx(&self) -> usize {
        self.state
            .combat
            .attacker_player
            .map(|p| p as usize)
            .unwrap_or(self.state.active_player as usize)
    }

    fn target_idx(&self) -> usize {
        self.state
            .combat
            .target_player
            .map(|p| p as usize)
            .unwrap_or(1 - self.attacker_idx())
    }

    fn swing_clause(&self) -> String {
        let attacker = self.named_combatant(
            self.attacker_idx(),
            self.state.combat.attacker_id.as_deref(),
            false,
        );
        let target = self.named_combatant(
            self.target_idx(),
            self.state.combat.target_id.as_deref(),
            self.state.combat.target_is_leader,
        );
        let whose = if self.attacker_idx() == 0 {
            "Your"
        } else {
            "Their"
        };
        format!("{whose} {attacker} is swinging at {target}.")
    }

    fn named_combatant(&self, player: usize, card_id: Option<&str>, force_leader: bool) -> String {
        let side = if player == 0 { &self.you } else { &self.them };
        let poss = if player == 0 { "your" } else { "their" };
        let id = card_id.unwrap_or("");
        let as_leader = force_leader
            || id.is_empty()
            || id.eq_ignore_ascii_case("leader")
            || id.eq_ignore_ascii_case(&side.leader_id);

        if as_leader {
            return format!(
                "{poss} {} at {} ({} life)",
                named(&side.leader_name, &side.leader_id),
                fmt_power(side.leader_power),
                side.life
            );
        }

        if let Some(body) = self
            .state
            .players
            .get(player)
            .and_then(|p| p.characters.iter().find(|c| c.card_id == id))
        {
            return format!("{poss} {}", self.body_label(body));
        }

        format!("{poss} {}", named(&lookup(self.repo, id), id))
    }

    fn plan(&self, line: String, steps: Vec<String>) -> CombatDoThis {
        CombatDoThis {
            line,
            steps,
            you: self.you_roster(),
            them: self.them_roster(),
        }
    }

    fn you_roster(&self) -> Vec<String> {
        self.side_roster(0)
    }

    fn them_roster(&self) -> Vec<String> {
        self.side_roster(1)
    }

    fn side_roster(&self, player: usize) -> Vec<String> {
        let side = if player == 0 { &self.you } else { &self.them };
        if side.leader_id.is_empty()
            && side.leader_name.is_empty()
            && self.state.players[player].characters.is_empty()
        {
            return Vec::new();
        }
        let mut rows = Vec::new();
        if !side.leader_id.is_empty() || !side.leader_name.is_empty() {
            rows.push(format!(
                "{} · {} · {} life",
                named(&side.leader_name, &side.leader_id),
                fmt_power(side.leader_power),
                side.life
            ));
            if !side.leader_text.is_empty() {
                rows.push(clip_text(&side.leader_text, 140));
            }
        }
        if side.don_active > 0 || side.don_rested > 0 || (player == 0 && side.hand_count > 0) {
            let mut meta = format!("{} DON · {} rest", side.don_active, side.don_rested);
            if player == 0 {
                meta.push_str(&format!(" · {} in hand", side.hand_count));
            }
            rows.push(meta);
        }
        for body in &self.state.players[player].characters {
            rows.push(self.body_short(body));
        }
        let named_hand: Vec<String> = self.state.players[player]
            .hand
            .iter()
            .filter(|card| !card.card_id.is_empty())
            .map(|card| self.hand_short(card))
            .collect();
        if !named_hand.is_empty() {
            rows.push(format!("Hand: {}", named_hand.join("; ")));
        }
        rows
    }

    fn hand_short(&self, card: &CardInstance) -> String {
        let def = self.repo.and_then(|r| r.get_by_id(&card.card_id).ok());
        let name = def
            .as_ref()
            .map(|d| d.name.as_str())
            .unwrap_or(card.card_id.as_str());
        let mut label = named(name, &card.card_id);
        if let Some(counter) = def.as_ref().map(|d| d.counter).filter(|c| *c > 0) {
            label.push_str(&format!(" · {} counter", fmt_power(counter)));
        }
        label
    }

    fn body_short(&self, body: &CardInstance) -> String {
        let def = self.repo.and_then(|r| r.get_by_id(&body.card_id).ok());
        let name = def
            .as_ref()
            .map(|d| d.name.as_str())
            .unwrap_or(body.card_id.as_str());
        let printed = def.as_ref().map(|d| d.power as i32).unwrap_or(0);
        let power = body.effective_power(printed.max(0) as u32);
        let stance = if body.rested || body.tapped {
            "rested"
        } else {
            "ready"
        };
        let mut bits = vec![
            named(name, &body.card_id),
            format!("{} {stance}", fmt_power(power)),
        ];
        if body.attached_don > 0 {
            bits.push(format!(
                "{} + {} DON",
                fmt_power(printed),
                body.attached_don
            ));
        }
        if def.as_ref().is_some_and(|d| d.keywords.blocker) {
            bits.push("Blocker".into());
        }
        if def.as_ref().is_some_and(|d| d.keywords.rush) {
            bits.push("Rush".into());
        }
        bits.join(" · ")
    }

    fn body_label(&self, body: &CardInstance) -> String {
        let def = self.repo.and_then(|r| r.get_by_id(&body.card_id).ok());
        let name = def
            .as_ref()
            .map(|d| d.name.as_str())
            .unwrap_or(body.card_id.as_str());
        let printed = def.as_ref().map(|d| d.power as i32).unwrap_or(0);
        let power = body.effective_power(printed.max(0) as u32);
        let stance = if body.rested || body.tapped {
            "rested"
        } else {
            "ready"
        };
        let mut label = format!(
            "{} at {} ({stance})",
            named(name, &body.card_id),
            fmt_power(power)
        );
        if body.attached_don > 0 {
            label.push_str(&format!(
                ", {} + {} DON",
                fmt_power(printed),
                body.attached_don
            ));
        }
        if def.as_ref().is_some_and(|d| d.keywords.blocker) {
            label.push_str(", Blocker");
        }
        if def.as_ref().is_some_and(|d| d.keywords.rush) {
            label.push_str(", Rush");
        }
        label
    }

    fn your_blockers(&self) -> Vec<String> {
        keyword_blockers(&self.state.players[0], self.repo)
            .into_iter()
            .map(|c| self.body_label(c))
            .collect()
    }

    fn their_blockers(&self) -> Vec<String> {
        keyword_blockers(&self.state.players[1], self.repo)
            .into_iter()
            .map(|c| self.body_label(c))
            .collect()
    }

    fn your_blocker_steps(&self) -> Vec<String> {
        let blockers = self.your_blockers();
        if blockers.is_empty() {
            vec!["You have no ready Blocker on the table.".into()]
        } else {
            vec![format!("Ready Blocker: {}.", blockers.join("; "))]
        }
    }

    fn their_blocker_watch(&self) -> Vec<String> {
        let blockers = self.their_blockers();
        if blockers.is_empty() {
            vec!["They have no ready Blocker showing.".into()]
        } else {
            vec![format!("Watch their Blocker: {}.", blockers.join("; "))]
        }
    }

    fn your_counter_steps(&self, required: i32) -> Vec<String> {
        let in_hand = hand_counters(&self.state.players[0], self.repo);
        let mut steps = Vec::new();
        if !in_hand.is_empty() {
            steps.push(format!("Counters in hand: {}.", in_hand.join("; ")));
        } else if self.you.hand_count > 0 {
            steps.push(format!(
                "You have {} cards in hand (about {} if they are 1k counters).",
                self.you.hand_count,
                fmt_power((self.you.hand_count as i32).min(5) * 1000)
            ));
        } else {
            steps.push("Your hand is empty — no counters.".into());
        }
        if required > 0 {
            steps.push(format!(
                "Need {} counter to keep this.",
                fmt_power(required)
            ));
        }
        steps
    }

    fn your_board_summary(&self) -> String {
        if self.state.players[0].characters.is_empty() {
            "No characters out.".into()
        } else {
            let bodies: Vec<_> = self.state.players[0]
                .characters
                .iter()
                .map(|c| self.body_label(c))
                .collect();
            format!("Your board: {}.", bodies.join("; "))
        }
    }

    fn their_board_summary(&self) -> String {
        if self.state.players[1].characters.is_empty() {
            format!(
                "Their {} at {}, {} life, empty board",
                named(&self.them.leader_name, &self.them.leader_id),
                fmt_power(self.them.leader_power),
                self.them.life
            )
        } else {
            let bodies: Vec<_> = self.state.players[1]
                .characters
                .iter()
                .map(|c| self.body_label(c))
                .collect();
            format!("Their board: {}.", bodies.join("; "))
        }
    }

    fn your_next_swing(&self) -> Option<String> {
        let attacker = self.state.players[0]
            .characters
            .iter()
            .find(|c| !c.rested && !c.tapped);
        if let Some(body) = attacker {
            let atk = self.body_label(body);
            if let Some(prey) = self.best_character_target(body) {
                return Some(format!("Swing {atk} into their {prey}."));
            }
            return Some(format!(
                "Swing {atk} at their {} at {} ({} life).",
                named(&self.them.leader_name, &self.them.leader_id),
                fmt_power(self.them.leader_power),
                self.them.life
            ));
        }
        if !self.state.players[0].leader.rested {
            return Some(format!(
                "Leader swing: {} at {} into their {} at {} ({} life).",
                named(&self.you.leader_name, &self.you.leader_id),
                fmt_power(self.you.leader_power),
                named(&self.them.leader_name, &self.them.leader_id),
                fmt_power(self.them.leader_power),
                self.them.life
            ));
        }
        None
    }

    fn best_character_target(&self, attacker: &CardInstance) -> Option<String> {
        let printed = self
            .repo
            .and_then(|r| r.get_by_id(&attacker.card_id).ok())
            .map(|d| d.power)
            .unwrap_or(0);
        let atk = attacker.effective_power(printed);
        self.state.players[1]
            .characters
            .iter()
            .filter_map(|c| {
                let def = self.repo.and_then(|r| r.get_by_id(&c.card_id).ok())?;
                let def_pow = c.effective_power(def.power);
                if atk > def_pow {
                    Some((def_pow, self.body_label(c)))
                } else {
                    None
                }
            })
            .max_by_key(|(pow, _)| *pow)
            .map(|(_, label)| label)
    }

    fn don_step(&self) -> String {
        let ready = self.state.players[0]
            .characters
            .iter()
            .find(|c| !c.rested && !c.tapped);
        if let Some(body) = ready {
            format!(
                "You have {} active DON — attach toward {} if this swing needs the extra 1k.",
                self.you.don_active,
                self.body_label(body)
            )
        } else {
            format!(
                "You have {} active DON — attach to {} before swinging.",
                self.you.don_active,
                named(&self.you.leader_name, &self.you.leader_id)
            )
        }
    }
}

fn side_view(player: &PlayerState, repo: Option<&CardRepository<'_>>) -> SideView {
    let leader_id = player.leader.card_id.clone();
    let (leader_name, leader_text) = resolve_leader(repo, &leader_id, &player.leader_name);
    SideView {
        life: player.life,
        don_active: player.don_active,
        don_rested: player.don_rested,
        hand_count: player.hand_count,
        leader_name,
        leader_id,
        leader_text,
        leader_power: player.leader.effective_power() as i32,
    }
}

fn resolve_leader(
    repo: Option<&CardRepository<'_>>,
    id: &str,
    observed: &str,
) -> (String, String) {
    if let Some(def) = repo.and_then(|r| {
        if id.is_empty() || id.eq_ignore_ascii_case("leader") {
            None
        } else {
            r.get_by_id(id).ok()
        }
    }) {
        let name = if def.name.is_empty() {
            if !observed.is_empty() {
                observed.to_string()
            } else {
                id.to_string()
            }
        } else {
            def.name
        };
        return (name, def.rules_text);
    }
    if !observed.is_empty() {
        if let Some(def) = repo.and_then(|r| {
            r.search_by_name(observed, 6).ok().and_then(|hits| {
                hits.into_iter().find(|d| {
                    d.card_type == optcg_core::CardType::Leader
                        && d.name.eq_ignore_ascii_case(observed)
                })
            })
        }) {
            return (def.name, def.rules_text);
        }
        return (observed.to_string(), String::new());
    }
    if id.is_empty() || id.eq_ignore_ascii_case("leader") {
        ("leader".into(), String::new())
    } else {
        (id.to_string(), String::new())
    }
}

fn lookup(repo: Option<&CardRepository<'_>>, id: &str) -> String {
    resolve_leader(repo, id, "").0
}

fn clip_text(text: &str, max: usize) -> String {
    let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= max {
        return one;
    }
    let mut clipped: String = one.chars().take(max.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

fn named(name: &str, id: &str) -> String {
    if id.is_empty() || id.eq_ignore_ascii_case("leader") {
        return name.to_string();
    }
    if name == id || name.is_empty() {
        id.to_string()
    } else {
        format!("{name} ({id})")
    }
}

fn keyword_blockers<'a>(
    player: &'a PlayerState,
    repo: Option<&CardRepository<'_>>,
) -> Vec<&'a CardInstance> {
    player
        .characters
        .iter()
        .filter(|c| !c.rested && !c.tapped)
        .filter(|c| {
            repo.and_then(|r| r.get_by_id(&c.card_id).ok())
                .is_some_and(|d| d.keywords.blocker)
        })
        .collect()
}

fn hand_counters(player: &PlayerState, repo: Option<&CardRepository<'_>>) -> Vec<String> {
    player
        .hand
        .iter()
        .filter_map(|c| {
            let def = repo.and_then(|r| r.get_by_id(&c.card_id).ok())?;
            if def.counter <= 0 {
                return None;
            }
            Some(format!(
                "{} {} counter",
                named(&def.name, &c.card_id),
                fmt_power(def.counter)
            ))
        })
        .collect()
}

fn fmt_power(n: i32) -> String {
    if n >= 1000 && n % 1000 == 0 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_core::{CardInstance, CombatState, Zone};
    use optcg_database::{AssetParser, CardRepository, Database};

    fn combat(attacker: &str, attacker_player: u8, target_player: u8) -> CombatState {
        CombatState {
            active: true,
            attacker_id: Some(attacker.into()),
            attacker_player: Some(attacker_player),
            target_id: Some("leader".into()),
            target_player: Some(target_player),
            target_is_leader: true,
            ..CombatState::default()
        }
    }

    #[test]
    fn names_the_bodies_and_the_math() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        let mut sanji = CardInstance::new("ST01-012", 1, Zone::Character);
        sanji.attached_don = 3;
        state.players[1].characters.push(sanji);
        state.players[0]
            .characters
            .push(CardInstance::new("ST01-010", 0, Zone::Character));
        state.players[0].hand_count = 0;
        state.players[0].life = 2;
        state.combat = combat("ST01-012", 1, 0);
        let analysis = crate::CombatMath::analyze_current_combat(&state, &repo).unwrap();
        let battle = battle_do_this(&state, Some(&repo), Some(&analysis)).unwrap();
        let line = battle.line.to_lowercase();
        assert!(line.contains("sanji"), "{line}");
        assert!(line.contains("st01-012"), "{line}");
        assert!(
            line.contains("luffy") || line.contains("st01-001"),
            "{line}"
        );
        assert!(line.contains("9k") || line.contains("lethal"), "{line}");
        assert!(battle
            .you
            .iter()
            .any(|s| s.contains("Nami") || s.contains("ST01-010")));
        assert!(battle
            .you
            .iter()
            .any(|s| s.contains("2 life") || s.contains("life")));
        assert!(battle
            .them
            .iter()
            .any(|s| s.contains("Sanji") || s.contains("ST01-012")));
    }

    #[test]
    fn table_plan_names_ready_attackers() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.phase = Phase::Main;
        state.players[0]
            .characters
            .push(CardInstance::new("ST01-012", 0, Zone::Character));
        state.players[1]
            .characters
            .push(CardInstance::new("ST01-002", 1, Zone::Character));
        let plan = table_do_this(&state, &repo).unwrap();
        assert!(plan.line.contains("Sanji") || plan.you.iter().any(|s| s.contains("Sanji")));
        assert!(plan
            .them
            .iter()
            .any(|s| s.contains("Usopp") || s.contains("ST01-002")));
    }

    #[test]
    fn roster_keeps_an_observed_leader_name_when_the_id_is_unknown() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.phase = Phase::Main;
        state.player_two_mut().set_leader_id("OP13-001");
        state.player_two_mut().set_leader_name("Silvers Rayleigh");
        let plan = table_do_this(&state, &repo).unwrap();
        assert!(
            plan.them
                .iter()
                .any(|s| s.contains("Silvers Rayleigh") && s.contains("OP13-001")),
            "{:?}",
            plan.them
        );
    }

    #[test]
    fn roster_names_cards_in_both_hands() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.phase = Phase::Main;
        state.players[0]
            .hand
            .push(CardInstance::new("ST01-007", 0, Zone::Hand));
        state.players[1]
            .hand
            .push(CardInstance::new("ST01-002", 1, Zone::Hand));
        let plan = table_do_this(&state, &repo).unwrap();
        assert!(
            plan.you
                .iter()
                .any(|s| s.contains("ST01-007") || s.contains("Nami")),
            "{:?}",
            plan.you
        );
        assert!(
            plan.them
                .iter()
                .any(|s| s.contains("ST01-002") || s.contains("Usopp")),
            "{:?}",
            plan.them
        );
    }
}
