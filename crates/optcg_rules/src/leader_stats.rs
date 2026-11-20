//! What winning lists for a Leader look like, counted from real decklists.
//!
//! Everything here is a count or an average over tournament top-cut lists and
//! the printed card text. It describes; it does not recommend. The tags
//! ("K.O. effect", "bounce") come from simple text patterns, so they are
//! approximate and the UI words them that way.

use crate::legality::BanList;
use crate::meta::MetaData;
use optcg_core::CardDefinition;
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Fewer lists than this and nothing here is shown as a pattern.
pub const MIN_LISTS: usize = 3;

/// Short labels for what a card does that matters in a matchup.
pub fn card_tags(def: &CardDefinition) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if def.keywords.blocker {
        tags.push("Blocker");
    }
    if def.keywords.rush {
        tags.push("Rush");
    }
    if def.keywords.double_attack {
        tags.push("Double Attack");
    }
    if def.keywords.banish {
        tags.push("Banish");
    }
    let text = def.rules_text.as_str();
    if text.contains("K.O. up to") || text.contains("K.O. 1") || text.contains("K.O. all") {
        tags.push("K.O. effect");
    }
    // One sentence has to say all of it, so "return" in one effect and
    // "opponent" in another do not add up to a bounce effect.
    let bounces = text.split(['.', '\n']).any(|sentence| {
        let s = sentence.to_lowercase();
        s.contains("return") && s.contains("owner's hand") && s.contains("opponent")
    });
    if bounces {
        tags.push("bounce");
    }
    tags
}

