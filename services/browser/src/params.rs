//! Parameter extraction helpers and tool response builders.
//!
//! All handler functions receive a raw `serde_json::Value` from the MCP-lite
//! frame.  These helpers centralise the repetitive extraction and error
//! formatting so each handler stays focused on browser logic.

use crate::runner::{screenshot_path, ts_ms};
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;

/// Extract a non-empty `session_id` string from the params object.
pub fn require_session_id(params: &Value) -> Result<String> {
    params
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing 'session_id'"))
}

/// Extract a non-empty string field `key` from the params object.
pub fn require_str<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing or empty '{}'", key))
}

/// Build a standard tool response that includes `session_id`, screenshot path,
/// and timestamp, merged with any caller-supplied `extra` fields.
pub fn ok_with_screenshot(session_id: &str, dir: &Path, extra: Value) -> Value {
    let ss = screenshot_path(dir);
    let mut base = json!({
        "ok": true,
        "session_id": session_id,
        "screenshot": ss.to_string_lossy(),
        "screenshot_ts": ts_ms(),
    });
    if let (Some(obj), Some(ext)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in ext {
            obj.insert(k.clone(), v.clone());
        }
    }
    base
}
