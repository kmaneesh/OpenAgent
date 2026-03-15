use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single whitelist entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub platform: String,
    pub channel_id: String,
    pub note: Option<String>,
    pub added_at: i64,
}

/// Open (or create) the guard SQLite database at `db_path` and run migrations.
pub fn open(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("open guard db at {db_path}"))?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;

         CREATE TABLE IF NOT EXISTS whitelist (
             id         INTEGER PRIMARY KEY AUTOINCREMENT,
             platform   TEXT    NOT NULL,
             channel_id TEXT    NOT NULL,
             note       TEXT,
             added_at   INTEGER NOT NULL,
             UNIQUE(platform, channel_id)
         );

         -- tracks senders seen but not yet whitelisted (for admin review)
         CREATE TABLE IF NOT EXISTS seen_senders (
             platform   TEXT    NOT NULL,
             channel_id TEXT    NOT NULL,
             first_seen INTEGER NOT NULL,
             last_seen  INTEGER NOT NULL,
             hit_count  INTEGER NOT NULL DEFAULT 1,
             PRIMARY KEY (platform, channel_id)
         );",
    )
    .context("guard db migration")?;

    Ok(conn)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Returns `true` if `(platform, channel_id)` is in the whitelist.
pub fn check(conn: &Connection, platform: &str, channel_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM whitelist WHERE platform = ?1 AND channel_id = ?2",
        params![platform, channel_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Add a sender to the whitelist. Idempotent — updates `note` if the entry exists.
pub fn add(conn: &Connection, platform: &str, channel_id: &str, note: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO whitelist (platform, channel_id, note, added_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(platform, channel_id) DO UPDATE SET note = excluded.note",
        params![platform, channel_id, note, now_ms()],
    )?;
    Ok(())
}

/// Remove a sender from the whitelist. Returns `true` if a row was deleted.
pub fn remove(conn: &Connection, platform: &str, channel_id: &str) -> Result<bool> {
    let rows = conn.execute(
        "DELETE FROM whitelist WHERE platform = ?1 AND channel_id = ?2",
        params![platform, channel_id],
    )?;
    Ok(rows > 0)
}

/// List all whitelist entries.
pub fn list(conn: &Connection) -> Result<Vec<WhitelistEntry>> {
    let mut stmt = conn.prepare(
        "SELECT platform, channel_id, note, added_at FROM whitelist ORDER BY added_at DESC",
    )?;
    let entries = stmt
        .query_map([], |row| {
            Ok(WhitelistEntry {
                platform: row.get(0)?,
                channel_id: row.get(1)?,
                note: row.get(2)?,
                added_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Record a blocked sender in `seen_senders` so admins can review and whitelist them.
pub fn record_seen(conn: &Connection, platform: &str, channel_id: &str) -> Result<()> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO seen_senders (platform, channel_id, first_seen, last_seen, hit_count)
         VALUES (?1, ?2, ?3, ?3, 1)
         ON CONFLICT(platform, channel_id) DO UPDATE
             SET last_seen = excluded.last_seen,
                 hit_count = hit_count + 1",
        params![platform, channel_id, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS whitelist (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 platform   TEXT    NOT NULL,
                 channel_id TEXT    NOT NULL,
                 note       TEXT,
                 added_at   INTEGER NOT NULL,
                 UNIQUE(platform, channel_id)
             );
             CREATE TABLE IF NOT EXISTS seen_senders (
                 platform   TEXT    NOT NULL,
                 channel_id TEXT    NOT NULL,
                 first_seen INTEGER NOT NULL,
                 last_seen  INTEGER NOT NULL,
                 hit_count  INTEGER NOT NULL DEFAULT 1,
                 PRIMARY KEY (platform, channel_id)
             );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn add_and_check() {
        let conn = mem_db();
        assert!(!check(&conn, "telegram", "alice").unwrap());
        add(&conn, "telegram", "alice", Some("test user")).unwrap();
        assert!(check(&conn, "telegram", "alice").unwrap());
    }

    #[test]
    fn remove_returns_false_when_not_present() {
        let conn = mem_db();
        assert!(!remove(&conn, "discord", "nobody").unwrap());
    }

    #[test]
    fn remove_deletes_entry() {
        let conn = mem_db();
        add(&conn, "slack", "bob", None).unwrap();
        assert!(remove(&conn, "slack", "bob").unwrap());
        assert!(!check(&conn, "slack", "bob").unwrap());
    }

    #[test]
    fn list_returns_entries_newest_first() {
        let conn = mem_db();
        add(&conn, "telegram", "a", None).unwrap();
        add(&conn, "telegram", "b", Some("note")).unwrap();
        let entries = list(&conn).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn add_is_idempotent() {
        let conn = mem_db();
        add(&conn, "discord", "x", None).unwrap();
        add(&conn, "discord", "x", Some("updated")).unwrap();
        let entries = list(&conn).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].note.as_deref(), Some("updated"));
    }

    #[test]
    fn record_seen_increments_hit_count() {
        let conn = mem_db();
        record_seen(&conn, "telegram", "stranger").unwrap();
        record_seen(&conn, "telegram", "stranger").unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT hit_count FROM seen_senders WHERE platform='telegram' AND channel_id='stranger'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
}
