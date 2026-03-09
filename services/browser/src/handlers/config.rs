//! Browser configuration handlers: set, network, frame, dialog, mouse.

use crate::params::{require_session_id, require_str};
use crate::runner::run_session;
use crate::session::{lookup_session, SessionMap};
use anyhow::Result;
use serde_json::{json, Value};

pub fn handle_set(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let what = params.get("what").and_then(|v| v.as_str()).unwrap_or("");

    let result = match what {
        "viewport" => {
            let w = params.get("width").and_then(|v| v.as_i64()).unwrap_or(1280).to_string();
            let h = params.get("height").and_then(|v| v.as_i64()).unwrap_or(800).to_string();
            run_session(&session_id, &["set", "viewport", &w, &h])?
        }
        "device" => {
            let name = require_str(&params, "name")?.to_string();
            run_session(&session_id, &["set", "device", &name])?
        }
        "geo" => {
            let lat = params.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0).to_string();
            let lng = params.get("lng").and_then(|v| v.as_f64()).unwrap_or(0.0).to_string();
            run_session(&session_id, &["set", "geo", &lat, &lng])?
        }
        "offline" => {
            let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("on");
            run_session(&session_id, &["set", "offline", value])?
        }
        "headers" => {
            let json_str = require_str(&params, "json")?.to_string();
            run_session(&session_id, &["set", "headers", &json_str])?
        }
        "credentials" => {
            let username = require_str(&params, "username")?.to_string();
            let password = require_str(&params, "password")?.to_string();
            run_session(&session_id, &["set", "credentials", &username, &password])?
        }
        "media" => {
            let scheme = params.get("scheme").and_then(|v| v.as_str()).unwrap_or("dark");
            run_session(&session_id, &["set", "media", scheme])?
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown 'what': {}. Use: viewport, device, geo, offline, headers, credentials, media",
                what
            ))
        }
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "result": result }),
    )?)
}

pub fn handle_network(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("requests");

    let result = match action {
        "route" => {
            let url_pattern = require_str(&params, "url_pattern")?.to_string();
            if params.get("abort").and_then(|v| v.as_bool()).unwrap_or(false) {
                run_session(&session_id, &["network", "route", &url_pattern, "--abort"])?
            } else if let Some(body) = params.get("body").and_then(|v| v.as_str()) {
                run_session(&session_id, &["network", "route", &url_pattern, "--body", body])?
            } else {
                run_session(&session_id, &["network", "route", &url_pattern])?
            }
        }
        "unroute" => {
            if let Some(url_pattern) = params.get("url_pattern").and_then(|v| v.as_str()) {
                run_session(&session_id, &["network", "unroute", url_pattern])?
            } else {
                run_session(&session_id, &["network", "unroute"])?
            }
        }
        _ => {
            if let Some(filter) = params.get("filter").and_then(|v| v.as_str()) {
                run_session(&session_id, &["network", "requests", "--filter", filter])?
            } else {
                run_session(&session_id, &["network", "requests"])?
            }
        }
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "result": result }),
    )?)
}

pub fn handle_frame(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let selector = params.get("selector").and_then(|v| v.as_str()).unwrap_or("main");
    if selector == "main" || selector.is_empty() {
        run_session(&session_id, &["frame", "main"])?;
    } else {
        run_session(&session_id, &["frame", selector])?;
    }
    Ok(serde_json::to_string(&json!({ "ok": true, "session_id": session_id }))?)
}

pub fn handle_dialog(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("dismiss");

    let result = if action == "accept" {
        if let Some(text) = params.get("text").and_then(|v| v.as_str()) {
            run_session(&session_id, &["dialog", "accept", text])?
        } else {
            run_session(&session_id, &["dialog", "accept"])?
        }
    } else {
        run_session(&session_id, &["dialog", "dismiss"])?
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "result": result }),
    )?)
}

pub fn handle_mouse(params: Value, sessions: SessionMap) -> Result<String> {
    let session_id = require_session_id(&params)?;
    lookup_session(&sessions, &session_id)?;
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("move");

    let result = match action {
        "move" => {
            let x = params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0).to_string();
            let y = params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0).to_string();
            run_session(&session_id, &["mouse", "move", &x, &y])?
        }
        "down" => {
            let button = params.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            run_session(&session_id, &["mouse", "down", button])?
        }
        "up" => {
            let button = params.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            run_session(&session_id, &["mouse", "up", button])?
        }
        "wheel" => {
            let dy = params.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0).to_string();
            let dx = params.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0).to_string();
            run_session(&session_id, &["mouse", "wheel", &dy, &dx])?
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown mouse action: {}. Use: move, down, up, wheel",
                action
            ))
        }
    };

    Ok(serde_json::to_string(
        &json!({ "ok": true, "session_id": session_id, "result": result }),
    )?)
}
