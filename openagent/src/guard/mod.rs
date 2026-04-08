pub mod scrub;

/// Guard — inline contact whitelist.
///
/// Replaces the external `services/guard` daemon.  The SQLite database
/// (`data/guard.db`) is opened once at startup and shared via `Arc<Mutex>`.
/// All operations are synchronous (rusqlite); callers in async context must
/// use `tokio::task::spawn_blocking`.
///
/// Access policy:
///   - Guard disabled        → `"guard_disabled"` — always allowed.
///   - `web` / `whatsapp`   → `"platform_bypass"` — always allowed.
///   - `"allowed"` in table → allowed.
///   - `"blocked"` in table → blocked, visit recorded.
///   - not in table / `"unknown"` → blocked, visit recorded.  Operator must
///     call `allow()` to permit future messages.
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// GuardEntry — returned by list()
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardEntry {
    pub platform:   String,
    pub channel_id: String,
    pub name:       String,
    pub status:     String,
    pub note:       String,
    pub first_seen: String,
    pub last_seen:  String,
    pub hit_count:  i64,
}

// ---------------------------------------------------------------------------
// GuardDb
// ---------------------------------------------------------------------------

/// Shared handle to the guard SQLite database.
///
/// `Clone` is cheap — it clones the inner `Arc`.
/// Embed in `AppState` and clone to pass into `dispatch` and `console`.
#[derive(Clone, Debug)]
pub struct GuardDb {
    conn:        Arc<Mutex<Connection>>,
    pub enabled: bool,
}

