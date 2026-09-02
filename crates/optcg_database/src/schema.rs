use crate::error::DbResult;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use tracing::info;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS cards (
    card_id     TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    card_type   TEXT NOT NULL DEFAULT 'character',
    cost        INTEGER NOT NULL DEFAULT 0,
    power       INTEGER NOT NULL DEFAULT 0,
    counter     INTEGER NOT NULL DEFAULT 0,
    color       TEXT NOT NULL DEFAULT '',
    rush        INTEGER NOT NULL DEFAULT 0,
    blocker     INTEGER NOT NULL DEFAULT 0,
    double_attack INTEGER NOT NULL DEFAULT 0,
    banish      INTEGER NOT NULL DEFAULT 0,
    text        TEXT NOT NULL DEFAULT '',
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_cards_name ON cards(name);
CREATE INDEX IF NOT EXISTS idx_cards_type ON cards(card_type);
CREATE INDEX IF NOT EXISTS idx_cards_color ON cards(color);
"#;

/// SQLite database wrapper with schema initialization.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &str) -> DbResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA_SQL)?;
        info!(path, "database opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn connection(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    pub fn card_count(&self) -> DbResult<i64> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM cards", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn upsert_card(
        &self,
        card_id: &str,
        name: &str,
        card_type: &str,
        cost: u32,
        power: u32,
        counter: i32,
        color: &str,
        rush: bool,
        blocker: bool,
        double_attack: bool,
        banish: bool,
        text: &str,
    ) -> DbResult<()> {
        let conn = self.conn.lock();
        conn.execute(
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
            params![
                card_id,
                name,
                card_type,
                cost,
                power,
                counter,
                color,
                rush as i32,
                blocker as i32,
                double_attack as i32,
                banish as i32,
                text,
            ],
        )?;
        Ok(())
    }
}
