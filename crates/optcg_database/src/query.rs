use crate::error::{DatabaseError, DbResult};
use crate::schema::Database;
use optcg_core::{CardDefinition, CardType, Keywords};
use rusqlite::params;

/// Query layer for card data lookups.
pub struct CardRepository<'a> {
    db: &'a Database,
}

impl<'a> CardRepository<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn get_by_id(&self, card_id: &str) -> DbResult<CardDefinition> {
        let conn = self.db.connection();
        conn.query_row(
                r#"
                SELECT card_id, name, card_type, cost, power, counter, color,
                       rush, blocker, double_attack, banish, text
                FROM cards WHERE card_id = ?1
                "#,
                params![card_id],
                |row| {
                    Ok(CardDefinition {
                        card_id: row.get(0)?,
                        name: row.get(1)?,
                        card_type: parse_card_type(row.get::<_, String>(2)?),
                        cost: row.get::<_, i64>(3)? as u32,
                        power: row.get::<_, i64>(4)? as u32,
                        counter: row.get(5)?,
                        color: row.get(6)?,
                        keywords: Keywords {
                            rush: row.get::<_, i64>(7)? != 0,
                            blocker: row.get::<_, i64>(8)? != 0,
                            double_attack: row.get::<_, i64>(9)? != 0,
                            banish: row.get::<_, i64>(10)? != 0,
                            counter: row.get(5)?,
                        },
                        text: row.get(11)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DatabaseError::CardNotFound(card_id.to_string())
                }
                other => DatabaseError::Sqlite(other),
            })
    }

    pub fn search_by_name(&self, query: &str, limit: usize) -> DbResult<Vec<CardDefinition>> {
        let pattern = format!("%{query}%");
        let conn = self.db.connection();
        let mut stmt = conn.prepare(
            r#"
            SELECT card_id, name, card_type, cost, power, counter, color,
                   rush, blocker, double_attack, banish, text
            FROM cards WHERE name LIKE ?1 LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(CardDefinition {
                card_id: row.get(0)?,
                name: row.get(1)?,
                card_type: parse_card_type(row.get::<_, String>(2)?),
                cost: row.get::<_, i64>(3)? as u32,
                power: row.get::<_, i64>(4)? as u32,
                counter: row.get(5)?,
                color: row.get(6)?,
                keywords: Keywords {
                    rush: row.get::<_, i64>(7)? != 0,
                    blocker: row.get::<_, i64>(8)? != 0,
                    double_attack: row.get::<_, i64>(9)? != 0,
                    banish: row.get::<_, i64>(10)? != 0,
                    counter: row.get(5)?,
                },
                text: row.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DatabaseError::from)
    }

    pub fn all_characters(&self) -> DbResult<Vec<CardDefinition>> {
        let conn = self.db.connection();
        let mut stmt = conn.prepare(
            r#"
            SELECT card_id, name, card_type, cost, power, counter, color,
                   rush, blocker, double_attack, banish, text
            FROM cards WHERE card_type = 'character'
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CardDefinition {
                card_id: row.get(0)?,
                name: row.get(1)?,
                card_type: parse_card_type(row.get::<_, String>(2)?),
                cost: row.get::<_, i64>(3)? as u32,
                power: row.get::<_, i64>(4)? as u32,
                counter: row.get(5)?,
                color: row.get(6)?,
                keywords: Keywords {
                    rush: row.get::<_, i64>(7)? != 0,
                    blocker: row.get::<_, i64>(8)? != 0,
                    double_attack: row.get::<_, i64>(9)? != 0,
                    banish: row.get::<_, i64>(10)? != 0,
                    counter: row.get(5)?,
                },
                text: row.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DatabaseError::from)
    }
}

fn parse_card_type(s: String) -> CardType {
    match s.to_ascii_lowercase().as_str() {
        "leader" => CardType::Leader,
        "event" => CardType::Event,
        "stage" => CardType::Stage,
        _ => CardType::Character,
    }
}
