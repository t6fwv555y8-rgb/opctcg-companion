//! Grouping winning lists into builds, and weighting lists by how recent and
//! how big the event was.
//!
//! A Leader is rarely played one way. Two lists sharing a Leader can have only
//! half their cards in common. Grouping lists by card overlap separates those
//! builds so a player can be compared with their own, and so a generated deck
//! is one coherent build rather than an average of several.

use crate::meta::{MetaData, MetaDeck};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Lists whose card sets overlap at least this much (Jaccard) are one build.
const SAME_BUILD: f32 = 0.55;
/// A list from this many days before the newest event counts half as much.
const HALF_LIFE_DAYS: f32 = 21.0;

/// Days since 1970-01-01 for "YYYY-MM-DD"; `None` if it does not parse.
pub fn day_number(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.get(..2)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's civil-from-days, inverted.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

impl MetaData {
    /// How much each of `lists` should count: newer events and bigger events
    /// count more. Weights are relative to the newest event in the data, so
    /// the result does not depend on today's date.
    pub fn weights_for(&self, lists: &[&MetaDeck]) -> Vec<f32> {
        let events: HashMap<&str, (Option<i64>, u32)> = self
            .tournaments
            .iter()
            .map(|t| (t.id.as_str(), (day_number(&t.date), t.players)))
            .collect();
        let newest = events.values().filter_map(|(d, _)| *d).max();
        lists
            .iter()
            .map(|deck| {
                let Some((Some(day), players)) = events.get(deck.tournament_id.as_str()).copied()
                else {
                    return 0.5; // unknown event: neither favoured nor ignored
                };
                let age = newest.map_or(0, |n| (n - day).max(0)) as f32;
                let recency = 0.5f32.powf(age / HALF_LIFE_DAYS);
                let size = ((players.max(8) as f32).ln() / 1024f32.ln()).clamp(0.3, 1.0);
                recency * size
            })
            .collect()
    }
}

/// One build of a Leader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantInfo {
    pub id: usize,
    /// Named after the cards that set it apart, e.g. "Saul + Loki build".
    pub label: String,
    pub lists: usize,
    pub share_pct: u8,
    /// Cards this build runs far more than the others.
    pub signature: Vec<String>,
}

/// A build together with the lists in it.
#[derive(Debug, Clone)]
pub struct Cluster<'a> {
    pub info: VariantInfo,
    pub members: Vec<&'a MetaDeck>,
}

fn jaccard(a: &HashSet<&str>, b: &HashSet<&str>) -> f32 {
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Group lists into builds by card overlap (average-link, deterministic).
pub fn cluster<'a>(lists: &[&'a MetaDeck], repo: &CardRepository<'_>) -> Vec<Cluster<'a>> {
    let n = lists.len();
    if n == 0 {
        return Vec::new();
    }
    let sets: Vec<HashSet<&str>> = lists
        .iter()
        .map(|d| d.cards.keys().map(String::as_str).collect())
        .collect();

    let mut groups: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    loop {
        let mut best: Option<(usize, usize, f32)> = None;
        for i in 0..groups.len() {
            for j in (i + 1)..groups.len() {
                let mut total = 0.0;
                for &a in &groups[i] {
                    for &b in &groups[j] {
                        total += jaccard(&sets[a], &sets[b]);
                    }
                }
                let avg = total / (groups[i].len() * groups[j].len()) as f32;
                if avg >= SAME_BUILD && best.map_or(true, |(_, _, s)| avg > s) {
                    best = Some((i, j, avg));
                }
            }
        }
        let Some((i, j, _)) = best else { break };
        let merged = groups.remove(j);
        groups[i].extend(merged);
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));

    let name = |id: &str| repo.get_by_id(id).map(|d| d.name).unwrap_or_else(|_| id.to_string());
    groups
        .iter()
        .enumerate()
        .map(|(id, members)| {
            let inside = members.len() as f32;
            let outside = (n - members.len()) as f32;
            let mut signature: Vec<(String, f32)> = Vec::new();
            let mut counts: HashMap<&str, u32> = HashMap::new();
            for &m in members {
                for card in &sets[m] {
                    *counts.entry(card).or_default() += 1;
                }
            }
            for (card, count) in counts {
                let inc_in = count as f32 / inside;
                let count_out = (0..n)
                    .filter(|i| !members.contains(i) && sets[*i].contains(card))
                    .count() as f32;
                let inc_out = if outside == 0.0 { 0.0 } else { count_out / outside };
                if inc_in >= 0.6 && inc_out <= 0.3 && outside > 0.0 {
                    signature.push((card.to_string(), inc_in - inc_out));
                }
            }
            signature.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
            let signature: Vec<String> = signature.into_iter().take(3).map(|(id, _)| name(&id)).collect();
            let label = match (groups.len(), signature.as_slice()) {
                (1, _) => "All winning lists".to_string(),
                (_, []) => format!("Build {}", id + 1),
                (_, [one]) => format!("{one} build"),
                (_, [a, b, ..]) => format!("{a} + {b} build"),
            };
            Cluster {
                info: VariantInfo {
                    id,
                    label,
                    lists: members.len(),
                    share_pct: ((members.len() * 100) / n) as u8,
                    signature,
                },
                members: members.iter().map(|&i| lists[i]).collect(),
            }
        })
        .collect()
}

