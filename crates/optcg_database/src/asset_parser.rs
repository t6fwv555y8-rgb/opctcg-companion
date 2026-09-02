use crate::error::DbResult;
use crate::schema::Database;
use optcg_core::{CardDefinition, CardType, Keywords};
use serde::Deserialize;
use tracing::{info, warn};

/// JSON card entry from bulk asset payload.
#[derive(Debug, Deserialize)]
pub struct JsonCardEntry {
    pub card_id: String,
    pub name: String,
    #[serde(default)]
    pub card_type: String,
    #[serde(default)]
    pub cost: u32,
    #[serde(default)]
    pub power: u32,
    #[serde(default)]
    pub counter: i32,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub rush: bool,
    #[serde(default)]
    pub blocker: bool,
    #[serde(default)]
    pub double_attack: bool,
    #[serde(default)]
    pub banish: bool,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Deserialize)]
struct BulkPayload {
    #[serde(default)]
    cards: Vec<JsonCardEntry>,
}

/// Parses bulk JSON card data and synchronizes into SQLite transactionally.
pub struct AssetParser;

impl AssetParser {
    pub fn parse_json_str(json: &str) -> DbResult<Vec<JsonCardEntry>> {
        let payload: BulkPayload = serde_json::from_str(json)?;
        if !payload.cards.is_empty() {
            return Ok(payload.cards);
        }
        let arr: Vec<JsonCardEntry> = serde_json::from_str(json)?;
        Ok(arr)
    }

    pub fn sync_to_database(db: &Database, entries: &[JsonCardEntry]) -> DbResult<usize> {
        let conn = db.connection();
        let tx = conn.unchecked_transaction()?;
        let mut synced = 0usize;

        for entry in entries {
            tx.execute(
                r#"
                INSERT INTO cards (
                    card_id, name, card_type, cost, power, counter, color,
                    rush, blocker, double_attack, banish, text, updated_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,datetime('now'))
                ON CONFLICT(card_id) DO UPDATE SET
                    name=excluded.name,
                    card_type=excluded.card_type,
                    cost=excluded.cost,
                    power=excluded.power,
                    counter=excluded.counter,
                    color=excluded.color,
                    rush=excluded.rush,
                    blocker=excluded.blocker,
                    double_attack=excluded.double_attack,
                    banish=excluded.banish,
                    text=excluded.text,
                    updated_at=datetime('now')
                "#,
                rusqlite::params![
                    entry.card_id,
                    entry.name,
                    entry.card_type,
                    entry.cost,
                    entry.power,
                    entry.counter,
                    entry.color,
                    entry.rush as i32,
                    entry.blocker as i32,
                    entry.double_attack as i32,
                    entry.banish as i32,
                    entry.text,
                ],
            )?;
            synced += 1;
        }

        tx.commit()?;
        info!(synced, "card assets synchronized");
        Ok(synced)
    }

    pub fn load_file_and_sync(db: &Database, path: &str) -> DbResult<usize> {
        let content = std::fs::read_to_string(path)?;
        let entries = Self::parse_json_str(&content)?;
        Self::sync_to_database(db, &entries)
    }

    pub fn to_definition(entry: &JsonCardEntry) -> CardDefinition {
        let card_type = parse_card_type(&entry.card_type);
        CardDefinition {
            card_id: entry.card_id.clone(),
            name: entry.name.clone(),
            card_type,
            cost: entry.cost,
            power: entry.power,
            counter: entry.counter,
            color: entry.color.clone(),
            attributes: optcg_core::CardAttributes {
                color: entry.color.clone(),
                card_type,
            },
            keywords: Keywords {
                rush: entry.rush,
                blocker: entry.blocker,
                double_attack: entry.double_attack,
                banish: entry.banish,
                counter: entry.counter,
            },
            rules_text: entry.text.clone(),
        }
    }

    pub fn seed_defaults(db: &Database) -> DbResult<usize> {
        let defaults = include_str!("../assets/default_cards.json");
        match Self::parse_json_str(defaults) {
            Ok(entries) => Self::sync_to_database(db, &entries),
            Err(e) => {
                warn!(error = %e, "failed to seed defaults");
                Ok(0)
            }
        }
    }
}

fn parse_card_type(s: &str) -> CardType {
    match s.to_ascii_lowercase().as_str() {
        "leader" => CardType::Leader,
        "event" => CardType::Event,
        "stage" => CardType::Stage,
        _ => CardType::Character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_cards_transactionally() {
        let db = Database::open_in_memory().unwrap();
        let entries = vec![JsonCardEntry {
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
        }];
        let n = AssetParser::sync_to_database(&db, &entries).unwrap();
        assert_eq!(n, 1);
        assert_eq!(db.card_count().unwrap(), 1);
    }
}
