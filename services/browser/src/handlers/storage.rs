//! Session-scoped persistence handlers: cookies, state, storage, pdf.

use crate::params::{require_session_id, require_str};
use crate::runner::{artifacts_dir, run_session};
use crate::session::{lookup_session, SessionMap};
use anyhow::Result;
use serde_json::{json, Value};

pub fn handle_cookies(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("get");

    let result = match action {
        "clear" => run_session(&session_id, &["cookies", "clear"])?,
        "set" => {
            let name = require_str(&params, "name")?.to_string();
            let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            run_session(&session_id, &["cookies", "set", &name, &value])?
        }
        _ => run_session(&session_id, &["cookies"])?,
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "result": result }),
    )?)
}

pub fn handle_state(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("save");
    let state_dir = artifacts_dir().join(&session_id).join("state.json");
    let state_str = state_dir.to_string_lossy().to_string();

    let result = match action {
        "load" => run_session(&session_id, &["state", "load", &state_str])?,
        _ => run_session(&session_id, &["state", "save", &state_str])?,
    };

    Ok(serde_json::to_string(&json!({
        "ok": true,
        "session_id": session_id,
        "action": action,
        "state_file": state_str,
        "result": result,
    }))?)
}

pub fn handle_storage(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let store = params.get("store").and_then(|v| v.as_str()).unwrap_or("local");
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("get");

    let result = match action {
        "set" => {
            let key = require_str(&params, "key")?.to_string();
            let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            run_session(&session_id, &["storage", store, "set", &key, &value])?
        }
        "clear" => run_session(&session_id, &["storage", store, "clear"])?,
        _ => {
            if let Some(key) = params.get("key").and_then(|v| v.as_str()) {
                run_session(&session_id, &["storage", store, key])?
            } else {
                run_session(&session_id, &["storage", store])?
            }
        }
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "result": result }),
    )?)
}

pub fn handle_pdf(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let session = lookup_session(&sessions, &session_id)?;
    let pdf_path = session.screenshot_dir.join("page.pdf");
    let pdf_str = pdf_path.to_string_lossy().to_string();
    run_session(&session_id, &["pdf", &pdf_str])?;
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "pdf": pdf_str }),
    )?)
}
