use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db;

/// `guard.check` — check if a sender is allowed.
///
/// Returns `{"allowed": bool, "reason": "...", "name": "..."}`.
/// reason:
///   - `"platform_bypass"` — web/whatsapp, always allowed
///   - `"allowed"`         — status is 'allowed' in guard table
///   - `"blocked"`         — status is 'blocked'
///   - `"unknown"`         — not seen before; recorded as 'unknown'
pub fn handle_check(conn: &Connection, params: Value) -> Result<String> {
    let platform = params
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: platform"))?;
    let channel_id = params
        .get("channel_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: channel_id"))?;

    // Web: local UI only. WhatsApp: @lid whitelist doesn't match new JID format.
    if platform == "web" || platform == "whatsapp" {
        return Ok(json!({"allowed": true, "reason": "platform_bypass", "name": ""}).to_string());
    }

    let (status, name) = db::check(conn, platform, channel_id)?;

    match status.as_str() {
        "allowed" => Ok(json!({"allowed": true,  "reason": "allowed",  "name": name}).to_string()),
        "blocked" => {
            let _ = db::record_seen(conn, platform, channel_id);
            Ok(json!({"allowed": false, "reason": "blocked",  "name": name}).to_string())
        }
        _ => {
            // Unknown — record the visit and block
            let _ = db::record_seen(conn, platform, channel_id);
            Ok(json!({"allowed": false, "reason": "unknown",  "name": name}).to_string())
        }
    }
}

/// `guard.allow` — add or update a contact as allowed.
pub fn handle_allow(conn: &Connection, params: Value) -> Result<String> {
    let platform = params
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: platform"))?;
    let channel_id = params
        .get("channel_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: channel_id"))?;
    // Accept "name", "label", or "note" as the display name field (backward compat).
    let name = params.get("name")
        .or_else(|| params.get("label"))
        .or_else(|| params.get("note"))
        .and_then(Value::as_str);
    let note = params.get("note").and_then(Value::as_str);

    db::set_status(conn, platform, channel_id, "allowed", name, note)?;
    let (_, resolved_name) = db::check(conn, platform, channel_id)?;
    Ok(json!({"ok": true, "platform": platform, "channel_id": channel_id, "name": resolved_name}).to_string())
}

/// `guard.block` — block a contact.
pub fn handle_block(conn: &Connection, params: Value) -> Result<String> {
    let platform = params
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: platform"))?;
    let channel_id = params
        .get("channel_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: channel_id"))?;
    let note = params.get("note").and_then(Value::as_str);

    db::set_status(conn, platform, channel_id, "blocked", None, note)?;
    Ok(json!({"ok": true, "platform": platform, "channel_id": channel_id}).to_string())
}

/// `guard.name` — set or update the human-readable name for a contact.
pub fn handle_name(conn: &Connection, params: Value) -> Result<String> {
    let platform = params
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: platform"))?;
    let channel_id = params
        .get("channel_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: channel_id"))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: name"))?;

    let updated = db::set_name(conn, platform, channel_id, name)?;
    Ok(json!({"ok": updated, "platform": platform, "channel_id": channel_id, "name": name}).to_string())
}

/// `guard.remove` — remove a contact from the guard table entirely.
pub fn handle_remove(conn: &Connection, params: Value) -> Result<String> {
    let platform = params
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: platform"))?;
    let channel_id = params
        .get("channel_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param: channel_id"))?;

    let removed = db::remove(conn, platform, channel_id)?;
    Ok(json!({"ok": removed, "platform": platform, "channel_id": channel_id}).to_string())
}

