//! Tool handler implementations: sandbox.execute and sandbox.shell.
//!
//! Each handler wires all four OTEL pillars via SandboxTelemetry:
//!   Traces  — tracing::info_span! with per-operation attributes
//!   Metrics — SandboxTelemetry::record() on success and error
//!   Logs    — structured tracing::{info!, error!} events on every path
//!   Baggage — attach_context() propagates remote parent + tool/language tags

use crate::metrics::{
    elapsed_ms, execute_err, execute_ok, shell_err, shell_ok, SandboxTelemetry,
};
use crate::msb::{sandbox_name, MsbClient};
use anyhow::Result;
use opentelemetry::KeyValue;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, info_span, warn};

pub fn handle_execute(params: Value, tel: Arc<SandboxTelemetry>) -> Result<String> {
    let p = params
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Params must be an object"))?;
    let lang = p.get("language").and_then(|v| v.as_str()).unwrap_or("").trim();
    let code = p.get("code").and_then(|v| v.as_str()).unwrap_or("").trim();

    if lang.is_empty() {
        return Err(anyhow::anyhow!("Missing 'language' parameter"));
    }
    if code.is_empty() {
        return Err(anyhow::anyhow!("Missing 'code' parameter"));
    }

    let (image, repl_lang) = match lang {
        "python" => ("microsandbox/python", "python"),
        "node" | "javascript" | "js" => ("microsandbox/node", "javascript"),
        other => {
            return Err(anyhow::anyhow!(
                "Unsupported language '{other}'. Supported: python, node"
            ))
        }
    };

    // ── Pillar: Baggage — attach context + remote parent span ────────────────
    // sdk-rust injects _trace_id/_span_id from the MCP-lite frame so this
    // span becomes a child of the Python AgentLoop span in the trace backend.
    let _cx_guard = SandboxTelemetry::attach_context(
        &params,
        vec![
            KeyValue::new("tool", "sandbox.execute"),
            KeyValue::new("language", lang.to_string()),
        ],
    );

    // ── Pillar: Traces — span wrapping the full execution lifecycle ───────────
    let span = info_span!(
        "sandbox.execute",
        language = lang,
        sandbox_name = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
        output_len = tracing::field::Empty,
        status = tracing::field::Empty,
    );
    let _enter = span.enter();

    let name = sandbox_name(lang);
    span.record("sandbox_name", name.as_str());

    // ── Pillar: Logs — structured event at invocation start ──────────────────
    info!(language = lang, sandbox = %name, "sandbox.execute start");

    let t_start = Instant::now();
    let msb = MsbClient::from_env()?;
    msb.start(&name, image)?;
    let result = msb.repl_run(&name, repl_lang, code);
    msb.stop(&name);
    let duration_ms = elapsed_ms(t_start);

    // ── Pillar: Traces + Logs + Metrics — record outcome ────────────────────
    match &result {
        Ok(output) => {
            let output_len = output.len();
            span.record("duration_ms", duration_ms);
            span.record("output_len", output_len as i64);
            span.record("status", "ok");
            // Logs
            info!(
                language = lang, sandbox = %name,
                duration_ms, output_len,
                "sandbox.execute ok"
            );
            // Metrics
            tel.record(&execute_ok(lang, &name, duration_ms, output_len));
        }
        Err(e) => {
            span.record("duration_ms", duration_ms);
            span.record("status", "error");
            // Logs
            error!(
                language = lang, sandbox = %name,
                duration_ms, error = %e,
                "sandbox.execute error"
            );
            // Metrics
            tel.record(&execute_err(lang, &name, duration_ms));
        }
    }

    result
}

pub fn handle_shell(params: Value, tel: Arc<SandboxTelemetry>) -> Result<String> {
    let p = params
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Params must be an object"))?;
    let command = p.get("command").and_then(|v| v.as_str()).unwrap_or("").trim();

    if command.is_empty() {
        return Err(anyhow::anyhow!("Missing 'command' parameter"));
    }

    // ── Pillar: Baggage — attach context + remote parent span ────────────────
    let _cx_guard = SandboxTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "sandbox.shell")],
    );

    // ── Pillar: Traces ────────────────────────────────────────────────────────
    let span = info_span!(
        "sandbox.shell",
        sandbox_name = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
        output_len = tracing::field::Empty,
        status = tracing::field::Empty,
    );
    let _enter = span.enter();

    let name = sandbox_name("shell");
    span.record("sandbox_name", name.as_str());

    // ── Pillar: Logs ─────────────────────────────────────────────────────────
    info!(sandbox = %name, "sandbox.shell start");

    let t_start = Instant::now();
    let msb = MsbClient::from_env()?;
    // Python image ships with bash, coreutils, and common Unix tools.
    if let Err(e) = msb.start(&name, "microsandbox/python") {
        warn!(sandbox = %name, error = %e, "sandbox start failed");
        return Err(e);
    }
    let result = msb.command_run(&name, command);
    msb.stop(&name);
    let duration_ms = elapsed_ms(t_start);

    // ── Pillar: Traces + Logs + Metrics — record outcome ─────────────────────
    match &result {
        Ok(output) => {
            let output_len = output.len();
            span.record("duration_ms", duration_ms);
            span.record("output_len", output_len as i64);
            span.record("status", "ok");
            info!(
                sandbox = %name, duration_ms, output_len,
                "sandbox.shell ok"
            );
            tel.record(&shell_ok(&name, duration_ms, output_len));
        }
        Err(e) => {
            span.record("duration_ms", duration_ms);
            span.record("status", "error");
            error!(
                sandbox = %name, duration_ms, error = %e,
                "sandbox.shell error"
            );
            tel.record(&shell_err(&name, duration_ms));
        }
    }

    result
}
