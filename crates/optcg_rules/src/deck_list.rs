use optcg_database::CardRepository;
use serde::{Deserialize, Serialize};

/// One unique card line from a pasted deck list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeckListEntry {
    pub card_id: String,
    pub name: String,
    pub quantity: u8,
    pub cost: u32,
    pub card_type: String,
    pub color: String,
    pub rush: bool,
    pub blocker: bool,
    pub counter: i32,
}

/// Parsed user-pasted deck list (exact combination).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PastedDeckList {
    pub raw: String,
    pub name: Option<String>,
    pub leader_id: Option<String>,
    pub entries: Vec<DeckListEntry>,
    pub warnings: Vec<String>,
    pub total_cards: u32,
}

impl PastedDeckList {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Parse common OPTCG deck-list paste formats and resolve via card DB when possible.
    ///
    /// Supported lines (examples):
    /// - `4x ST01-002`
    /// - `4 ST01-002 Usopp`
    /// - `ST01-012 x4`
    /// - `Deck: Red Luffy Aggro` / `Leader: ST01-001`
    /// - bare `ST01-003` (qty 1)
    pub fn parse(raw: &str, repo: &CardRepository<'_>) -> Self {
        let mut list = PastedDeckList {
            raw: raw.to_string(),
            ..Default::default()
        };

        // Aggregate quantities by card id (case-insensitive key).
        let mut qty_by_id: Vec<(String, u8, Option<String>)> = Vec::new();

        for (line_no, raw_line) in raw.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }

            if let Some(name) = strip_prefix_ci(line, "deck:") {
                list.name = Some(name.trim().to_string());
                continue;
            }
            if let Some(name) = strip_prefix_ci(line, "deck name:") {
                list.name = Some(name.trim().to_string());
                continue;
            }
            if let Some(leader) = strip_prefix_ci(line, "leader:") {
                let leader = leader.trim();
                if let Some(id) = extract_card_id(leader) {
                    list.leader_id = Some(id);
                } else if !leader.is_empty() {
                    // Try name lookup
                    if let Ok(found) = repo.search_by_name(leader, 1) {
                        if let Some(c) = found.first() {
                            list.leader_id = Some(c.card_id.clone());
                        } else {
                            list.warnings.push(format!(
                                "line {}: could not resolve leader '{leader}'",
                                line_no + 1
                            ));
                        }
                    }
                }
                continue;
            }

            match parse_card_line(line) {
                Some((qty, id_or_name, display_hint)) => {
                    let resolved_id = if looks_like_card_id(&id_or_name) {
                        normalize_card_id(&id_or_name)
                    } else if let Ok(found) = repo.search_by_name(&id_or_name, 1) {
                        found
                            .first()
                            .map(|c| c.card_id.clone())
                            .unwrap_or_else(|| id_or_name.clone())
                    } else {
                        id_or_name.clone()
                    };

                    if let Some((_, q, _)) = qty_by_id
                        .iter_mut()
                        .find(|(id, _, _)| id.eq_ignore_ascii_case(&resolved_id))
                    {
                        *q = q.saturating_add(qty);
                    } else {
                        qty_by_id.push((resolved_id, qty, display_hint));
                    }
                }
                None => {
                    list.warnings
                        .push(format!("line {}: could not parse '{line}'", line_no + 1));
                }
            }
        }

        for (card_id, quantity, hint) in qty_by_id {
            let entry = match repo.get_by_id(&card_id) {
                Ok(def) => DeckListEntry {
                    card_id: def.card_id,
                    name: def.name,
                    quantity,
                    cost: def.cost,
                    card_type: format!("{:?}", def.card_type).to_lowercase(),
                    color: def.color,
                    rush: def.keywords.rush,
                    blocker: def.keywords.blocker,
                    counter: def.counter,
                },
                Err(_) => DeckListEntry {
                    card_id: card_id.clone(),
                    name: hint.unwrap_or_else(|| card_id.clone()),
                    quantity,
                    cost: 0,
                    card_type: "unknown".into(),
                    color: String::new(),
                    rush: false,
                    blocker: false,
                    counter: 0,
                },
            };

            if entry.card_type == "leader" && list.leader_id.is_none() {
                list.leader_id = Some(entry.card_id.clone());
            }
            list.total_cards = list.total_cards.saturating_add(u32::from(entry.quantity));
            list.entries.push(entry);
        }

        list.entries.sort_by(|a, b| {
            a.cost
                .cmp(&b.cost)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.card_id.cmp(&b.card_id))
        });

        list
    }
}

fn strip_prefix_ci<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&line[prefix.len()..])
    } else {
        None
    }
}

fn looks_like_card_id(s: &str) -> bool {
    let s = s.trim();
    let re_ok = s.len() >= 5
        && s.chars().any(|c| c.is_ascii_digit())
        && s.contains('-');
    re_ok
}

fn normalize_card_id(raw: &str) -> String {
    raw.trim()
        .replace('_', "-")
        .trim_end_matches(".webp")
        .trim_end_matches(".WEBP")
        .to_ascii_uppercase()
}