/// The `Type:` line the card sync writes at the top of the text.
pub fn card_types(def: &CardDefinition) -> Vec<String> {
    def.rules_text
        .lines()
        .find_map(|line| line.strip_prefix("Type: "))
        .map(|rest| rest.split(" / ").map(|t| t.trim().to_string()).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreCard {
    pub card_id: String,
    pub name: String,
    pub in_lists: u32,
    pub avg_copies: f32,
    pub cost: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaderStats {
    pub leader_id: String,
    pub leader_name: String,
    pub format: String,
    pub lists: usize,
    pub enough_data: bool,
    /// Cards most lists run, most common first.
    pub core: Vec<CoreCard>,
    /// Average copies per 50-card list.
    pub avg_counter_cards: f32,
    pub avg_blockers: f32,
    pub avg_rush: f32,
    pub avg_double_attack: f32,
    pub avg_banish: f32,
    pub avg_ko_effects: f32,
    pub avg_bounce: f32,
    pub avg_events: f32,
    pub avg_characters: f32,
    /// Card types most of the deck shares, with average copies per list.
    pub top_types: Vec<(String, f32)>,
    /// Cards that matter in a matchup (tagged) and that many lists run.
    pub notable: Vec<CoreCard>,
}

/// Count winning lists for `leader_id` (in `format`, or the most common one).
pub fn leader_stats(
    meta: &MetaData,
    repo: &CardRepository<'_>,
    banlist: Option<&BanList>,
    leader_id: &str,
    format: Option<&str>,
) -> LeaderStats {
    let format = format
        .map(str::to_string)
        .or_else(|| meta.default_format())
        .unwrap_or_default();
    let filter = (!format.is_empty()).then_some(format.as_str());
    let lists = meta.lists_for(leader_id, filter);
    let n = lists.len();

    let mut out = LeaderStats {
        leader_id: leader_id.to_string(),
        leader_name: repo
            .get_by_id(leader_id)
            .map(|d| d.name)
            .unwrap_or_else(|_| leader_id.to_string()),
        format,
        lists: n,
        enough_data: n >= MIN_LISTS,
        ..Default::default()
    };
    if !out.enough_data {
        return out;
    }

    // Look each distinct card up once.
    let mut defs: HashMap<&str, Option<CardDefinition>> = HashMap::new();
    for deck in &lists {
        for id in deck.cards.keys() {
            if !defs.contains_key(id.as_str()) {
                defs.insert(id.as_str(), repo.get_by_id(id).ok());
            }
        }
    }

    let mut seen_in: HashMap<&str, u32> = HashMap::new();
    let mut copies: HashMap<&str, u32> = HashMap::new();
    let mut sums = [0f32; 8]; // counter, blocker, rush, double, banish, ko, bounce, events
    let mut characters = 0f32;
    let mut type_copies: HashMap<String, u32> = HashMap::new();

    for deck in &lists {
        for (id, qty) in &deck.cards {
            let q = u32::from(*qty);
            *seen_in.entry(id.as_str()).or_default() += 1;
            *copies.entry(id.as_str()).or_default() += q;
            let Some(Some(def)) = defs.get(id.as_str()) else {
                continue;
            };
            let qf = q as f32;
            if def.counter > 0 {
                sums[0] += qf;
            }
            if def.keywords.blocker {
                sums[1] += qf;
            }
            if def.keywords.rush {
                sums[2] += qf;
            }
            if def.keywords.double_attack {
                sums[3] += qf;
            }
            if def.keywords.banish {
                sums[4] += qf;
            }
            let tags = card_tags(def);
            if tags.contains(&"K.O. effect") {
                sums[5] += qf;
            }
            if tags.contains(&"bounce") {
                sums[6] += qf;
            }
            if matches!(def.card_type, optcg_core::CardType::Event) {
                sums[7] += qf;
            }
            if matches!(def.card_type, optcg_core::CardType::Character) {
                characters += qf;
            }
            for t in card_types(def) {
                *type_copies.entry(t).or_default() += q;
            }
        }
    }
    let nf = n as f32;
    out.avg_counter_cards = sums[0] / nf;
    out.avg_blockers = sums[1] / nf;
    out.avg_rush = sums[2] / nf;
    out.avg_double_attack = sums[3] / nf;
    out.avg_banish = sums[4] / nf;
    out.avg_ko_effects = sums[5] / nf;
    out.avg_bounce = sums[6] / nf;
    out.avg_events = sums[7] / nf;
    out.avg_characters = characters / nf;

    let mut types: Vec<(String, f32)> = type_copies
        .into_iter()
        .map(|(t, c)| (t, c as f32 / nf))
        .filter(|(_, avg)| *avg >= 8.0) // a type most of the deck shares, not a stray card
        .collect();
    types.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    types.truncate(3);
    out.top_types = types;

    let allowed = |id: &str| banlist.map_or(true, |b| b.card_is_legal(id));
    let mut cards: Vec<CoreCard> = seen_in
        .iter()
        .filter(|(id, _)| allowed(id))
        .map(|(id, seen)| {
            let def = defs.get(id).and_then(|d| d.as_ref());
            CoreCard {
                card_id: (*id).to_string(),
                name: def.map(|d| d.name.clone()).unwrap_or_else(|| (*id).to_string()),
                in_lists: *seen,
                avg_copies: copies[id] as f32 / *seen as f32,
                cost: def.map(|d| d.cost).unwrap_or(0),
                tags: def
                    .map(|d| card_tags(d).into_iter().map(str::to_string).collect())
                    .unwrap_or_default(),
            }
        })
        .collect();
    cards.sort_by(|a, b| b.in_lists.cmp(&a.in_lists).then(a.card_id.cmp(&b.card_id)));
    out.notable = cards
        .iter()
        .filter(|c| !c.tags.is_empty() && c.in_lists as usize * 10 >= n * 3)
        .take(8)
        .cloned()
        .collect();
    cards.truncate(12);
    out.core = cards;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::MetaDeck;
    use optcg_database::{AssetParser, Database};

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        db
    }

    fn data(n: usize) -> MetaData {
        MetaData {
            decks: (0..n)
                .map(|_| MetaDeck {
                    leader: "ST01-001".into(),
                    format: "OP17".into(),
                    cards: [("ST01-002".to_string(), 4u8), ("ST01-003".to_string(), 4u8)].into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn counts_are_averages_per_list() {
        let db = db();
        let repo = CardRepository::new(&db);
        let stats = leader_stats(&data(4), &repo, None, "ST01-001", None);
        assert_eq!((stats.lists, stats.enough_data), (4, true));
        assert_eq!(stats.core[0].in_lists, 4);
        assert!((stats.core[0].avg_copies - 4.0).abs() < 0.01);
        // Both cards carry a printed Counter in the starter data.
        assert!(stats.avg_counter_cards >= 0.0);
    }

    #[test]
    fn too_few_lists_report_nothing_as_a_pattern() {
        let db = db();
        let repo = CardRepository::new(&db);
        let stats = leader_stats(&data(2), &repo, None, "ST01-001", None);
        assert!(!stats.enough_data);
        assert!(stats.core.is_empty());
    }

    #[test]
    fn tags_come_from_keywords_and_card_text() {
        let mut def = CardDefinition::from_parts(
            "X-001".into(),
            "Tester".into(),
            optcg_core::CardType::Character,
            3,
            4000,
            0,
            "Red".into(),
            optcg_core::Keywords { blocker: true, ..Default::default() },
            "Type: Elbaph / Tester\n[On Play] K.O. up to 1 of your opponent's Characters with a cost of 3 or less.".into(),
        );
        let tags = card_tags(&def);
        assert!(tags.contains(&"Blocker") && tags.contains(&"K.O. effect"), "{tags:?}");
        assert_eq!(card_types(&def), vec!["Elbaph", "Tester"]);
        def.rules_text = "[On Play] Return 1 of your Characters to the owner's hand. Your opponent trashes 1 card.".into();
        assert!(!card_tags(&def).contains(&"bounce"), "return and opponent in different sentences");
        def.rules_text = "[On Play] Return up to 1 of your opponent's Characters to the owner's hand.".into();
        assert!(card_tags(&def).contains(&"bounce"));
        def.rules_text = "[On K.O.] Draw 1 card.".into();
        assert!(!card_tags(&def).contains(&"K.O. effect"), "an On K.O. trigger is not removal");
    }
}
