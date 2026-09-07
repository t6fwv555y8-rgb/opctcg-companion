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
        let math = format!(
            "{} vs {} (need {need} to hold)",
            fmt_power(a.attacker_power),
            fmt_power(a.defender_power)
        );
        if defending {
            if a.lethal_to_leader {
                let mut steps = vec![swing.clone(), format!("Math: {math}.")];
                steps.extend(table.your_blocker_steps());
                steps.extend(table.your_counter_steps(a.required_counter));
                steps.push(format!(
                    "You are at {} life. If this hits, you lose.",
                    table.you.life
                ));
                let line = match table.your_blockers().first() {
                    Some(blocker) => format!(
                        "{swing} Block with {blocker} or counter {need} — this is lethal."
                    ),
                    None => format!("{swing} Counter {need} or you lose — this is lethal."),
                };
                return Some(CombatDoThis { line, steps });
            }
            if a.recommended_block || (state.combat.blocker_offered && !a.survives_without_counter)
            {
                let blocker = table
                    .your_blockers()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "a ready Blocker".into());
                let mut steps = vec![swing.clone(), format!("Math: {math}.")];
                steps.push(format!("Block with {blocker}."));
                if a.required_counter > 0 {
                    steps.extend(table.your_counter_steps(a.required_counter));
                }
                return Some(CombatDoThis {
                    line: format!("{swing} Block with {blocker}."),
                    steps,
                });
            }
            if a.required_counter > 0 && !a.survives_without_counter {
                let mut steps = vec![swing.clone(), format!("Math: {math}.")];
                steps.extend(table.your_counter_steps(a.required_counter));
                steps.push("If you keep the cards, take the hit.".into());
                return Some(CombatDoThis {
                    line: format!("{swing} Counter {need} or take the hit."),
                    steps,
                });
            }
            if a.survives_without_counter {
                return Some(CombatDoThis {
                    line: format!("{swing} They don't break through — take it."),
                    steps: vec![
                        swing,
                        format!("Math: {math}."),
                        "Don't spend a Blocker or counter here.".into(),
                    ],
                });
            }
        } else if a.lethal_to_leader {
            let mut steps = vec![swing.clone(), format!("Math: {math}.")];
            steps.extend(table.their_blocker_watch());
            return Some(CombatDoThis {
                line: format!("{swing} This is lethal — go through."),
                steps,
            });
        } else if a.required_counter > 0 {
            let mut steps = vec![swing.clone(), format!("Math: {math}.")];
            steps.extend(table.their_blocker_watch());
            return Some(CombatDoThis {
                line: format!("{swing} They need {need} to live."),
                steps,
            });
        } else {
            return Some(CombatDoThis {
                line: format!("{swing} They don't break this — resolve."),
                steps: vec![swing, format!("Math: {math}.")],
            });
        }
    }

    let mut steps = vec![swing.clone()];
    steps.extend(table.board_steps());
    if state.combat.blocker_offered {
        steps.extend(table.your_blocker_steps());
        return Some(CombatDoThis {
            line: format!("{swing} Blocker window — decide now."),
            steps,
        });
    }
    if defending {
        steps.extend(table.your_blocker_steps());
        return Some(CombatDoThis {
            line: format!("{swing} Block, counter, or take it."),
            steps,
        });
    }
    steps.extend(table.their_blocker_watch());
    Some(CombatDoThis {
        line: format!("{swing} Resolve this swing."),
        steps,
    })
}

