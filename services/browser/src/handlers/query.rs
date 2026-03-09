//! Page observation handlers: snapshot, screenshot, get, wait, eval, extract,
//! is, console, errors, highlight, diff.

use crate::params::{ok_with_screenshot, require_session_id, require_str};
use crate::runner::{run_session, screenshot_path};
use crate::session::{lookup_session, SessionMap};
use anyhow::Result;
use serde_json::{json, Value};

pub fn handle_snapshot(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let interactive_only = params.get("interactive_only").and_then(|v| v.as_bool()).unwrap_or(false);
    let session = lookup_session(&sessions, &session_id)?;

    let ss = screenshot_path(&session.screenshot_dir);
    let ss_str = ss.to_string_lossy().to_string();
    let snap_args: &[&str] = if interactive_only { &["snapshot", "-i"] } else { &["snapshot"] };
    let text = run_session(&session_id, snap_args)?;
    run_session(&session_id, &["screenshot", &ss_str])?;

    Ok(serde_json::to_string(&json!({
        "ok": true,
        "session_id": session_id,
        "text": text,
        "screenshot": ss_str,
        "screenshot_ts": crate::runner::ts_ms(),
    }))?)
}

pub fn handle_screenshot(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let full_page = params.get("full_page").and_then(|v| v.as_bool()).unwrap_or(false);
    let session = lookup_session(&sessions, &session_id)?;

    let ss = screenshot_path(&session.screenshot_dir);
    let ss_str = ss.to_string_lossy().to_string();
    if full_page {
        run_session(&session_id, &["screenshot", &ss_str, "--full"])?;
    } else {
        run_session(&session_id, &["screenshot", &ss_str])?;
    }

    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_get(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let what = params.get("what").and_then(|v| v.as_str()).unwrap_or("text").to_string();
    lookup_session(&sessions, &session_id)?;

    let result = if let Some(sel) = params.get("selector").and_then(|v| v.as_str()) {
        if what == "attr" {
            let attr = params.get("attr").and_then(|v| v.as_str()).unwrap_or("href");
            run_session(&session_id, &["get", &what, sel, attr])?
        } else {
            run_session(&session_id, &["get", &what, sel])?
        }
    } else {
        run_session(&session_id, &["get", &what])?
    };

    Ok(serde_json::to_string(&json!({
        "ok": true,
        "session_id": session_id,
        "what": what,
        "result": result,
    }))?)
}

pub fn handle_wait(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;

    if let Some(ms) = params.get("ms").and_then(|v| v.as_i64()) {
        run_session(&session_id, &["wait", &ms.to_string()])?;
    } else if let Some(text) = params.get("text").and_then(|v| v.as_str()) {
        run_session(&session_id, &["wait", "--text", text])?;
    } else if let Some(url_pattern) = params.get("url_pattern").and_then(|v| v.as_str()) {
        run_session(&session_id, &["wait", "--url", url_pattern])?;
    } else if let Some(load) = params.get("load_state").and_then(|v| v.as_str()) {
        run_session(&session_id, &["wait", "--load", load])?;
    } else {
        let sel = require_str(&params, "selector")?.to_string();
        run_session(&session_id, &["wait", &sel])?;
    }

    Ok(serde_json::to_string(&json!({ "ok": true, "session_id": session_id }))?)
}

pub fn handle_eval(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let js = require_str(&params, "js")?.to_string();
    lookup_session(&sessions, &session_id)?;
    let result = run_session(&session_id, &["eval", &js])?;
    Ok(serde_json::to_string(&json!({
        "ok": true,
        "session_id": session_id,
        "result": result,
    }))?)
}

pub fn handle_extract(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;

    let result = if let Some(sel) = params.get("selector").and_then(|v| v.as_str()) {
        run_session(&session_id, &["get", "text", sel])?
    } else {
        run_session(&session_id, &["snapshot"])?
    };

    Ok(serde_json::to_string(&json!({
        "ok": true,
        "session_id": session_id,
        "text": result,
    }))?)
}

pub fn handle_is(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let check = params.get("check").and_then(|v| v.as_str()).unwrap_or("visible").to_string();
    let selector = require_str(&params, "selector")?.to_string();
    lookup_session(&sessions, &session_id)?;
    let result = run_session(&session_id, &["is", &check, &selector]).unwrap_or_default();
    let value = result.trim().eq_ignore_ascii_case("true") || result.trim() == "1";
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "check": check, "result": value }),
    )?)
}

pub fn handle_console(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let result = if params.get("clear").and_then(|v| v.as_bool()).unwrap_or(false) {
        run_session(&session_id, &["console", "--clear"])?
    } else {
        run_session(&session_id, &["console"])?
    };
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "output": result }),
    )?)
}

pub fn handle_errors(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let result = if params.get("clear").and_then(|v| v.as_bool()).unwrap_or(false) {
        run_session(&session_id, &["errors", "--clear"])?
    } else {
        run_session(&session_id, &["errors"])?
    };
    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "errors": result }),
    )?)
}

pub fn handle_highlight(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["highlight", &selector])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_diff(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or("snapshot");
    lookup_session(&sessions, &session_id)?;

    let result = match kind {
        "screenshot" => {
            let baseline = require_str(&params, "baseline")?.to_string();
            run_session(&session_id, &["diff", "screenshot", "--baseline", &baseline])?
        }
        _ => run_session(&session_id, &["diff", "snapshot"])?,
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "diff": result }),
    )?)
}
