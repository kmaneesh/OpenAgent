//! Sandbox service — MCP-lite wrapper for microsandbox.
//!
//! Provides sandboxed code execution (Python, Node.js) and shell commands via a
//! microsandbox server (VM-level OCI isolation).  Supersedes the Go shell service.
//!
//! Tools exposed:
//!   sandbox.execute  — run Python or Node.js code via sandbox.repl.run
//!   sandbox.shell    — run a shell command via sandbox.command.run
//!
//! Environment variables:
//!   OPENAGENT_SOCKET_PATH — Unix socket path  (default: data/sockets/sandbox.sock)
//!   OPENAGENT_LOGS_DIR    — traces + metrics  (default: logs)
//!   MSB_SERVER_URL        — microsandbox URL  (default: http://127.0.0.1:5555)
//!   MSB_API_KEY           — API key (required; run: msb server keygen)
//!   MSB_MEMORY_MB         — VM memory in MB  (default: 512)
//!
//! # Abort
//!
//! Panics if the log-level env filter directive is invalid, or if microsandbox
//! returns malformed JSON that violates the expected schema.

mod handlers;
mod metrics;
mod msb;
mod tools;

use anyhow::Result;
use metrics::SandboxTelemetry;
use mimalloc::MiMalloc;
use msb::DEFAULT_SOCKET_PATH;
use sdk_rust::{setup_otel, McpLiteServer};
use std::env;
use std::sync::Arc;
use tracing::info;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    let logs_dir = env::var("OPENAGENT_LOGS_DIR").unwrap_or_else(|_| "logs".to_string());

    // ── Pillar: Traces + Logs — initialise OTEL bridge ───────────────────────
    // Writes sandbox-traces-YYYY-MM-DD.jsonl; bridges tracing! → OTEL spans.
    let _otel_guard = match setup_otel("sandbox", &logs_dir) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("otel init failed (continuing without file traces): {e}");
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("sandbox=info".parse().expect("valid log directive")),
                )
                .try_init()
                .ok();
            None
        }
    };

    // ── Pillar: Metrics — daily JSONL writer ──────────────────────────────────
    // Writes sandbox-metric-YYYY-MM-DD.jsonl; one line per tool invocation.
    let tel = Arc::new(SandboxTelemetry::new(&logs_dir)?);

    let socket_path =
        env::var("OPENAGENT_SOCKET_PATH").unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string());

    let mut server = McpLiteServer::new(tools::make_tools(), "ready");
    tools::register_handlers(&mut server, Arc::clone(&tel));

    info!(socket = %socket_path, "sandbox.start");
    server.serve(&socket_path).await?;
    Ok(())
}