impl GuardDb {
    /// Open (or create) the guard database at `db_path`.
    pub fn open(db_path: &str, enabled: bool) -> Result<Self> {
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
             );",
        )
        .context("guard db init")?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)), enabled })
    }

    // ---- read -----------------------------------------------------------------

    /// Check if a sender is allowed.
    ///
    /// Returns `(allowed, reason)` where reason is one of:
    /// `"guard_disabled"` | `"platform_bypass"` | `"allowed"` | `"blocked"` | `"unknown"`.
    pub fn check(&self, platform: &str, channel_id: &str) -> Result<(bool, String)> {
        if !self.enabled {
            return Ok((true, "guard_disabled".to_string()));
        }
        // web: local UI only.  whatsapp: JID format doesn't map to our whitelist.
        if platform == "web" || platform == "whatsapp" {
            return Ok((true, "platform_bypass".to_string()));
        }

        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("guard db lock: {e}"))?;
        let row = conn.query_row(
            "SELECT status FROM guard WHERE platform = ?1 AND channel_id = ?2",
            params![platform, channel_id],
            |row| row.get::<_, String>(0),
        );

        match row {
            Ok(status) => match status.as_str() {
                "allowed" => Ok((true, "allowed".to_string())),
                "blocked" => {
                    Self::record_seen_locked(&conn, platform, channel_id)?;
                    Ok((false, "blocked".to_string()))
                }
                _ => {
                    Self::record_seen_locked(&conn, platform, channel_id)?;
                    Ok((false, "unknown".to_string()))
                }
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Self::record_seen_locked(&conn, platform, channel_id)?;
                Ok((false, "unknown".to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    // ---- write ----------------------------------------------------------------

    /// Allow a contact.  Idempotent — updates name/note if already present.
    pub fn allow(
        &self,
        platform: &str,
        channel_id: &str,
        name: Option<&str>,
        note: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("guard db lock: {e}"))?;
        Self::set_status_locked(&conn, platform, channel_id, "allowed", name, note)
    }

    /// Block a contact.
    pub fn block(&self, platform: &str, channel_id: &str, note: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("guard db lock: {e}"))?;
        Self::set_status_locked(&conn, platform, channel_id, "blocked", None, note)
    }

    /// Set or update the human-readable name for an existing contact.
    /// Returns `true` if the contact existed in the table.
    pub fn set_name(&self, platform: &str, channel_id: &str, name: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("guard db lock: {e}"))?;
        let rows = conn.execute(
            "UPDATE guard SET name = ?1 WHERE platform = ?2 AND channel_id = ?3",
            params![name, platform, channel_id],
        )?;
        Ok(rows > 0)
    }

    /// Remove a contact entirely.  Returns `true` if a row was deleted.
    /// After removal the contact is treated as unknown (blocked) on next contact.
    pub fn remove(&self, platform: &str, channel_id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("guard db lock: {e}"))?;
        let rows = conn.execute(
            "DELETE FROM guard WHERE platform = ?1 AND channel_id = ?2",
            params![platform, channel_id],
        )?;
        Ok(rows > 0)
    }

    /// List all contacts, newest `last_seen` first.
    pub fn list(&self) -> Result<Vec<GuardEntry>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("guard db lock: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT platform, channel_id, name, status, note, first_seen, last_seen, hit_count
             FROM guard ORDER BY last_seen DESC",
        )?;
        let entries = stmt
            .query_map([], |row| {
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
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    // ---- private helpers ------------------------------------------------------

    fn record_seen_locked(conn: &Connection, platform: &str, channel_id: &str) -> Result<()> {
        let now = now_iso();
        conn.execute(
            "INSERT INTO guard (platform, channel_id, status, first_seen, last_seen, hit_count)
             VALUES (?1, ?2, 'unknown', ?3, ?3, 1)
             ON CONFLICT(platform, channel_id) DO UPDATE
                 SET last_seen = excluded.last_seen,
                     hit_count = hit_count + 1",
            params![platform, channel_id, now],
        )?;
        Ok(())
    }

    fn set_status_locked(
        conn: &Connection,
        platform: &str,
        channel_id: &str,
        status: &str,
        name: Option<&str>,
        note: Option<&str>,
    ) -> Result<()> {
        let now = now_iso();
        conn.execute(
            "INSERT INTO guard (platform, channel_id, name, status, note, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(platform, channel_id) DO UPDATE
                 SET status    = excluded.status,
                     name      = CASE WHEN excluded.name != '' THEN excluded.name ELSE name END,
                     note      = CASE WHEN excluded.note != '' THEN excluded.note ELSE note END,
                     last_seen = excluded.last_seen",
            params![
                platform,
                channel_id,
                name.unwrap_or(""),
                status,
                note.unwrap_or(""),
                now
            ],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Timestamp helper — no external deps, no chrono
// ---------------------------------------------------------------------------

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
    let months = [
        31u64,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_guard() -> GuardDb {
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
                 first_seen  TEXT NOT NULL DEFAULT '',
                 last_seen   TEXT NOT NULL DEFAULT '',
                 hit_count   INTEGER NOT NULL DEFAULT 1,
                 UNIQUE(platform, channel_id)
             );",
        )
        .unwrap();
        GuardDb { conn: Arc::new(Mutex::new(conn)), enabled: true }
    }

    #[test]
    fn web_always_allowed() {
        let g = mem_guard();
        let (ok, reason) = g.check("web", "browser").unwrap();
        assert!(ok);
        assert_eq!(reason, "platform_bypass");
    }

    #[test]
    fn whatsapp_always_allowed() {
        let g = mem_guard();
        let (ok, reason) = g.check("whatsapp", "123@s.whatsapp.net").unwrap();
        assert!(ok);
        assert_eq!(reason, "platform_bypass");
    }

    #[test]
    fn unknown_sender_is_blocked() {
        let g = mem_guard();
        let (ok, reason) = g.check("telegram", "stranger").unwrap();
        assert!(!ok);
        assert_eq!(reason, "unknown");
    }

    #[test]
    fn allow_then_check() {
        let g = mem_guard();
        g.allow("discord", "alice", Some("Alice"), None).unwrap();
        let (ok, reason) = g.check("discord", "alice").unwrap();
        assert!(ok);
        assert_eq!(reason, "allowed");
    }

    #[test]
    fn block_then_check() {
        let g = mem_guard();
        g.block("slack", "spammer", None).unwrap();
        let (ok, reason) = g.check("slack", "spammer").unwrap();
        assert!(!ok);
        assert_eq!(reason, "blocked");
    }

    #[test]
    fn remove_resets_to_unknown() {
        let g = mem_guard();
        g.allow("discord", "x", None, None).unwrap();
        let removed = g.remove("discord", "x").unwrap();
        assert!(removed);
        let (ok, reason) = g.check("discord", "x").unwrap();
        assert!(!ok);
        assert_eq!(reason, "unknown");
    }

    #[test]
    fn set_name_returns_false_when_missing() {
        let g = mem_guard();
        let updated = g.set_name("slack", "nobody", "Bob").unwrap();
        assert!(!updated);
    }

    #[test]
    fn set_name_updates_existing() {
        let g = mem_guard();
        g.allow("slack", "bob", Some(""), None).unwrap();
        let updated = g.set_name("slack", "bob", "Bob Smith").unwrap();
        assert!(updated);
    }

    #[test]
    fn list_returns_all() {
        let g = mem_guard();
        g.allow("telegram", "a", Some("Alice"), None).unwrap();
        g.block("telegram", "b", None).unwrap();
        let entries = g.list().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn disabled_guard_allows_all() {
        let mut g = mem_guard();
        g.enabled = false;
        let (ok, reason) = g.check("telegram", "anyone").unwrap();
        assert!(ok);
        assert_eq!(reason, "guard_disabled");
    }
}
