//! Browser session registry — in-memory map of active Chromium contexts.
//!
//! Each entry maps a `session_id` string to a [`BrowserSession`] that tracks
//! the screenshot directory and the last-known URL.  The map is wrapped in
//! `Arc<Mutex<…>>` so it can be shared across synchronous tool handlers.

use crate::runner::ensure_session_dir;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Per-session state kept in the service process.
#[derive(Debug, Clone)]
pub struct BrowserSession {
    pub screenshot_dir: PathBuf,
    pub current_url: String,
}

/// Shared, thread-safe session registry passed into every tool handler.
pub type SessionMap = Arc<Mutex<HashMap<String, BrowserSession>>>;

/// Return an existing session or create a fresh one for `session_id`.
///
/// The session directory under `data/artifacts/browser/<session_id>/` is
/// created on demand.  The new entry is inserted into `sessions` before
/// returning so subsequent tool calls can look it up without re-opening.
pub fn get_or_create_session(
    sessions: &SessionMap,
    session_id: &str,
    url: &str,
) -> Result<BrowserSession> {
    let dir = ensure_session_dir(session_id)?;
    let s = BrowserSession {
        screenshot_dir: dir,
        current_url: url.to_string(),
    };
    sessions
        .lock()
        .expect("sessions poisoned")
        .insert(session_id.to_string(), s.clone());
    Ok(s)
}

/// Look up an existing session or return an error telling the LLM to call
/// `browser.open` first.
pub fn lookup_session(sessions: &SessionMap, session_id: &str) -> Result<BrowserSession> {
    sessions
        .lock()
        .expect("sessions poisoned")
        .get(session_id)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown session '{}'. Call browser.open first.",
                session_id
            )
        })
}
