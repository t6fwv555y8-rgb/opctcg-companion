//! What winning decks actually play, and how a given deck compares.
//!
//! The data is a set of tournament top-cut lists written by
//! `scripts/sync_meta.py`. Everything here is descriptive statistics over those
//! lists: it says how many winning lists run a card, never that a card "wins".

use crate::legality::{BanList, Violation};
use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Fewer lists than this and the numbers are too thin to advise on.
pub const MIN_LISTS_FOR_ADVICE: usize = 3;

/// Tournament ids are numbers on some sources and UUID strings on others.
fn id_as_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => Ok(String::new()),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaDeck {
    pub leader: String,
    #[serde(default)]
    pub archetype: String,
    #[serde(default)]
    pub placement: String,
    /// Normalised format, e.g. "OP17" or "OP17.5". Empty in older downloads.
    #[serde(default)]
    pub format: String,
    #[serde(default, deserialize_with = "id_as_string")]
    pub tournament_id: String,
    /// "Large Official Event" (in-person), "Unofficial Event" (online), or empty.
    #[serde(default)]
    pub event_type: String,
    pub cards: HashMap<String, u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaTournament {
    #[serde(default, deserialize_with = "id_as_string")]
    pub id: String,
    pub name: String,
    pub date: String,
    pub format: String,
    pub players: u32,
    #[serde(default)]
    pub event_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaData {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub tournaments: Vec<MetaTournament>,
    #[serde(default)]
    pub decks: Vec<MetaDeck>,
}

impl MetaData {
    /// Read `meta_decks.json`; `None` when it is absent or unreadable.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Lists for a Leader, in one format when given (lists with no format
    /// recorded count as the default format).
    pub fn lists_for(&self, leader: &str, format: Option<&str>) -> Vec<&MetaDeck> {
        self.decks
            .iter()
            .filter(|d| d.leader.eq_ignore_ascii_case(leader))
            .filter(|d| match format {
                Some(f) => d.format.eq_ignore_ascii_case(f),
                None => true,
            })
            .collect()
    }

    /// Formats in the data, most lists first.
    pub fn formats(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for deck in &self.decks {
            *counts.entry(deck.format.as_str()).or_default() += 1;
        }
        let mut out: Vec<(String, usize)> =
            counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }

    /// The format to compare against when none is chosen: the one with the
    /// most lists, so a rare ".5" side format never silently becomes the default.
    pub fn default_format(&self) -> Option<String> {
        self.formats().into_iter().next().map(|(f, _)| f)
    }

    /// Leaders in the data, most-played first.
    pub fn leaders(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for deck in &self.decks {
            *counts.entry(deck.leader.as_str()).or_default() += 1;
        }
        let mut out: Vec<(String, usize)> =
            counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }
}

/// One card worth mentioning, with the numbers behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardAdvice {
    pub card_id: String,
    pub name: String,
    /// Share of winning lists (0-100) that run at least one copy.
    pub in_lists_pct: u8,
    /// How many lists run it, out of `DeckComparison::lists`.
    pub in_lists: u32,
    /// Average copies among the lists that run it.
    pub avg_copies: f32,
    pub your_copies: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeckComparison {
    pub leader_id: String,
    pub leader_name: String,
    /// How many winning lists use this Leader.
    pub lists: usize,
    /// False when there are too few lists to say anything useful.
    pub enough_data: bool,
    /// Most common archetype labels for this Leader.
    pub archetypes: Vec<String>,
    /// How many of those lists came from in-person official events vs online/unofficial ones.
    pub official_lists: usize,
    pub unofficial_lists: usize,
    /// The event filter applied: "all", "official" or "online".
    pub events: String,
    /// Lists for this Leader and format before the event filter, to label the filter buttons.
    pub all_official_lists: usize,
    pub all_unofficial_lists: usize,
    /// True when a deck list was supplied; false means only `popular` is filled in.
    pub has_list: bool,
    /// What winners with this Leader play most, whether or not you have a list.
    pub popular: Vec<CardAdvice>,
    /// Cards most winners play that you don't.
    pub missing: Vec<CardAdvice>,
    /// Cards you play that almost no winner does.
    pub rarely_played: Vec<CardAdvice>,
    /// Cards you run fewer copies of than winners usually do.
    pub fewer_copies: Vec<CardAdvice>,
    /// Counter cards (cards with a Counter value) in your list versus winners' average.
    pub your_counter_cards: u32,
    pub avg_counter_cards: f32,
    /// The format these numbers are for, and every format in the data.
    pub format: String,
    pub formats: Vec<FormatCount>,
    /// Why your list is not legal in Standard (empty when it is, or when no ban list is loaded).
    pub violations: Vec<Violation>,
    /// The different builds of this Leader in the winning lists, largest first.
    pub builds: Vec<crate::variants::VariantInfo>,
    /// The build your list is closest to (only with a list and 2+ builds).
    pub your_build: Option<crate::variants::VariantInfo>,
    /// Date of the ban list used ("" when none was loaded).
    pub banlist_updated: String,
    /// Cards in your list that are not in the card database, so skipped in the totals.
    pub unknown_cards: u32,
    /// Where the data came from and how fresh it is.
    pub source: String,
    pub generated_at: String,
    /// Other Leaders that do have data, for when yours has none.
    pub available_leaders: Vec<LeaderCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatCount {
    pub format: String,
    pub lists: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderCount {
    pub leader_id: String,
    pub name: String,
    pub lists: usize,
}

fn counter_cards<'a>(
    cards: impl Iterator<Item = (&'a String, &'a u8)>,
    repo: &CardRepository<'_>,
    unknown: &mut u32,
) -> u32 {
    let mut total = 0u32;
    for (id, qty) in cards {
        // One lookup per iteration: each `get_by_id` takes the database lock.
        match repo.get_by_id(id) {
            Ok(def) if def.counter > 0 => total += u32::from(*qty),
            Ok(_) => {}
            Err(_) => *unknown += u32::from(*qty),
        }
    }
    total
}

/// Compare `your_cards` (card id -> copies, Leader excluded) with the winning
/// lists that share your Leader.
pub fn compare(
    meta: &MetaData,
    leader_id: &str,
    your_cards: &HashMap<String, u8>,
    repo: &CardRepository<'_>,
    format: Option<&str>,
    events: Option<&str>,
    banlist: Option<&BanList>,
) -> DeckComparison {
    let format = format
        .map(str::to_string)
        .or_else(|| meta.default_format())
        .unwrap_or_default();
    let format_filter = (!format.is_empty()).then_some(format.as_str());
    let all_lists = meta.lists_for(leader_id, format_filter);
    let is_official = |d: &MetaDeck| {
        d.event_type.contains("Official") && !d.event_type.contains("Unofficial")
    };
    let is_online = |d: &MetaDeck| d.event_type.contains("Unofficial");
    let events = match events {
        Some("official") => "official",
        Some("online") => "online",
        _ => "all",
    };
    let lists: Vec<&MetaDeck> = all_lists
        .iter()
        .copied()
        .filter(|d| match events {
            "official" => is_official(d),
            "online" => is_online(d),
            _ => true,
        })
        .collect();
    let n = lists.len();
    let leader_name = |id: &str| {
        repo.get_by_id(id)
            .map(|d| d.name)
            .unwrap_or_else(|_| id.to_string())
    };

    let mut out = DeckComparison {
        leader_id: leader_id.to_string(),
        leader_name: leader_name(leader_id),
        lists: n,
        enough_data: n >= MIN_LISTS_FOR_ADVICE,
        format: format.clone(),
        events: events.to_string(),
        all_official_lists: all_lists.iter().filter(|d| is_official(d)).count(),
        all_unofficial_lists: all_lists.iter().filter(|d| is_online(d)).count(),
        formats: meta
            .formats()
            .into_iter()
            .map(|(format, lists)| FormatCount { format, lists })
            .collect(),
        banlist_updated: banlist.map(|b| b.last_updated.clone()).unwrap_or_default(),
        source: meta.source.clone(),
        generated_at: meta.generated_at.clone(),
        ..Default::default()
    };

    out.available_leaders = meta
        .leaders()
        .into_iter()
        .take(12)
        .map(|(id, lists)| LeaderCount {
            name: leader_name(&id),
            leader_id: id,
            lists,
        })
        .collect();

    if n == 0 {
        return out;
    }

    out.official_lists = lists.iter().filter(|d| d.event_type.contains("Official")).count();
    out.unofficial_lists = lists.iter().filter(|d| d.event_type.contains("Unofficial")).count();

    // Most common archetype labels.
    let mut labels: HashMap<&str, usize> = HashMap::new();
    for deck in &lists {
        if !deck.archetype.is_empty() {
            *labels.entry(deck.archetype.as_str()).or_default() += 1;
        }
    }
    let mut labels: Vec<_> = labels.into_iter().collect();
    labels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    out.archetypes = labels.into_iter().take(3).map(|(l, _)| l.to_string()).collect();

    // Inclusion and copy counts per card.
    let mut seen_in: HashMap<&str, usize> = HashMap::new();
    let mut copies_total: HashMap<&str, u32> = HashMap::new();
    for deck in &lists {
        for (id, qty) in &deck.cards {
            *seen_in.entry(id.as_str()).or_default() += 1;
            *copies_total.entry(id.as_str()).or_default() += u32::from(*qty);
        }
    }
    let advice = |id: &str, yours: u8| -> CardAdvice {
        let seen = seen_in.get(id).copied().unwrap_or(0);
        let copies = copies_total.get(id).copied().unwrap_or(0);
        CardAdvice {
            card_id: id.to_string(),
            name: leader_name(id),
            in_lists_pct: ((seen * 100) / n) as u8,
            in_lists: seen as u32,
            avg_copies: if seen == 0 { 0.0 } else { copies as f32 / seen as f32 },
            your_copies: yours,
        }
    };

    out.has_list = !your_cards.is_empty();
    if out.enough_data {
        let clusters = crate::variants::cluster(&all_lists, repo);
        out.builds = clusters.iter().map(|c| c.info.clone()).collect();
        if out.has_list && clusters.len() > 1 {
            out.your_build = crate::variants::closest(&clusters, your_cards).map(|c| c.info.clone());
        }
    }
    if let (true, Some(banlist)) = (out.has_list, banlist) {
        out.violations = banlist.check(leader_id, your_cards, repo);
    }
    // Never suggest a card that is not legal in Standard.
    let allowed = |id: &str| banlist.map_or(true, |b| b.card_is_legal(id));

    if out.enough_data {
        let mut popular: Vec<CardAdvice> = seen_in
            .keys()
            .filter(|id| allowed(id))
            .map(|id| advice(id, 0))
            .collect();
        popular.sort_by(|a, b| {
            b.in_lists_pct
                .cmp(&a.in_lists_pct)
                .then(b.avg_copies.partial_cmp(&a.avg_copies).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.card_id.cmp(&b.card_id))
        });
        popular.truncate(15);
        out.popular = popular;
    }

    if out.enough_data && out.has_list {
        let mut missing: Vec<CardAdvice> = seen_in
            .iter()
            .filter(|(id, seen)| {
                **seen * 2 >= n && your_cards.get(**id).copied().unwrap_or(0) == 0 && allowed(id)
            })
            .map(|(id, _)| advice(id, 0))
            .collect();
        missing.sort_by(|a, b| {
            b.in_lists_pct
                .cmp(&a.in_lists_pct)
                .then(a.card_id.cmp(&b.card_id))
        });
        missing.truncate(8);
        out.missing = missing;

        let mut rare: Vec<CardAdvice> = your_cards
            .iter()
            .filter(|(id, qty)| {
                **qty > 0 && seen_in.get(id.as_str()).copied().unwrap_or(0) * 10 <= n
            })
            .map(|(id, qty)| advice(id, *qty))
            .collect();
        rare.sort_by(|a, b| b.your_copies.cmp(&a.your_copies).then(a.card_id.cmp(&b.card_id)));
        rare.truncate(8);
        out.rarely_played = rare;

        let mut fewer: Vec<CardAdvice> = your_cards
            .iter()
            .filter_map(|(id, qty)| {
                let a = advice(id, *qty);
                (a.in_lists_pct >= 50 && a.avg_copies - f32::from(*qty) >= 1.5).then_some(a)
            })
            .collect();
        fewer.sort_by(|a, b| {
            (b.avg_copies - f32::from(b.your_copies))
                .partial_cmp(&(a.avg_copies - f32::from(a.your_copies)))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.card_id.cmp(&b.card_id))
        });
        fewer.truncate(6);
        out.fewer_copies = fewer;
    }

    if !out.has_list {
        return out;
    }
    let mut unknown = 0u32;
    out.your_counter_cards = counter_cards(your_cards.iter(), repo, &mut unknown);
    out.unknown_cards = unknown;
    let mut ignored = 0u32;
    let total: u32 = lists
        .iter()
        .map(|deck| counter_cards(deck.cards.iter(), repo, &mut ignored))
        .sum();
    out.avg_counter_cards = total as f32 / n as f32;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_database::{AssetParser, Database};

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::seed_defaults(&db).unwrap();
        db
    }

    fn deck(leader: &str, cards: &[(&str, u8)]) -> MetaDeck {
        MetaDeck {
            leader: leader.into(),
            archetype: "Red Luffy".into(),
            cards: cards.iter().map(|(c, q)| (c.to_string(), *q)).collect(),
            ..Default::default()
        }
    }

    fn meta() -> MetaData {
        MetaData {
            source: "test".into(),
            decks: vec![
                deck("ST01-001", &[("ST01-002", 4), ("ST01-003", 4), ("ST01-010", 2)]),
                deck("ST01-001", &[("ST01-002", 4), ("ST01-003", 3)]),
                deck("ST01-001", &[("ST01-002", 4), ("ST01-003", 4), ("ST01-010", 2)]),
                deck("OP17-079", &[("ST01-002", 1)]),
            ],
            ..Default::default()
        }
    }

    fn yours(cards: &[(&str, u8)]) -> HashMap<String, u8> {
        cards.iter().map(|(c, q)| (c.to_string(), *q)).collect()
    }

    #[test]
    fn reads_numeric_and_string_tournament_ids_and_event_types() {
        let json = r#"{"decks":[
            {"leader":"ST01-001","tournament_id":123,"cards":{"ST01-002":4}},
            {"leader":"ST01-001","tournament_id":"abc-uuid","event_type":"Large Official Event","cards":{"ST01-002":4}},
            {"leader":"ST01-001","event_type":"Unofficial Event","cards":{"ST01-002":4}}],
          "tournaments":[{"id":"u-1","name":"x","date":"d","format":"OP17","players":9}]}"#;
        let meta: MetaData = serde_json::from_str(json).unwrap();
        assert_eq!(meta.decks[0].tournament_id, "123");
        assert_eq!(meta.decks[1].tournament_id, "abc-uuid");
        let db = db();
        let repo = CardRepository::new(&db);
        let cmp = compare(&meta, "ST01-001", &yours(&[]), &repo, None, None, None);
        assert_eq!((cmp.official_lists, cmp.unofficial_lists), (1, 1));
    }

    #[test]
    fn without_a_list_it_shows_what_winners_play_instead_of_calling_everything_missing() {
        let db = db();
        let repo = CardRepository::new(&db);
        let cmp = compare(&meta(), "ST01-001", &yours(&[]), &repo, None, None, None);
        assert!(!cmp.has_list);
        assert!(cmp.missing.is_empty() && cmp.rarely_played.is_empty());
        assert_eq!(cmp.popular.first().map(|c| c.card_id.as_str()), Some("ST01-002"));
    }

    #[test]
    fn the_event_filter_separates_in_person_from_online_and_counts_in_lists() {
        let mut data = meta();
        data.decks.push(deck("ST01-001", &[("ST01-002", 4), ("ST01-003", 4)]));
        for (i, deck) in data.decks.iter_mut().enumerate() {
            deck.event_type = if i == 0 { "Large Official Event" } else { "Unofficial Event" }.into();
        }
        let db = db();
        let repo = CardRepository::new(&db);
        let online = compare(&data, "ST01-001", &yours(&[]), &repo, None, Some("online"), None);
        assert_eq!((online.lists, online.events.as_str()), (3, "online"));
        assert_eq!((online.all_official_lists, online.all_unofficial_lists), (1, 3));
        let top = &online.popular[0];
        assert_eq!((top.card_id.as_str(), top.in_lists), ("ST01-002", 3), "counts, not just percent");
        let official = compare(&data, "ST01-001", &yours(&[]), &repo, None, Some("official"), None);
        assert_eq!(official.lists, 1);
    }

    #[test]
    fn formats_are_kept_apart_and_the_biggest_is_the_default() {
        let mut data = meta();
        for (i, deck) in data.decks.iter_mut().enumerate() {
            deck.format = if i == 2 { "OP17.5".into() } else { "OP17".into() };
        }
        assert_eq!(data.default_format().as_deref(), Some("OP17"));
        let db = db();
        let repo = CardRepository::new(&db);
        let main = compare(&data, "ST01-001", &yours(&[]), &repo, None, None, None);
        assert_eq!((main.format.as_str(), main.lists), ("OP17", 2));
        let side = compare(&data, "ST01-001", &yours(&[]), &repo, Some("OP17.5"), None, None);
        assert_eq!((side.format.as_str(), side.lists), ("OP17.5", 1));
        assert_eq!(side.formats.len(), 2);
    }

    #[test]
    fn illegal_cards_are_reported_for_your_deck_and_never_suggested() {
        let db = db();
        let repo = CardRepository::new(&db);
        let banlist = BanList {
            banned: ["ST01-003".to_string()].into(),
            last_updated: "2026-04-10".into(),
            ..Default::default()
        };
        let cmp = compare(
            &meta(),
            "ST01-001",
            &yours(&[("ST01-003", 4), ("ST01-010", 2)]),
            &repo,
            None,
            None,
            Some(&banlist),
        );
        assert!(cmp.violations.iter().any(|v| v.card_id == "ST01-003" && v.kind == "banned"));
        assert_eq!(cmp.banlist_updated, "2026-04-10");
        let popular = compare(&meta(), "ST01-001", &yours(&[]), &repo, None, None, Some(&banlist));
        assert!(popular.popular.iter().all(|c| c.card_id != "ST01-003"));
    }

    #[test]
    fn reports_what_most_winners_play_that_you_do_not() {
        let db = db();
        let repo = CardRepository::new(&db);
        let cmp = compare(&meta(), "ST01-001", &yours(&[("ST01-003", 4)]), &repo, None, None, None);
        assert_eq!(cmp.lists, 3);
        assert!(cmp.enough_data);
        let ids: Vec<_> = cmp.missing.iter().map(|c| c.card_id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"ST01-002"), "in 100% of lists: {ids:?}");
        assert_eq!(cmp.missing[0].in_lists_pct, 100);
    }

    #[test]
    fn flags_cards_almost_no_winner_runs_and_light_copy_counts() {
        let db = db();
        let repo = CardRepository::new(&db);
        let cmp = compare(
            &meta(),
            "ST01-001",
            &yours(&[("ST01-002", 1), ("ZZ99-001", 3)]),
            &repo,
            None,
            None,
            None,
        );
        assert!(cmp.rarely_played.iter().any(|c| c.card_id == "ZZ99-001"));
        let fewer = cmp.fewer_copies.iter().find(|c| c.card_id == "ST01-002").unwrap();
        assert_eq!(fewer.your_copies, 1);
        assert!(fewer.avg_copies >= 4.0);
        assert_eq!(cmp.unknown_cards, 3, "unknown card is counted, not guessed");
    }

    #[test]
    fn too_few_lists_gives_no_advice_and_says_so() {
        let db = db();
        let repo = CardRepository::new(&db);
        let cmp = compare(&meta(), "OP17-079", &yours(&[("ST01-003", 4)]), &repo, None, None, None);
        assert_eq!(cmp.lists, 1);
        assert!(!cmp.enough_data);
        assert!(cmp.missing.is_empty() && cmp.rarely_played.is_empty());
    }

    #[test]
    fn an_unseen_leader_lists_which_leaders_have_data() {
        let db = db();
        let repo = CardRepository::new(&db);
        let cmp = compare(&meta(), "OP05-001", &yours(&[]), &repo, None, None, None);
        assert_eq!(cmp.lists, 0);
        assert_eq!(cmp.available_leaders[0].leader_id, "ST01-001");
    }
}
