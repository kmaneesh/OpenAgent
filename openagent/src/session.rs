/// Session store — writes conversation turns to openagent.db.
///
/// Mirrors the schema written by the Python `SqliteSessionBackend`:
/// - `session_metadata(session_key TEXT PK, hidden_at, browser_session_id, browser_last_active)`
/// - `turns(id, session_key, role, content, tool_call_id, tool_name, ts)`
///
/// Both tables are created with `IF NOT EXISTS` so this is a no-op when the
/// DB already has the tables from the Python side.
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct SessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SessionStore {
    pub fn open(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("open session db at {db_path}"))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;

             CREATE TABLE IF NOT EXISTS session_metadata (
                 session_key          TEXT PRIMARY KEY,
                 hidden_at            TEXT,
                 browser_session_id   TEXT,
                 browser_last_active  TEXT
             );

             CREATE TABLE IF NOT EXISTS turns (
                 id           INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_key  TEXT NOT NULL,
                 role         TEXT NOT NULL,
                 content      TEXT NOT NULL,
                 tool_call_id TEXT NOT NULL DEFAULT '',
                 tool_name    TEXT NOT NULL DEFAULT '',
                 ts           TEXT NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_turns_session ON turns (session_key, id);",
        )
        .context("session db migration")?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Ensure a `session_metadata` row exists for this session key.
    pub fn upsert_session(&self, session_key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO session_metadata (session_key) VALUES (?1)",
            params![session_key],
        )
        .context("upsert session_metadata")?;
        Ok(())
    }

    /// Append a turn (user or assistant) to the turns table.
    pub fn append_turn(&self, session_key: &str, role: &str, content: &str) -> Result<()> {
        let ts = now_iso();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO turns (session_key, role, content, ts) VALUES (?1, ?2, ?3, ?4)",
            params![session_key, role, content, ts],
        )
        .context("insert turn")?;
        Ok(())
    }
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hr = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}T{hr:02}:{min:02}:{sec:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    loop {
        let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        let dy = if leap { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let months = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1u64;
    for dm in &months {
        if days < *dm {
            break;
        }
        days -= dm;
        m += 1;
    }
    (y, m, days + 1)
}
