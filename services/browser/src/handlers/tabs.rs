//! Tab management handlers: tab_new, tab_switch, tab_list, tab_close.

use crate::params::require_session_id;
use crate::runner::run_session;
use crate::session::{lookup_session, SessionMap};
use anyhow::Result;
use serde_json::{json, Value};

pub fn handle_tab_new(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;

    if let Some(url) = params.get("url").and_then(|v| v.as_str()) {
        run_session(&session_id, &["tab", "new", url])?;
    } else {
        run_session(&session_id, &["tab", "new"])?;
    }

    let tab_list = run_session(&session_id, &["tab"])?;
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "tabs": tab_list }),
    )?)
}

pub fn handle_tab_switch(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let n = params
        .get("n")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'n' (tab number)"))?;
    lookup_session(&sessions, &session_id)?;

    run_session(&session_id, &["tab", &n.to_string()])?;
    let current_url = run_session(&session_id, &["get", "url"]).unwrap_or_default();

    if let Some(s) = sessions
        .lock()
        .expect("sessions poisoned")
        .get_mut(&session_id)
    {
        s.current_url = current_url.clone();
    }

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "tab": n, "url": current_url }),
    )?)
}

pub fn handle_tab_list(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let result = run_session(&session_id, &["tab"])?;
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "tabs": result }),
    )?)
}

pub fn handle_tab_close(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    if let Some(n) = params.get("n").and_then(|v| v.as_i64()) {
        run_session(&session_id, &["tab", "close", &n.to_string()])?;
    } else {
        run_session(&session_id, &["tab", "close"])?;
    }
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id }),
    )?)
}
