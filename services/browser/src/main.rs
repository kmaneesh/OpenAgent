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
//!   OPENAGENT_LOGS_DIR      — OTEL log/trace dir (default: logs)
//!   BROWSER_BIN             — agent-browser binary (default: agent-browser)
//!   BROWSER_ARTIFACTS_DIR   — screenshot root (default: data/artifacts/browser)

mod handlers;
mod metrics;
mod params;
mod runner;
mod session;
mod tools;

use handlers::*;
use metrics::{elapsed_ms, tool_metric, BrowserTelemetry};
use mimalloc::MiMalloc;
use sdk_rust::{setup_otel, McpLiteServer};
use serde_json::Value;
use session::SessionMap;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{error, info, info_span};

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

    let _otel_guard = setup_otel("browser", &logs_dir)
        .inspect_err(|e| eprintln!("{{\"level\":\"WARN\",\"message\":\"otel init failed\",\"error\":\"{e}\"}}"))
        .ok();

    let tel = Arc::new(BrowserTelemetry::new(&logs_dir)?);

    let socket_path = env::var("OPENAGENT_SOCKET_PATH")
        .unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());

    fs::create_dir_all(runner::artifacts_dir()).ok();

    let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));

    // ── OTEL-wrapped handler macro ────────────────────────────────────────────
    // Each tool call gets:
    //   Baggage  — remote parent from _trace_id/_span_id in params
    //   Trace    — info_span! with tool name + status
    //   Log      — info/error on completion
    //   Metrics  — BrowserTelemetry::record() with duration
    macro_rules! handler {
        ($tool:literal, $fn:ident, $sessions:expr, $tel:expr) => {{
            let s = Arc::clone(&$sessions);
            let t = Arc::clone(&$tel);
            move |p: Value| {
                let _cx_guard = BrowserTelemetry::attach_context(
                    &p,
                    vec![opentelemetry::KeyValue::new("tool", $tool)],
                );
                let span = info_span!($tool, status = tracing::field::Empty, duration_ms = tracing::field::Empty);
                let _enter = span.enter();
                let session_id = p.get("session_id").and_then(|v| v.as_str()).map(str::to_string);
                let t_start = Instant::now();
                let result = $fn(p, Arc::clone(&s));
                let duration_ms = elapsed_ms(t_start);
                match &result {
                    Ok(_) => {
                        span.record("status", "ok");
                        span.record("duration_ms", duration_ms);
                        info!(tool = $tool, duration_ms, "ok");
                        t.record(&tool_metric($tool, session_id.as_deref(), "ok", duration_ms));
                    }
                    Err(e) => {
                        span.record("status", "error");
                        span.record("duration_ms", duration_ms);
                        error!(tool = $tool, duration_ms, error = %e, "error");
                        t.record(&tool_metric($tool, session_id.as_deref(), "error", duration_ms));
                    }
                }
                result
            }
        }};
    }

    let mut server = McpLiteServer::new(tools::tool_definitions(), "ready");

    // ── Session lifecycle ────────────────────────────────────────────────────
    server.register_tool("browser.open",     handler!("browser.open",     handle_open,     sessions, tel));
    server.register_tool("browser.navigate", handler!("browser.navigate", handle_navigate, sessions, tel));
    server.register_tool("browser.close",    handler!("browser.close",    handle_close,    sessions, tel));
    // ── Page observation ─────────────────────────────────────────────────────
    server.register_tool("browser.snapshot",   handler!("browser.snapshot",   handle_snapshot,   sessions, tel));
    server.register_tool("browser.screenshot", handler!("browser.screenshot", handle_screenshot, sessions, tel));
    server.register_tool("browser.get",        handler!("browser.get",        handle_get,        sessions, tel));
    server.register_tool("browser.wait",       handler!("browser.wait",       handle_wait,       sessions, tel));
    server.register_tool("browser.eval",       handler!("browser.eval",       handle_eval,       sessions, tel));
    server.register_tool("browser.extract",    handler!("browser.extract",    handle_extract,    sessions, tel));
    server.register_tool("browser.is",         handler!("browser.is",         handle_is,         sessions, tel));
    server.register_tool("browser.console",    handler!("browser.console",    handle_console,    sessions, tel));
    server.register_tool("browser.errors",     handler!("browser.errors",     handle_errors,     sessions, tel));
    server.register_tool("browser.diff",       handler!("browser.diff",       handle_diff,       sessions, tel));
    server.register_tool("browser.highlight",  handler!("browser.highlight",  handle_highlight,  sessions, tel));
    // ── User interaction ─────────────────────────────────────────────────────
    server.register_tool("browser.click",      handler!("browser.click",      handle_click,      sessions, tel));
    server.register_tool("browser.dblclick",   handler!("browser.dblclick",   handle_dblclick,   sessions, tel));
    server.register_tool("browser.fill",       handler!("browser.fill",       handle_fill,       sessions, tel));
    server.register_tool("browser.type",       handler!("browser.type",       handle_type,       sessions, tel));
    server.register_tool("browser.press",      handler!("browser.press",      handle_press,      sessions, tel));
    server.register_tool("browser.hover",      handler!("browser.hover",      handle_hover,      sessions, tel));
    server.register_tool("browser.select",     handler!("browser.select",     handle_select,     sessions, tel));
    server.register_tool("browser.check",      handler!("browser.check",      handle_check,      sessions, tel));
    server.register_tool("browser.scroll",     handler!("browser.scroll",     handle_scroll,     sessions, tel));
    server.register_tool("browser.scrollinto", handler!("browser.scrollinto", handle_scrollinto, sessions, tel));
    server.register_tool("browser.find",       handler!("browser.find",       handle_find,       sessions, tel));
    server.register_tool("browser.focus",      handler!("browser.focus",      handle_focus,      sessions, tel));
    server.register_tool("browser.drag",       handler!("browser.drag",       handle_drag,       sessions, tel));
    server.register_tool("browser.upload",     handler!("browser.upload",     handle_upload,     sessions, tel));
    server.register_tool("browser.keydown",    handler!("browser.keydown",    handle_keydown,    sessions, tel));
    server.register_tool("browser.keyup",      handler!("browser.keyup",      handle_keyup,      sessions, tel));
    // ── Navigation history ───────────────────────────────────────────────────
    server.register_tool("browser.back",    handler!("browser.back",    handle_back,    sessions, tel));
    server.register_tool("browser.forward", handler!("browser.forward", handle_forward, sessions, tel));
    server.register_tool("browser.reload",  handler!("browser.reload",  handle_reload,  sessions, tel));
    // ── Tabs ─────────────────────────────────────────────────────────────────
    server.register_tool("browser.tab_new",    handler!("browser.tab_new",    handle_tab_new,    sessions, tel));
    server.register_tool("browser.tab_switch", handler!("browser.tab_switch", handle_tab_switch, sessions, tel));
    server.register_tool("browser.tab_list",   handler!("browser.tab_list",   handle_tab_list,   sessions, tel));
    server.register_tool("browser.tab_close",  handler!("browser.tab_close",  handle_tab_close,  sessions, tel));
    // ── Frames & dialogs ─────────────────────────────────────────────────────
    server.register_tool("browser.frame",  handler!("browser.frame",  handle_frame,  sessions, tel));
    server.register_tool("browser.dialog", handler!("browser.dialog", handle_dialog, sessions, tel));
    // ── Storage & state ──────────────────────────────────────────────────────
    server.register_tool("browser.cookies", handler!("browser.cookies", handle_cookies, sessions, tel));
    server.register_tool("browser.state",   handler!("browser.state",   handle_state,   sessions, tel));
    server.register_tool("browser.storage", handler!("browser.storage", handle_storage, sessions, tel));
    server.register_tool("browser.pdf",     handler!("browser.pdf",     handle_pdf,     sessions, tel));
    // ── Browser configuration ────────────────────────────────────────────────
    server.register_tool("browser.set",     handler!("browser.set",     handle_set,     sessions, tel));
    server.register_tool("browser.network", handler!("browser.network", handle_network, sessions, tel));
    server.register_tool("browser.mouse",   handler!("browser.mouse",   handle_mouse,   sessions, tel));

    info!(socket = %socket_path, "browser.start");
    server.serve(&socket_path).await?;
    Ok(())
}