/// Which build your list is closest to (average overlap with its lists).
pub fn closest<'a, 'b>(
    clusters: &'b [Cluster<'a>],
    your_cards: &HashMap<String, u8>,
) -> Option<&'b Cluster<'a>> {
    let mine: HashSet<&str> = your_cards.keys().map(String::as_str).collect();
    clusters
        .iter()
        .map(|c| {
            let score: f32 = c
                .members
                .iter()
                .map(|d| jaccard(&mine, &d.cards.keys().map(String::as_str).collect()))
                .sum::<f32>()
                / c.members.len().max(1) as f32;
            (c, score)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::MetaTournament;
    use optcg_database::{AssetParser, Database};

    fn deck(tournament: &str, ids: &[&str]) -> MetaDeck {
        MetaDeck {
            leader: "ST01-001".into(),
            tournament_id: tournament.into(),
            cards: ids.iter().map(|i| (i.to_string(), 4u8)).collect(),
            ..Default::default()
        }
    }

    fn repo_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        db
    }

    #[test]
    fn day_numbers_count_days_between_dates() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(day_number("2026-09-29").unwrap() - day_number("2026-09-08").unwrap(), 21);
        assert_eq!(day_number("not a date"), None);
    }

    #[test]
    fn newer_and_bigger_events_weigh_more() {
        let meta = MetaData {
            tournaments: vec![
                MetaTournament { id: "new".into(), date: "2026-09-29".into(), players: 512, ..Default::default() },
                MetaTournament { id: "old".into(), date: "2026-08-18".into(), players: 512, ..Default::default() },
                MetaTournament { id: "small".into(), date: "2026-09-29".into(), players: 16, ..Default::default() },
            ],
            ..Default::default()
        };
        let (a, b, c) = (deck("new", &["A"]), deck("old", &["A"]), deck("small", &["A"]));
        let w = meta.weights_for(&[&a, &b, &c]);
        assert!(w[0] > w[1] * 2.0, "six weeks old counts far less: {w:?}");
        assert!(w[0] > w[2], "a 16-player event counts less than a 512-player one: {w:?}");
        let unknown = deck("???", &["A"]);
        assert_eq!(meta.weights_for(&[&unknown]), vec![0.5]);
    }

    #[test]
    fn lists_with_different_cards_split_into_builds_named_by_what_sets_them_apart() {
        let db = repo_db();
        let repo = CardRepository::new(&db);
        let shared = ["S1", "S2", "S3", "S4", "S5", "S6"];
        let with = |extra: &[&'static str]| -> Vec<&str> { shared.iter().copied().chain(extra.iter().copied()).collect() };
        let lists = vec![
            deck("t", &with(&["ST01-002", "ST01-003", "A3", "A4"])),
            deck("t", &with(&["ST01-002", "ST01-003", "A3", "A5"])),
            deck("t", &with(&["ST01-002", "ST01-003", "A4", "A5"])),
            deck("t", &["B1", "B2", "B3", "B4", "B5", "B6", "B7", "S1"]),
            deck("t", &["B1", "B2", "B3", "B4", "B5", "B6", "B8", "S1"]),
        ];
        let refs: Vec<&MetaDeck> = lists.iter().collect();
        let clusters = cluster(&refs, &repo);
        assert_eq!(clusters.len(), 2, "{:?}", clusters.iter().map(|c| &c.info).collect::<Vec<_>>());
        assert_eq!((clusters[0].info.lists, clusters[1].info.lists), (3, 2));
        assert_eq!(clusters[0].info.share_pct, 60);
        assert!(clusters[0].info.label.contains("build"));
        let you: HashMap<String, u8> = ["B1", "B2", "B3", "B4"].iter().map(|c| (c.to_string(), 4)).collect();
        assert_eq!(closest(&clusters, &you).unwrap().info.id, 1, "your list is the smaller build");
    }

    #[test]
    fn one_coherent_group_is_not_split_or_given_a_made_up_name() {
        let db = repo_db();
        let repo = CardRepository::new(&db);
        let lists = vec![deck("t", &["A", "B", "C", "D"]), deck("t", &["A", "B", "C", "E"])];
        let refs: Vec<&MetaDeck> = lists.iter().collect();
        let clusters = cluster(&refs, &repo);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].info.label, "All winning lists");
    }
}
