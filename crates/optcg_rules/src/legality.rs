//! Whether a deck is legal in the Standard format: banned cards, banned pairs,
//! and cards from rotated-out blocks.
//!
//! The data comes from `banlist.json`, written by `scripts/sync_meta.py`. It is
//! a snapshot with a date, never live: the UI shows when it was last updated.

use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Cards that may not appear in the same deck: any of `a` with any of `b`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PairBan {
    #[serde(default)]
    pub a: Vec<String>,
    #[serde(default)]
    pub b: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BanList {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub last_updated: String,
    #[serde(default)]
    pub banned: HashSet<String>,
    #[serde(default)]
    pub pair_bans: Vec<PairBan>,
    #[serde(default)]
    pub rotated_blocks: Vec<u32>,
    /// Every card from a rotated block (reprints in a newer block are not here).
    #[serde(default)]
    pub rotated_cards: HashSet<String>,
}

/// One reason a deck is not legal, with the card it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub card_id: String,
    pub name: String,
    /// "banned", "pair" or "rotated".
    pub kind: String,
    pub reason: String,
}

impl BanList {
    /// Read `banlist.json`; `None` when it is absent or unreadable.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// True when the card alone is allowed (pair bans need the whole deck).
    pub fn card_is_legal(&self, card_id: &str) -> bool {
        let id = card_id.to_uppercase();
        !self.banned.contains(&id) && !self.rotated_cards.contains(&id)
    }

    /// Every reason `leader` plus `cards` (card id to copies) is not legal.
    pub fn check(
        &self,
        leader: &str,
        cards: &HashMap<String, u8>,
        repo: &CardRepository<'_>,
    ) -> Vec<Violation> {
        let name = |id: &str| {
            repo.get_by_id(id)
                .map(|d| d.name)
                .unwrap_or_else(|_| id.to_string())
        };
        let mut ids: Vec<String> = cards
            .iter()
            .filter(|(_, qty)| **qty > 0)
            .map(|(id, _)| id.to_uppercase())
            .collect();
        ids.push(leader.to_uppercase());
        ids.sort();
        ids.dedup();

        let mut out = Vec::new();
        for id in &ids {
            if self.banned.contains(id) {
                out.push(Violation {
                    card_id: id.clone(),
                    name: name(id),
                    kind: "banned".into(),
                    reason: "banned in Standard".into(),
                });
            } else if self.rotated_cards.contains(id) {
                out.push(Violation {
                    card_id: id.clone(),
                    name: name(id),
                    kind: "rotated".into(),
                    reason: "rotated out of Standard".into(),
                });
            }
        }
        for pair in &self.pair_bans {
            for a in pair.a.iter().filter(|a| ids.contains(&a.to_uppercase())) {
                for b in pair.b.iter().filter(|b| ids.contains(&b.to_uppercase())) {
                    out.push(Violation {
                        card_id: a.clone(),
                        name: name(a),
                        kind: "pair".into(),
                        reason: format!("can't be played with {} ({b})", name(b)),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_database::{AssetParser, Database};

    fn list() -> BanList {
        BanList {
            banned: ["ST01-010".to_string()].into(),
            pair_bans: vec![PairBan {
                a: vec!["ST01-002".into()],
                b: vec!["ST01-003".into(), "ST01-012".into()],
            }],
            rotated_cards: ["ST01-014".to_string()].into(),
            ..Default::default()
        }
    }

    fn cards(ids: &[&str]) -> HashMap<String, u8> {
        ids.iter().map(|id| (id.to_string(), 4)).collect()
    }

    #[test]
    fn flags_banned_rotated_and_paired_cards() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let found = list().check("ST01-001", &cards(&["ST01-010", "ST01-014", "ST01-002", "ST01-003"]), &repo);
        let kinds: Vec<_> = found.iter().map(|v| (v.card_id.as_str(), v.kind.as_str())).collect();
        assert!(kinds.contains(&("ST01-010", "banned")), "{kinds:?}");
        assert!(kinds.contains(&("ST01-014", "rotated")), "{kinds:?}");
        assert!(kinds.contains(&("ST01-002", "pair")), "{kinds:?}");
    }

    #[test]
    fn one_half_of_a_pair_is_fine_and_a_clean_deck_has_no_violations() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        assert!(list().check("ST01-001", &cards(&["ST01-002", "ST05-001"]), &repo).is_empty());
        assert!(list().check("ST01-001", &cards(&["ST01-003"]), &repo).is_empty());
    }

    #[test]
    fn the_leader_is_checked_too() {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        let repo = CardRepository::new(&db);
        let found = list().check("ST01-010", &cards(&[]), &repo);
        assert_eq!(found.len(), 1);
    }
}