/// `guard.list` — list all contacts in the guard table.
pub fn handle_list(conn: &Connection) -> Result<String> {
    let entries = db::list(conn)?;
    let count = entries.len();
    Ok(json!({"entries": entries, "count": count}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE guard (
                 id          INTEGER PRIMARY KEY AUTOINCREMENT,
                 platform    TEXT NOT NULL,
                 channel_id  TEXT NOT NULL,
                 name        TEXT NOT NULL DEFAULT '',
                 status      TEXT NOT NULL DEFAULT 'unknown',
                 note        TEXT NOT NULL DEFAULT '',
                 first_seen  TEXT NOT NULL DEFAULT (datetime('now')),
                 last_seen   TEXT NOT NULL DEFAULT (datetime('now')),
                 hit_count   INTEGER NOT NULL DEFAULT 1,
                 UNIQUE(platform, channel_id)
             );",
        ).unwrap();
        conn
    }

    #[test]
    fn web_always_allowed() {
        let conn = mem_db();
        let result: Value = serde_json::from_str(
            &handle_check(&conn, json!({"platform": "web", "channel_id": "browser"})).unwrap(),
        ).unwrap();
        assert_eq!(result["allowed"], true);
        assert_eq!(result["reason"], "platform_bypass");
    }

    #[test]
    fn whatsapp_always_allowed() {
        let conn = mem_db();
        let result: Value = serde_json::from_str(
            &handle_check(&conn, json!({"platform": "whatsapp", "channel_id": "123@s.whatsapp.net"})).unwrap(),
        ).unwrap();
        assert_eq!(result["allowed"], true);
        assert_eq!(result["reason"], "platform_bypass");
    }

    #[test]
    fn unknown_sender_blocked_and_recorded() {
        let conn = mem_db();
        let result: Value = serde_json::from_str(
            &handle_check(&conn, json!({"platform": "telegram", "channel_id": "stranger"})).unwrap(),
        ).unwrap();
        assert_eq!(result["allowed"], false);
        assert_eq!(result["reason"], "unknown");
        // Should be recorded in guard table
        let (status, _) = db::check(&conn, "telegram", "stranger").unwrap();
        assert_eq!(status, "unknown");
    }

    #[test]
    fn allow_then_check() {
        let conn = mem_db();
        handle_allow(&conn, json!({"platform": "discord", "channel_id": "alice", "name": "Alice"})).unwrap();
        let result: Value = serde_json::from_str(
            &handle_check(&conn, json!({"platform": "discord", "channel_id": "alice"})).unwrap(),
        ).unwrap();
        assert_eq!(result["allowed"], true);
        assert_eq!(result["reason"], "allowed");
        assert_eq!(result["name"], "Alice");
    }

    #[test]
    fn block_then_check() {
        let conn = mem_db();
        handle_block(&conn, json!({"platform": "slack", "channel_id": "spammer"})).unwrap();
        let result: Value = serde_json::from_str(
            &handle_check(&conn, json!({"platform": "slack", "channel_id": "spammer"})).unwrap(),
        ).unwrap();
        assert_eq!(result["allowed"], false);
        assert_eq!(result["reason"], "blocked");
    }

    #[test]
    fn rename_contact() {
        let conn = mem_db();
        handle_allow(&conn, json!({"platform": "telegram", "channel_id": "bob"})).unwrap();
        let result: Value = serde_json::from_str(
            &handle_name(&conn, json!({"platform": "telegram", "channel_id": "bob", "name": "Bob Smith"})).unwrap(),
        ).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["name"], "Bob Smith");
    }

    #[test]
    fn remove_then_check_unknown() {
        let conn = mem_db();
        handle_allow(&conn, json!({"platform": "discord", "channel_id": "x"})).unwrap();
        handle_remove(&conn, json!({"platform": "discord", "channel_id": "x"})).unwrap();
        let result: Value = serde_json::from_str(
            &handle_check(&conn, json!({"platform": "discord", "channel_id": "x"})).unwrap(),
        ).unwrap();
        assert_eq!(result["allowed"], false);
        assert_eq!(result["reason"], "unknown");
    }

    #[test]
    fn list_count_matches() {
        let conn = mem_db();
        handle_allow(&conn, json!({"platform": "telegram", "channel_id": "a"})).unwrap();
        handle_block(&conn, json!({"platform": "telegram", "channel_id": "b"})).unwrap();
        let result: Value = serde_json::from_str(&handle_list(&conn).unwrap()).unwrap();
        assert_eq!(result["count"], 2);
    }
}
