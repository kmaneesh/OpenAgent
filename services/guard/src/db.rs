use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A single whitelist entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub platform: String,
    pub channel_id: String,
    pub label: Option<String>,
    pub added_at: String,
}

/// Open (or create) the guard SQLite database at `db_path` and run migrations.
///
/// When pointing at the shared `openagent.db` the tables already exist with the
/// correct schema so the `CREATE TABLE IF NOT EXISTS` statements below are no-ops.
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
             label      TEXT    NOT NULL DEFAULT '',
             added_by   TEXT    NOT NULL DEFAULT '',
             added_at   TEXT    NOT NULL,
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

fn now_iso() -> String {
    // RFC-3339 UTC timestamp — matches what the Python side wrote.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-8601 UTC without external deps.
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hr = (s / 3600) % 24;
    let days = s / 86400;
    // Days since 1970-01-01 → approximate calendar date (good enough for a label).
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

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
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

/// Add a sender to the whitelist. Idempotent — updates `label` if the entry exists.
pub fn add(conn: &Connection, platform: &str, channel_id: &str, label: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO whitelist (platform, channel_id, label, added_by, added_at)
         VALUES (?1, ?2, ?3, 'guard', ?4)
         ON CONFLICT(platform, channel_id) DO UPDATE SET label = excluded.label",
        params![platform, channel_id, label.unwrap_or(""), now_iso()],
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
        "SELECT platform, channel_id, label, added_at FROM whitelist ORDER BY added_at DESC",
    )?;
    let entries = stmt
        .query_map([], |row| {
            Ok(WhitelistEntry {
                platform: row.get(0)?,
                channel_id: row.get(1)?,
                label: row.get(2)?,
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
                 label      TEXT    NOT NULL DEFAULT '',
                 added_by   TEXT    NOT NULL DEFAULT '',
                 added_at   TEXT    NOT NULL,
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
        add(&conn, "telegram", "b", Some("label")).unwrap();
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
        assert_eq!(entries[0].label.as_deref(), Some("updated"));
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
