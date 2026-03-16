use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardEntry {
    pub platform: String,
    pub channel_id: String,
    pub name: String,
    pub status: String,  // "allowed" | "blocked" | "unknown"
    pub note: String,
    pub first_seen: String,
    pub last_seen: String,
    pub hit_count: i64,
}

pub fn open(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("open guard db at {db_path}"))?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;

         CREATE TABLE IF NOT EXISTS guard (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             platform    TEXT NOT NULL,
             channel_id  TEXT NOT NULL,
             name        TEXT NOT NULL DEFAULT '',
             status      TEXT NOT NULL DEFAULT 'unknown',
             note        TEXT NOT NULL DEFAULT '',
             first_seen  TEXT NOT NULL,
             last_seen   TEXT NOT NULL,
             hit_count   INTEGER NOT NULL DEFAULT 1,
             UNIQUE(platform, channel_id)
         );

         -- Migrate existing whitelist entries → guard (status='allowed')
         INSERT OR IGNORE INTO guard (platform, channel_id, name, status, first_seen, last_seen)
         SELECT platform, channel_id, COALESCE(NULLIF(label,''), ''), 'allowed', added_at, added_at
         FROM whitelist WHERE EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='whitelist');

         -- Migrate existing blacklist entries → guard (status='blocked')
         INSERT OR IGNORE INTO guard (platform, channel_id, status, first_seen, last_seen, hit_count)
         SELECT platform, channel_id, 'blocked', first_seen, last_seen, message_count
         FROM blacklist WHERE EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='blacklist');

         -- Migrate seen_senders → guard (status='unknown')
         INSERT OR IGNORE INTO guard (platform, channel_id, status, first_seen, last_seen, hit_count)
         SELECT platform, channel_id, 'unknown', first_seen, last_seen, message_count
         FROM seen_senders WHERE EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='seen_senders');",
    )
    .context("guard db migration")?;

    Ok(conn)
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
        if days < dy { break; }
        days -= dy;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let months = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1u64;
    for dm in &months {
        if days < *dm { break; }
        days -= dm;
        m += 1;
    }
    (y, m, days + 1)
}