fn extract_card_id(text: &str) -> Option<String> {
    // Match patterns like ST01-001, OP01-016, EB01-001, P-001
    let upper = text.to_ascii_uppercase();
    for token in upper.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')) {
        let t = token.replace('_', "-");
        if looks_like_card_id(&t) {
            return Some(normalize_card_id(&t));
        }
    }
    None
}

/// Returns (quantity, id_or_name, optional display hint).
fn parse_card_line(line: &str) -> Option<(u8, String, Option<String>)> {
    let line = line.trim();
    // Strip trailing comments
    let line = line.split('#').next()?.trim();
    if line.is_empty() {
        return None;
    }

    // 4x ST01-002  /  4xST01-002  /  4 ST01-002 Name
    if let Some(caps) = match_qty_prefix(line) {
        return Some(caps);
    }

    // ST01-002 x4  /  ST01-002 x 4
    if let Some(caps) = match_qty_suffix(line) {
        return Some(caps);
    }

    // Bare card id (optionally with name after)
    if let Some(id) = extract_card_id(line) {
        let hint = line
            .split_whitespace()
            .filter(|t| !looks_like_card_id(t) && !t.eq_ignore_ascii_case("x"))
            .collect::<Vec<_>>()
            .join(" ");
        return Some((
            1,
            id,
            if hint.is_empty() { None } else { Some(hint) },
        ));
    }

    // Name only → qty 1
    if line.chars().any(|c| c.is_alphabetic()) {
        return Some((1, line.to_string(), Some(line.to_string())));
    }

    None
}

fn match_qty_prefix(line: &str) -> Option<(u8, String, Option<String>)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let qty: u8 = line[..i].parse().ok()?;
    let mut rest = line[i..].trim_start();
    if rest.to_ascii_lowercase().starts_with('x') {
        rest = rest[1..].trim_start();
    }
    if rest.is_empty() {
        return None;
    }
    if let Some(id) = extract_card_id(rest) {
        let hint = rest
            .split_whitespace()
            .filter(|t| !looks_like_card_id(t))
            .collect::<Vec<_>>()
            .join(" ");
        return Some((
            qty.max(1),
            id,
            if hint.is_empty() { None } else { Some(hint) },
        ));
    }
    // quantity + name
    Some((qty.max(1), rest.to_string(), Some(rest.to_string())))
}

fn match_qty_suffix(line: &str) -> Option<(u8, String, Option<String>)> {
    // ... x4 or ... x 4 at end
    let lower = line.to_ascii_lowercase();
    let idx = lower.rfind('x')?;
    let after = line[idx + 1..].trim();
    if after.is_empty() || !after.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let qty: u8 = after.parse().ok()?;
    let before = line[..idx].trim();
    if before.is_empty() {
        return None;
    }
    if let Some(id) = extract_card_id(before) {
        return Some((qty.max(1), id, None));
    }
    Some((qty.max(1), before.to_string(), Some(before.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use optcg_database::{AssetParser, Database, JsonCardEntry};

    fn db_with_st01() -> Database {
        let db = Database::open_in_memory().unwrap();
        AssetParser::sync_to_database(
            &db,
            &[
                JsonCardEntry {
                    card_id: "ST01-001".into(),
                    name: "Monkey.D.Luffy".into(),
                    card_type: "leader".into(),
                    cost: 0,
                    power: 5000,
                    counter: 0,
                    color: "Red".into(),
                    rush: false,
                    blocker: false,
                    double_attack: false,
                    banish: false,
                    text: String::new(),
                },
                JsonCardEntry {
                    card_id: "ST01-002".into(),
                    name: "Usopp".into(),
                    card_type: "character".into(),
                    cost: 2,
                    power: 4000,
                    counter: 1000,
                    color: "Red".into(),
                    rush: false,
                    blocker: false,
                    double_attack: false,
                    banish: false,
                    text: String::new(),
                },
                JsonCardEntry {
                    card_id: "ST01-012".into(),
                    name: "Sanji".into(),
                    card_type: "character".into(),
                    cost: 5,
                    power: 6000,
                    counter: 0,
                    color: "Red".into(),
                    rush: true,
                    blocker: false,
                    double_attack: false,
                    banish: false,
                    text: String::new(),
                },
            ],
        )
        .unwrap();
        db
    }

    #[test]
    fn parses_qty_prefix_and_leader() {
        let db = db_with_st01();
        let repo = CardRepository::new(&db);
        let raw = r#"
Deck: Red Luffy Aggro
Leader: ST01-001
4x ST01-002
2 ST01-012 Sanji
"#;
        let list = PastedDeckList::parse(raw, &repo);
        assert_eq!(list.name.as_deref(), Some("Red Luffy Aggro"));
        assert_eq!(list.leader_id.as_deref(), Some("ST01-001"));
        assert_eq!(list.total_cards, 6);
        assert!(list.entries.iter().any(|e| e.card_id == "ST01-002" && e.quantity == 4));
        assert!(list.entries.iter().any(|e| e.card_id == "ST01-012" && e.rush));
    }

    #[test]
    fn parses_qty_suffix() {
        let db = db_with_st01();
        let repo = CardRepository::new(&db);
        let list = PastedDeckList::parse("ST01-002 x4", &repo);
        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.entries[0].quantity, 4);
    }
}
