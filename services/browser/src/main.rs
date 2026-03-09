//! Browser service — MCP-lite wrapper for agent-browser CLI.
//!
//! Uses agent-browser's built-in `--session <id>` flag for persistent, isolated
//! browser sessions.  Each session keeps its own cookies, storage, and history.
//! Screenshots are written to `data/artifacts/browser/<session_id>/latest.png`.
//!
//! Install agent-browser first:
//!   npm install -g agent-browser
//!   agent-browser install        # download Chromium
//!
//! Environment variables:
//!   OPENAGENT_SOCKET_PATH   — Unix socket (default: data/sockets/browser.sock)
//!   BROWSER_BIN             — agent-browser binary (default: agent-browser)
//!   BROWSER_ARTIFACTS_DIR   — screenshot root (default: data/artifacts/browser)
//!
//! # Abort
//!
//! Panics if the log-level env filter directive is invalid, or if the session
//! mutex is poisoned due to a prior panic in a tool handler.

mod handlers;
mod params;
mod runner;
mod session;
mod tools;

use handlers::*;
use mimalloc::MiMalloc;
use sdk_rust::{setup_otel, McpLiteServer};
use serde_json::Value;
use session::SessionMap;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use tracing::info;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub const DEFAULT_SOCKET_PATH: &str = "data/sockets/browser.sock";
pub const DEFAULT_BROWSER_BIN: &str = "agent-browser";
pub const DEFAULT_ARTIFACTS_DIR: &str = "data/artifacts/browser";
/// Length of the generated session ID (hex chars from UUID v4, dashes stripped).
pub const SESSION_ID_LEN: usize = 12;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let logs_dir = env::var("OPENAGENT_LOGS_DIR").unwrap_or_else(|_| "logs".to_string());
    // _otel_guard must be kept alive until end of main — drop flushes spans.
    let _otel_guard = match setup_otel("browser", &logs_dir) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("otel init failed (continuing without file traces): {e}");
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("browser=info".parse().unwrap()),
                )
                .try_init()
                .ok();
            None
        }
    };

    let socket_path = env::var("OPENAGENT_SOCKET_PATH")
        .unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());

    fs::create_dir_all(runner::artifacts_dir()).ok();

    let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));

    // Capture sessions in a closure that clones the Arc for each handler.
    macro_rules! handler {
        ($fn:ident, $sessions:expr) => {{
            let s = Arc::clone(&$sessions);
            move |p: Value| $fn(p, Arc::clone(&s))
        }};
    }

    let mut server = McpLiteServer::new(tools::tool_definitions(), "ready");

    // ── Session lifecycle ────────────────────────────────────────────────────
    server.register_tool("browser.open",    handler!(handle_open,    sessions));
    server.register_tool("browser.navigate",handler!(handle_navigate, sessions));
    server.register_tool("browser.close",   handler!(handle_close,   sessions));
    // ── Page observation ─────────────────────────────────────────────────────
    server.register_tool("browser.snapshot",   handler!(handle_snapshot,   sessions));
    server.register_tool("browser.screenshot", handler!(handle_screenshot, sessions));
    server.register_tool("browser.get",        handler!(handle_get,        sessions));
    server.register_tool("browser.wait",       handler!(handle_wait,       sessions));
    server.register_tool("browser.eval",       handler!(handle_eval,       sessions));
    server.register_tool("browser.extract",    handler!(handle_extract,    sessions));
    server.register_tool("browser.is",         handler!(handle_is,         sessions));
    server.register_tool("browser.console",    handler!(handle_console,    sessions));
    server.register_tool("browser.errors",     handler!(handle_errors,     sessions));
    server.register_tool("browser.diff",       handler!(handle_diff,       sessions));
    server.register_tool("browser.highlight",  handler!(handle_highlight,  sessions));
    // ── User interaction ─────────────────────────────────────────────────────
    server.register_tool("browser.click",      handler!(handle_click,      sessions));
    server.register_tool("browser.dblclick",   handler!(handle_dblclick,   sessions));
    server.register_tool("browser.fill",       handler!(handle_fill,       sessions));
    server.register_tool("browser.type",       handler!(handle_type,       sessions));
    server.register_tool("browser.press",      handler!(handle_press,      sessions));
    server.register_tool("browser.hover",      handler!(handle_hover,      sessions));
    server.register_tool("browser.select",     handler!(handle_select,     sessions));
    server.register_tool("browser.check",      handler!(handle_check,      sessions));
    server.register_tool("browser.scroll",     handler!(handle_scroll,     sessions));
    server.register_tool("browser.scrollinto", handler!(handle_scrollinto, sessions));
    server.register_tool("browser.find",       handler!(handle_find,       sessions));
    server.register_tool("browser.focus",      handler!(handle_focus,      sessions));
    server.register_tool("browser.drag",       handler!(handle_drag,       sessions));
    server.register_tool("browser.upload",     handler!(handle_upload,     sessions));
    server.register_tool("browser.keydown",    handler!(handle_keydown,    sessions));
    server.register_tool("browser.keyup",      handler!(handle_keyup,      sessions));
    // ── Navigation history ───────────────────────────────────────────────────
    server.register_tool("browser.back",    handler!(handle_back,    sessions));
    server.register_tool("browser.forward", handler!(handle_forward, sessions));
    server.register_tool("browser.reload",  handler!(handle_reload,  sessions));
    // ── Tabs ─────────────────────────────────────────────────────────────────
    server.register_tool("browser.tab_new",    handler!(handle_tab_new,    sessions));
    server.register_tool("browser.tab_switch", handler!(handle_tab_switch, sessions));
    server.register_tool("browser.tab_list",   handler!(handle_tab_list,   sessions));
    server.register_tool("browser.tab_close",  handler!(handle_tab_close,  sessions));
    // ── Frames & dialogs ─────────────────────────────────────────────────────
    server.register_tool("browser.frame",  handler!(handle_frame,  sessions));
    server.register_tool("browser.dialog", handler!(handle_dialog, sessions));
    // ── Storage & state ──────────────────────────────────────────────────────
    server.register_tool("browser.cookies", handler!(handle_cookies, sessions));
    server.register_tool("browser.state",   handler!(handle_state,   sessions));
    server.register_tool("browser.storage", handler!(handle_storage, sessions));
    server.register_tool("browser.pdf",     handler!(handle_pdf,     sessions));
    // ── Browser configuration ────────────────────────────────────────────────
    server.register_tool("browser.set",     handler!(handle_set,     sessions));
    server.register_tool("browser.network", handler!(handle_network, sessions));
    server.register_tool("browser.mouse",   handler!(handle_mouse,   sessions));

    info!(socket = %socket_path, "browser.start");
    server.serve(&socket_path).await?;
    Ok(())
}