/// When no attack is open: name the bodies on the table and the next swing.
pub fn table_do_this(
    state: &GameState,
    repo: &CardRepository<'_>,
) -> Option<CombatDoThis> {
    if state.combat.active {
        return None;
    }
    let you = &state.players[0];
    let them = &state.players[1];
    if you.leader.card_id.is_empty() && you.characters.is_empty() && them.characters.is_empty() {
        return None;
    }

    let table = Table::read(state, Some(repo));
    let mut steps = table.board_steps();
    if steps.is_empty() {
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

    if let Some(swing) = table.your_next_swing() {
        if state.active_player == 0 {
            steps.insert(0, swing);
        }
    }
    if you.don_active > 0 && matches!(state.phase, Phase::Don | Phase::Main) {
        steps.push(table.don_step());
    }

    Some(CombatDoThis { line, steps })
}

struct SideView {
    life: u32,
    don_active: u32,
    don_rested: u32,
    hand_count: u32,
    leader_id: String,
    leader_name: String,
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
        analysis.is_some_and(|a| {
            a.lethal_to_leader || a.recommended_block || a.required_counter > 0
        }) && self.state.combat.target_is_leader
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

        if let Some(body) = self.state.players.get(player).and_then(|p| {
            p.characters.iter().find(|c| c.card_id == id)
        }) {
            return format!("{poss} {}", self.body_label(body));
        }

        format!("{poss} {}", named(&lookup(self.repo, id), id))
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
        let mut label = format!("{} at {} ({stance})", named(name, &body.card_id), fmt_power(power));
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
            steps.push(format!(
                "Counters in hand: {}.",
                in_hand.join("; ")
            ));
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
            steps.push(format!("Need {} counter to keep this.", fmt_power(required)));
        }
        steps
    }

    fn board_steps(&self) -> Vec<String> {
        let mut steps = Vec::new();
        steps.push(format!(
            "You: {} at {}, {} life, {} active DON / {} rested, {} in hand.",
            named(&self.you.leader_name, &self.you.leader_id),
            fmt_power(self.you.leader_power),
            self.you.life,
            self.you.don_active,
            self.you.don_rested,
            self.you.hand_count
        ));
        if !self.state.players[0].characters.is_empty() {
            let bodies: Vec<_> = self.state.players[0]
                .characters
                .iter()
                .map(|c| self.body_label(c))
                .collect();
            steps.push(format!("Your characters: {}.", bodies.join("; ")));
        } else {
            steps.push("You have no characters on the table.".into());
        }
        steps.push(format!(
            "Them: {} at {}, {} life, {} active DON / {} rested.",
            named(&self.them.leader_name, &self.them.leader_id),
            fmt_power(self.them.leader_power),
            self.them.life,
            self.them.don_active,
            self.them.don_rested
        ));
        if !self.state.players[1].characters.is_empty() {
            let bodies: Vec<_> = self.state.players[1]
                .characters
                .iter()
                .map(|c| self.body_label(c))
                .collect();
            steps.push(format!("Their characters: {}.", bodies.join("; ")));
        } else {
            steps.push("They have no characters on the table.".into());
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
        let ready = self
            .state
            .players[0]
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
    let leader_name = lookup(repo, &leader_id);
    SideView {
        life: player.life,
        don_active: player.don_active,
        don_rested: player.don_rested,
        hand_count: player.hand_count,
        leader_name,
        leader_id,
        leader_power: player.leader.effective_power() as i32,
    }
}

fn lookup(repo: Option<&CardRepository<'_>>, id: &str) -> String {
    if id.is_empty() || id.eq_ignore_ascii_case("leader") {
        return "leader".into();
    }
    repo.and_then(|r| r.get_by_id(id).ok())
        .map(|d| d.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| id.to_string())
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
        state.players[0].characters.push(CardInstance::new("ST01-010", 0, Zone::Character));
        state.players[0].hand_count = 0;
        state.players[0].life = 2;
        state.combat = combat("ST01-012", 1, 0);
        let analysis = crate::CombatMath::analyze_current_combat(&state, &repo).unwrap();
        let battle = battle_do_this(&state, Some(&repo), Some(&analysis)).unwrap();
        let line = battle.line.to_lowercase();
        assert!(line.contains("sanji"), "{line}");
        assert!(line.contains("st01-012"), "{line}");
        assert!(line.contains("luffy") || line.contains("st01-001"), "{line}");
        assert!(line.contains("9k") || line.contains("lethal"), "{line}");
        assert!(battle.steps.iter().any(|s| s.contains("Nami") || s.contains("ST01-010")));
        assert!(battle.steps.iter().any(|s| s.contains("2 life") || s.contains("life")));
    }

    #[test]
    fn table_plan_names_ready_attackers() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let mut state = GameState::new();
        state.phase = Phase::Main;
        state.players[0].characters.push(CardInstance::new("ST01-012", 0, Zone::Character));
        state.players[1].characters.push(CardInstance::new("ST01-002", 1, Zone::Character));
        let plan = table_do_this(&state, &repo).unwrap();
        assert!(plan.line.contains("Sanji") || plan.steps.iter().any(|s| s.contains("Sanji")));
        assert!(plan.steps.iter().any(|s| s.contains("Usopp") || s.contains("ST01-002")));
    }
}
