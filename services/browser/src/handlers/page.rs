//! Page lifecycle handlers: open, navigate, back, forward, reload, close.

use crate::app_config::BrowserDefaults;
use crate::params::{ok_with_screenshot, require_session_id, require_str};
use crate::runner::{new_session_id, run_session, run_session_with_identity, screenshot_path};
use crate::session::{get_or_create_session, lookup_session, SessionMap};
use anyhow::Result;
use serde_json::{json, Value};
use tracing::{info, warn};

pub fn handle_open(params: Value, sessions: SessionMap) -> Result<String> {
    let url = require_str(&params, "url")?.trim().to_string();
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(new_session_id);

    let session = get_or_create_session(&sessions, &session_id, &url)?;
    let ss = screenshot_path(&session.screenshot_dir);
    let ss_str = ss.to_string_lossy().to_string();
    let defaults = BrowserDefaults::load()?;

    run_session_with_identity(&session_id, &["open", &url], Some(&defaults.identity))?;
    if let (Some(width), Some(height)) = (
        defaults.identity.viewport_width,
        defaults.identity.viewport_height,
    ) {
        run_session(
            &session_id,
            &["set", "viewport", &width.to_string(), &height.to_string()],
        )?;
    }
    run_session(&session_id, &["screenshot", &ss_str])?;

    info!(session_id = %session_id, url = %url, "browser session opened");

    Ok(serde_json::to_string(&json!({
        "ok": true,
        "session_id": session_id,
        "url": url,
        "screenshot": ss_str,
        "screenshot_ts": crate::runner::ts_ms(),
    }))?)
}

pub fn handle_navigate(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let url = require_str(&params, "url")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;

    let ss = screenshot_path(&session.screenshot_dir);
    let ss_str = ss.to_string_lossy().to_string();
    run_session(&session_id, &["open", &url])?;
    run_session(&session_id, &["screenshot", &ss_str])?;

    if let Some(s) = sessions
        .lock()
        .expect("sessions poisoned")
        .get_mut(&session_id)
    {
        s.current_url = url.clone();
    }

    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({ "url": url }),
    ))?)
}

pub fn handle_back(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["back"])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(
        &session_id,
        &["screenshot", &ss.to_string_lossy().to_string()],
    )?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_forward(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["forward"])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(
        &session_id,
        &["screenshot", &ss.to_string_lossy().to_string()],
    )?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_reload(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["reload"])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(
        &session_id,
        &["screenshot", &ss.to_string_lossy().to_string()],
    )?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_close(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    {
        let mut guard = sessions.lock().expect("sessions poisoned");
        guard
            .remove(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown session '{}'", session_id))?;
    }
    let _ = run_session(&session_id, &["close"]);
    warn!(session_id = %session_id, "browser session closed");
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "closed": true }),
    )?)
}