/// Check a sender's status. Returns (status, name).
/// status is "allowed", "blocked", or "unknown" (not seen before).
pub fn check(conn: &Connection, platform: &str, channel_id: &str) -> Result<(String, String)> {
    let row = conn.query_row(
        "SELECT status, name FROM guard WHERE platform = ?1 AND channel_id = ?2",
        params![platform, channel_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );
    match row {
        Ok((status, name)) => Ok((status, name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(("unknown".to_string(), String::new())),
        Err(e) => Err(e.into()),
    }
}

/// Record a contact being seen. If not in guard table, inserts as 'unknown'.
/// If already present (any status), just updates last_seen and increments hit_count.
pub fn record_seen(conn: &Connection, platform: &str, channel_id: &str) -> Result<()> {
    let now = now_iso();
    conn.execute(
        "INSERT INTO guard (platform, channel_id, status, first_seen, last_seen, hit_count)
         VALUES (?1, ?2, 'unknown', ?3, ?3, 1)
         ON CONFLICT(platform, channel_id) DO UPDATE
             SET last_seen  = excluded.last_seen,
                 hit_count  = hit_count + 1",
        params![platform, channel_id, now],
    )?;
    Ok(())
}

/// Set status for a contact. Inserts if not present.
pub fn set_status(conn: &Connection, platform: &str, channel_id: &str, status: &str, name: Option<&str>, note: Option<&str>) -> Result<()> {
    let now = now_iso();
    conn.execute(
        "INSERT INTO guard (platform, channel_id, name, status, note, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(platform, channel_id) DO UPDATE
             SET status    = excluded.status,
                 name      = CASE WHEN excluded.name != '' THEN excluded.name ELSE name END,
                 note      = CASE WHEN excluded.note != '' THEN excluded.note ELSE note END,
                 last_seen = excluded.last_seen",
        params![platform, channel_id, name.unwrap_or(""), status, note.unwrap_or(""), now],
    )?;
    Ok(())
}

/// Set the human-readable name for a contact.
pub fn set_name(conn: &Connection, platform: &str, channel_id: &str, name: &str) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE guard SET name = ?1 WHERE platform = ?2 AND channel_id = ?3",
        params![name, platform, channel_id],
    )?;
    Ok(rows > 0)
}

/// Remove a guard entry entirely. Returns true if a row was deleted.
pub fn remove(conn: &Connection, platform: &str, channel_id: &str) -> Result<bool> {
    let rows = conn.execute(
        "DELETE FROM guard WHERE platform = ?1 AND channel_id = ?2",
        params![platform, channel_id],
    )?;
    Ok(rows > 0)
}

/// List all guard entries, newest last_seen first.
pub fn list(conn: &Connection) -> Result<Vec<GuardEntry>> {
    let mut stmt = conn.prepare(
        "SELECT platform, channel_id, name, status, note, first_seen, last_seen, hit_count
         FROM guard ORDER BY last_seen DESC",
    )?;
    let entries = stmt.query_map([], |row| {
        Ok(GuardEntry {
            platform:   row.get(0)?,
            channel_id: row.get(1)?,
            name:       row.get(2)?,
            status:     row.get(3)?,
            note:       row.get(4)?,
            first_seen: row.get(5)?,
            last_seen:  row.get(6)?,
            hit_count:  row.get(7)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS guard (
                 id          INTEGER PRIMARY KEY AUTOINCREMENT,
                 platform    TEXT NOT NULL,
                 channel_id  TEXT NOT NULL,
                 name        TEXT NOT NULL DEFAULT '',
                 status      TEXT NOT NULL DEFAULT 'unknown',
                 note        TEXT NOT NULL DEFAULT '',
                 first_seen  TEXT NOT NULL,
                 last_seen   TEXT NOT NULL,
                 hit_count   INTEGER NOT NULL DEFAULT 1,
                 UNIQUE(platform, channel_id)
             );",
        ).unwrap();
        conn
    }

    #[test]
    fn unknown_by_default() {
        let conn = mem_db();
        let (status, name) = check(&conn, "telegram", "stranger").unwrap();
        assert_eq!(status, "unknown");
        assert_eq!(name, "");
    }

    #[test]
    fn allow_then_check() {
        let conn = mem_db();
        set_status(&conn, "discord", "alice", "allowed", Some("Alice"), None).unwrap();
        let (status, name) = check(&conn, "discord", "alice").unwrap();
        assert_eq!(status, "allowed");
        assert_eq!(name, "Alice");
    }

    #[test]
    fn block_then_check() {
        let conn = mem_db();
        set_status(&conn, "telegram", "spammer", "blocked", None, Some("spam")).unwrap();
        let (status, _) = check(&conn, "telegram", "spammer").unwrap();
        assert_eq!(status, "blocked");
    }

    #[test]
    fn record_seen_increments() {
        let conn = mem_db();
        record_seen(&conn, "whatsapp", "stranger").unwrap();
        record_seen(&conn, "whatsapp", "stranger").unwrap();
        let (status, _) = check(&conn, "whatsapp", "stranger").unwrap();
        assert_eq!(status, "unknown");
        let count: i64 = conn.query_row(
            "SELECT hit_count FROM guard WHERE platform='whatsapp' AND channel_id='stranger'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn set_name_returns_false_when_missing() {
        let conn = mem_db();
        assert!(!set_name(&conn, "slack", "nobody", "Bob").unwrap());
    }

    #[test]
    fn set_name_updates_existing() {
        let conn = mem_db();
        set_status(&conn, "slack", "bob", "allowed", Some(""), None).unwrap();
        assert!(set_name(&conn, "slack", "bob", "Bob Smith").unwrap());
        let (_, name) = check(&conn, "slack", "bob").unwrap();
        assert_eq!(name, "Bob Smith");
    }

    #[test]
    fn remove_deletes_entry() {
        let conn = mem_db();
        set_status(&conn, "discord", "x", "allowed", None, None).unwrap();
        assert!(remove(&conn, "discord", "x").unwrap());
        let (status, _) = check(&conn, "discord", "x").unwrap();
        assert_eq!(status, "unknown");
    }

    #[test]
    fn list_returns_all() {
        let conn = mem_db();
        set_status(&conn, "telegram", "a", "allowed", Some("Alice"), None).unwrap();
        set_status(&conn, "telegram", "b", "blocked", None, None).unwrap();
        record_seen(&conn, "telegram", "c").unwrap();
        let entries = list(&conn).unwrap();
        assert_eq!(entries.len(), 3);
    }
}
