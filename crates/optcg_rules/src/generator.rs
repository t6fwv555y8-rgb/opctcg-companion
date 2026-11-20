//! Build a 50-card list for a Leader from what winning lists actually run.
//!
//! The generator does not invent a deck. It takes the winning lists for one
//! build of a Leader, weights newer and bigger events more, and keeps the
//! cards most of those lists share, at the copy count they usually run. It
//! stays legal (banned cards, pair bans, rotation, four copies, exactly 50).
//! The result is a consensus of what won, a starting point, not a promise.

use crate::legality::BanList;
use crate::leader_stats::card_tags;
use crate::meta::{MetaData, MIN_LISTS_FOR_ADVICE};
use crate::variants::{cluster, VariantInfo};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DECK_SIZE: u32 = 50;
const MAX_COPIES: u32 = 4;
/// Generating needs more lists than comparing does: a consensus of two is a coin toss.
pub const MIN_LISTS_TO_GENERATE: usize = 5;
/// A card must be in at least this share (weighted) of the build's lists to be a first pick.
const CORE_SHARE: f32 = 0.35;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedCard {
    pub card_id: String,
    pub name: String,
    pub quantity: u32,
    pub cost: u32,
    pub card_type: String,
    /// In how many of the build's winning lists the card appears.
    pub in_lists: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneratedDeck {
    pub leader_id: String,
    pub leader_name: String,
    pub format: String,
    /// The build this was made from, and every build that exists for the Leader.
    pub variant: Option<VariantInfo>,
    pub variants: Vec<VariantInfo>,
    /// Winning lists the deck was built from.
    pub based_on_lists: usize,
    pub total_cards: u32,
    pub cards: Vec<GeneratedCard>,
    /// Plain-language caveats and checks.
    pub notes: Vec<String>,
    /// Paste-ready list in the app's own format.
    pub deck_text: String,
    /// Opens the same list in Egman's deck builder.
    pub builder_url: String,
}

/// Why a deck could not be generated.
pub fn generate(
    meta: &MetaData,
    repo: &CardRepository<'_>,
    banlist: Option<&BanList>,
    leader_id: &str,
    format: Option<&str>,
    variant: Option<usize>,
) -> Result<GeneratedDeck, String> {
    let format = format
        .map(str::to_string)
        .or_else(|| meta.default_format())
        .unwrap_or_default();
    let filter = (!format.is_empty()).then_some(format.as_str());
    let all = meta.lists_for(leader_id, filter);
    let leader_name = repo
        .get_by_id(leader_id)
        .map(|d| d.name)
        .unwrap_or_else(|_| leader_id.to_string());
    if all.len() < MIN_LISTS_TO_GENERATE {
        return Err(format!(
            "Only {} winning list{} for {leader_name} in the data. Building a deck needs at least {MIN_LISTS_TO_GENERATE}.",
            all.len(),
            if all.len() == 1 { "" } else { "s" },
        ));
    }

    // Use one coherent build; fall back to every list when a build is too small.
    let clusters = cluster(&all, repo);
    let variants: Vec<VariantInfo> = clusters.iter().map(|c| c.info.clone()).collect();
    let chosen = variant.unwrap_or(0);
    let mut notes = Vec::new();
    let (lists, used_variant) = match clusters.get(chosen) {
        Some(c) if c.members.len() >= MIN_LISTS_TO_GENERATE => (c.members.clone(), Some(c.info.clone())),
        Some(c) => {
            notes.push(format!(
                "The {} has only {} lists, so the whole Leader's lists were used instead.",
                c.info.label, c.info.lists
            ));
            (all.clone(), None)
        }
        None => (all.clone(), None),
    };

    let weights = meta.weights_for(&lists);
    let total_weight: f32 = weights.iter().sum::<f32>().max(f32::EPSILON);

    // Weighted share of lists running each card, and the copies they run.
    struct Score {
        share: f32,
        copies: f32,
        in_lists: u32,
    }
    let mut scores: HashMap<&str, (f32, f32, u32)> = HashMap::new(); // weight in, weight*qty, lists
    for (deck, w) in lists.iter().zip(&weights) {
        for (id, qty) in &deck.cards {
            if id.eq_ignore_ascii_case(leader_id) {
                continue;
            }
            let e = scores.entry(id.as_str()).or_default();
            e.0 += w;
            e.1 += w * f32::from(*qty);
            e.2 += 1;
        }
    }
    let mut ranked: Vec<(&str, Score)> = scores
        .into_iter()
        .filter(|(id, _)| banlist.map_or(true, |b| b.card_is_legal(id)))
        .map(|(id, (w, wq, n))| {
            (id, Score { share: w / total_weight, copies: wq / w.max(f32::EPSILON), in_lists: n })
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.share
            .partial_cmp(&a.1.share)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.1.copies.partial_cmp(&a.1.copies).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.0.cmp(b.0))
    });

    let conflicts = |chosen: &HashMap<&str, u32>, id: &str| -> bool {
        banlist.is_some_and(|b| {
            b.pair_bans.iter().any(|p| {
                let in_a = |x: &str| p.a.iter().any(|c| c.eq_ignore_ascii_case(x));
                let in_b = |x: &str| p.b.iter().any(|c| c.eq_ignore_ascii_case(x));
                (in_a(id) && chosen.keys().any(|c| in_b(c))) || (in_b(id) && chosen.keys().any(|c| in_a(c)))
            })
        })
    };

    // Pass 1: the cards most lists share, at the copy count they usually run.
    let mut picked: HashMap<&str, u32> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    let mut total = 0u32;
    for (id, s) in ranked.iter().filter(|(_, s)| s.share >= CORE_SHARE) {
        if total >= DECK_SIZE || conflicts(&picked, id) {
            continue;
        }
        let want = (s.copies.round() as u32).clamp(1, MAX_COPIES).min(DECK_SIZE - total);
        picked.insert(id, want);
        order.push(id);
        total += want;
    }
    // Pass 2: still short, so add copies to the most common cards, then new cards.
    for id in order.clone() {
        if total >= DECK_SIZE {
            break;
        }
        let have = picked[id];
        if have < MAX_COPIES {
            let add = (MAX_COPIES - have).min(DECK_SIZE - total);
            *picked.get_mut(id).unwrap() += add;
            total += add;
        }
    }
    for (id, s) in ranked.iter() {
        if total >= DECK_SIZE {
            break;
        }
        if picked.contains_key(id) || conflicts(&picked, id) {
            continue;
        }
        let want = (s.copies.round() as u32).clamp(1, MAX_COPIES).min(DECK_SIZE - total);
        picked.insert(id, want);
        order.push(id);
        total += want;
    }
    if total < DECK_SIZE {
        return Err(format!(
            "The winning lists only gave {total} legal cards, not {DECK_SIZE}. The data is too thin for this Leader."
        ));
    }

    let mut cards: Vec<GeneratedCard> = order
        .iter()
        .map(|id| {
            let def = repo.get_by_id(id).ok();
            GeneratedCard {
                card_id: (*id).to_string(),
                name: def.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| (*id).to_string()),
                quantity: picked[id],
                cost: def.as_ref().map(|d| d.cost).unwrap_or(0),
                card_type: def
                    .as_ref()
                    .map(|d| format!("{:?}", d.card_type).to_lowercase())
                    .unwrap_or_else(|| "unknown".into()),
                in_lists: ranked.iter().find(|(r, _)| r == id).map(|(_, s)| s.in_lists).unwrap_or(0),
                tags: def
                    .as_ref()
                    .map(|d| card_tags(d).into_iter().map(str::to_string).collect())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let type_rank = |t: &str| match t {
        "character" => 0,
        "event" => 1,
        "stage" => 2,
        _ => 3,
    };
    cards.sort_by(|a, b| {
        type_rank(&a.card_type)
            .cmp(&type_rank(&b.card_type))
            .then(a.cost.cmp(&b.cost))
            .then(a.name.cmp(&b.name))
    });

    // Check the result the same way a player's own list is checked.
    let as_map: HashMap<String, u8> = cards.iter().map(|c| (c.card_id.clone(), c.quantity as u8)).collect();
    if let Some(b) = banlist {
        for v in b.check(leader_id, &as_map, repo) {
            notes.push(format!("Check: {} ({})", v.name, v.reason));
        }
    }
    let unknown = cards.iter().filter(|c| c.card_type == "unknown").count();
    if unknown > 0 {
        notes.push(format!(
            "{unknown} card{} not in the card database, so cost and type are blank. Run the card download.",
            if unknown == 1 { " is" } else { "s are" }
        ));
    }
    if lists.len() < 10 {
        notes.push(format!("Built from only {} lists: treat it as a rough starting point.", lists.len()));
    }
    notes.push(
        "This is what most winning lists of this build share. It is a starting point, not a guaranteed winner."
            .into(),
    );

    let mut deck_text = format!("Deck: {leader_name} (generated)\nLeader: {leader_id}\n");
    for c in &cards {
        deck_text.push_str(&format!("{}x {}\n", c.quantity, c.card_id));
    }
    let builder_url = format!(
        "https://deckbuilder.egmanevents.com/?deck={leader_id}:1,{}&type=optcg",
        cards
            .iter()
            .map(|c| format!("{}:{}", c.card_id, c.quantity))
            .collect::<Vec<_>>()
            .join(",")
    );

    Ok(GeneratedDeck {
        leader_id: leader_id.to_string(),
        leader_name,
        format,
        variant: used_variant,
        variants,
        based_on_lists: lists.len(),
        total_cards: total,
        cards,
        notes,
        deck_text,
        builder_url,
    })
}

/// Leaders with enough lists to generate from, most lists first.
pub fn generatable_leaders(
    meta: &MetaData,
    repo: &CardRepository<'_>,
    format: Option<&str>,
) -> Vec<(String, String, usize)> {
    let format = format.map(str::to_string).or_else(|| meta.default_format()).unwrap_or_default();
    let filter = (!format.is_empty()).then_some(format.as_str());
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for d in &meta.decks {
        if filter.map_or(true, |f| d.format.eq_ignore_ascii_case(f)) {
            *counts.entry(d.leader.as_str()).or_default() += 1;
        }
    }
    let mut out: Vec<(String, String, usize)> = counts
        .into_iter()
        .filter(|(_, n)| *n >= MIN_LISTS_TO_GENERATE.max(MIN_LISTS_FOR_ADVICE))
        .map(|(id, n)| {
            let name = repo.get_by_id(id).map(|d| d.name).unwrap_or_else(|_| id.to_string());
            (id.to_string(), name, n)
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legality::PairBan;
    use crate::meta::{MetaDeck, MetaTournament};
    use optcg_database::{AssetParser, Database};

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        db
    }

    /// 14 cards x4 plus a swapped-in pair; richer than a real list so bans still leave 50 legal cards.
    fn list(i: usize, base: &[&str], swap: &str) -> MetaDeck {
        let mut cards: HashMap<String, u8> = base.iter().map(|c| (c.to_string(), 4u8)).collect();
        cards.insert(swap.to_string(), 2);
        let _ = i;
        MetaDeck {
            leader: "L-001".into(),
            format: "OP17".into(),
            tournament_id: "t".into(),
            cards,
            ..Default::default()
        }
    }

    fn base() -> Vec<&'static str> {
        vec![
            "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C10", "C11", "C12", "C13", "C14",
        ]
    }

    fn meta(n: usize) -> MetaData {
        MetaData {
            tournaments: vec![MetaTournament { id: "t".into(), date: "2026-09-29".into(), players: 128, ..Default::default() }],
            decks: (0..n).map(|i| list(i, &base(), if i % 2 == 0 { "X1" } else { "X2" })).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn it_builds_exactly_fifty_cards_with_at_most_four_copies() {
        let db = db();
        let repo = CardRepository::new(&db);
        let deck = generate(&meta(8), &repo, None, "L-001", None, None).unwrap();
        assert_eq!(deck.total_cards, 50);
        assert_eq!(deck.cards.iter().map(|c| c.quantity).sum::<u32>(), 50);
        assert!(deck.cards.iter().all(|c| (1..=4).contains(&c.quantity)));
        assert!(deck.cards.iter().all(|c| c.card_id != "L-001"), "the Leader is not one of the 50");
        assert_eq!(deck.based_on_lists, 8);
        assert!(deck.deck_text.contains("Leader: L-001") && deck.deck_text.contains("4x C01"));
        assert!(deck.builder_url.contains("deck=L-001:1,"));
    }

    #[test]
    fn it_is_deterministic() {
        let db = db();
        let repo = CardRepository::new(&db);
        let a = generate(&meta(8), &repo, None, "L-001", None, None).unwrap();
        let b = generate(&meta(8), &repo, None, "L-001", None, None).unwrap();
        assert_eq!(a.deck_text, b.deck_text);
    }

    #[test]
    fn it_never_uses_banned_cards_or_both_halves_of_a_pair_ban() {
        let db = db();
        let repo = CardRepository::new(&db);
        let banlist = BanList {
            banned: ["C01".to_string()].into(),
            pair_bans: vec![PairBan { a: vec!["C02".into()], b: vec!["C03".into()] }],
            ..Default::default()
        };
        let deck = generate(&meta(8), &repo, Some(&banlist), "L-001", None, None).unwrap();
        let ids: Vec<_> = deck.cards.iter().map(|c| c.card_id.as_str()).collect();
        assert!(!ids.contains(&"C01"));
        assert!(!(ids.contains(&"C02") && ids.contains(&"C03")), "{ids:?}");
        assert_eq!(deck.total_cards, 50);
        assert!(deck.notes.iter().all(|n| !n.starts_with("Check:")), "{:?}", deck.notes);
    }

    #[test]
    fn it_refuses_to_build_from_too_few_lists() {
        let db = db();
        let repo = CardRepository::new(&db);
        let err = generate(&meta(3), &repo, None, "L-001", None, None).unwrap_err();
        assert!(err.contains("needs at least 5"), "{err}");
    }

    #[test]
    fn leaders_with_enough_lists_are_offered_most_played_first() {
        let db = db();
        let repo = CardRepository::new(&db);
        let mut data = meta(8);
        data.decks.extend((0..5).map(|i| MetaDeck { leader: "L-002".into(), ..list(i, &base(), "X1") }));
        let leaders = generatable_leaders(&data, &repo, None);
        assert_eq!(leaders[0].0, "L-001");
        assert_eq!(leaders.len(), 2);
    }
}
