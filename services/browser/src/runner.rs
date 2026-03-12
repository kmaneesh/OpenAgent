//! agent-browser CLI execution and filesystem utilities.
//!
//! Every tool handler calls [`run_session`] to invoke the `agent-browser`
//! binary with a `--session <id>` flag, keeping each session's Chromium
//! context fully isolated (cookies, storage, history).

use crate::app_config::BrowserIdentity;
use crate::{DEFAULT_ARTIFACTS_DIR, DEFAULT_BROWSER_BIN, SESSION_ID_LEN};
use anyhow::{Context, Result};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Return the `agent-browser` binary path from `BROWSER_BIN` env or the default.
pub fn browser_bin() -> String {
    env::var("BROWSER_BIN").unwrap_or_else(|_| DEFAULT_BROWSER_BIN.to_string())
}

/// Return the root artifacts directory from `BROWSER_ARTIFACTS_DIR` env or default.
pub fn artifacts_dir() -> PathBuf {
    PathBuf::from(
        env::var("BROWSER_ARTIFACTS_DIR").unwrap_or_else(|_| DEFAULT_ARTIFACTS_DIR.to_string()),
    )
}

/// Generate a short, URL-safe session ID from a UUID v4.
pub fn new_session_id() -> String {
    Uuid::new_v4().to_string().replace('-', "")[..SESSION_ID_LEN].to_string()
}

/// Ensure the per-session screenshot directory exists and return its path.
pub fn ensure_session_dir(session_id: &str) -> Result<PathBuf> {
    let dir = artifacts_dir().join(session_id);
    fs::create_dir_all(&dir).with_context(|| format!("Cannot create screenshot dir {:?}", dir))?;
    Ok(dir)
}

/// Canonical path for the session's rolling screenshot.
pub fn screenshot_path(dir: &Path) -> PathBuf {
    dir.join("latest.png")
}

/// Current time as milliseconds since the Unix epoch (for screenshot timestamps).
pub fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Run `agent-browser --session <session_id> [args…]` and return trimmed stdout.
///
/// On non-zero exit returns an error containing the stderr (or stdout) output.
/// Install the binary with: `npm install -g agent-browser && agent-browser install`
pub fn run_session(session_id: &str, args: &[&str]) -> Result<String> {
    run_session_with_identity(session_id, args, None)
}

pub fn run_session_with_identity(
    session_id: &str,
    args: &[&str],
    identity: Option<&BrowserIdentity>,
) -> Result<String> {
    let bin = browser_bin();
    let mut command = Command::new(&bin);
    command.arg("--session").arg(session_id);
    if let Some(identity) = identity {
        if !identity.user_agent.is_empty() {
            command.arg("--user-agent").arg(&identity.user_agent);
        }
        if !identity.color_scheme.is_empty() {
            command.arg("--color-scheme").arg(&identity.color_scheme);
        }
        if identity.headed {
            command.arg("--headed");
        }
        if !identity.launch_args.is_empty() {
            command.arg("--args").arg(identity.launch_args.join(","));
        }
        if !identity.extra_headers.is_empty() {
            command
                .arg("--headers")
                .arg(Value::Object(identity.extra_headers.clone()).to_string());
        }
    }
    let output = command.args(args).output().with_context(|| {
        format!(
            "Failed to execute '{}'. Install with: npm install -g agent-browser",
            bin
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let msg = if !stderr.is_empty() { &stderr } else { &stdout };
        return Err(anyhow::anyhow!(
            "agent-browser exited {}: {}",
            output.status,
            msg
        ));
    }

    Ok(if stdout.is_empty() { stderr } else { stdout })
}
