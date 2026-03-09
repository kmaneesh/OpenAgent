//! User-input handlers: click, fill, type, press, hover, select, check,
//! scroll, scrollinto, find, focus, drag, upload, keydown, keyup, dblclick.

use crate::params::{ok_with_screenshot, require_session_id, require_str};
use crate::runner::{run_session, screenshot_path};
use crate::session::{lookup_session, SessionMap};
use anyhow::Result;
use serde_json::{json, Value};

pub fn handle_click(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let session = lookup_session(&sessions, &session_id)?;
    let selector = params.get("selector").and_then(|v| v.as_str()).map(str::to_string);
    let new_tab = params.get("new_tab").and_then(|v| v.as_bool()).unwrap_or(false);

    if let Some(sel) = selector {
        if new_tab {
            run_session(&session_id, &["click", &sel, "--new-tab"])?;
        } else {
            run_session(&session_id, &["click", &sel])?;
        }
    } else {
        let x = params
            .get("x")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Provide 'selector' or 'x'+'y' coordinates"))?;
        let y = params
            .get("y")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Provide 'selector' or 'x'+'y' coordinates"))?;
        run_session(&session_id, &["mouse", "move", &x.to_string(), &y.to_string()])?;
        run_session(&session_id, &["mouse", "down"])?;
        run_session(&session_id, &["mouse", "up"])?;
    }

    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_dblclick(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["dblclick", &selector])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_fill(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["fill", &selector, &text])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_type(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let text = require_str(&params, "text")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;

    if let Some(sel) = params.get("selector").and_then(|v| v.as_str()) {
        run_session(&session_id, &["type", sel, &text])?;
    } else {
        run_session(&session_id, &["keyboard", "type", &text])?;
    }

    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_press(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let key = require_str(&params, "key")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["press", &key])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_hover(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["hover", &selector])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_select(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let value = require_str(&params, "value")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["select", &selector, &value])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_check(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let uncheck = params.get("uncheck").and_then(|v| v.as_bool()).unwrap_or(false);
    let session = lookup_session(&sessions, &session_id)?;

    if uncheck {
        run_session(&session_id, &["uncheck", &selector])?;
    } else {
        run_session(&session_id, &["check", &selector])?;
    }

    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_scroll(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let direction = params.get("direction").and_then(|v| v.as_str()).unwrap_or("down").to_string();
    let amount = params.get("amount").and_then(|v| v.as_i64()).unwrap_or(500).to_string();
    let session = lookup_session(&sessions, &session_id)?;

    if let Some(sel) = params.get("selector").and_then(|v| v.as_str()) {
        run_session(&session_id, &["scroll", &direction, &amount, "--selector", sel])?;
    } else {
        run_session(&session_id, &["scroll", &direction, &amount])?;
    }

    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_scrollinto(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["scrollintoview", &selector])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_find(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let by = params.get("by").and_then(|v| v.as_str()).unwrap_or("text").to_string();
    let value = require_str(&params, "value")?.to_string();
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("click").to_string();
    let session = lookup_session(&sessions, &session_id)?;

    let mut args: Vec<String> = vec!["find".to_string()];
    if by == "nth" {
        let n = params
            .get("n")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("'n' is required when by=nth"))?;
        args.extend(["nth".to_string(), n.to_string(), value.clone(), action.clone()]);
    } else {
        args.extend([by.clone(), value.clone(), action.clone()]);
    }
    if let Some(av) = params.get("action_value").and_then(|v| v.as_str()) {
        args.push(av.to_string());
    }
    if params.get("exact").and_then(|v| v.as_bool()).unwrap_or(false) {
        args.push("--exact".to_string());
    }
    if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
        args.extend(["--name".to_string(), name.to_string()]);
    }

    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_session(&session_id, &refs)?;

    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_focus(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["focus", &selector])?;
    Ok(serde_json::to_string(&json!({ "ok": true, "session_id": session_id }))?)
}

pub fn handle_drag(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let source = require_str(&params, "source")?.to_string();
    let target = require_str(&params, "target")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["drag", &source, &target])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_upload(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let selector = require_str(&params, "selector")?.to_string();
    let file = require_str(&params, "file")?.to_string();
    let session = lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["upload", &selector, &file])?;
    let ss = screenshot_path(&session.screenshot_dir);
    run_session(&session_id, &["screenshot", &ss.to_string_lossy().to_string()])?;
    Ok(serde_json::to_string(&ok_with_screenshot(
        &session_id,
        &session.screenshot_dir,
        json!({}),
    ))?)
}

pub fn handle_keydown(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let key = require_str(&params, "key")?.to_string();
    lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["keydown", &key])?;
    Ok(serde_json::to_string(&json!({ "ok": true, "session_id": session_id }))?)
}

pub fn handle_keyup(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    let key = require_str(&params, "key")?.to_string();
    lookup_session(&sessions, &session_id)?;
    run_session(&session_id, &["keyup", &key])?;
    Ok(serde_json::to_string(&json!({ "ok": true, "session_id": session_id }))?)
}
