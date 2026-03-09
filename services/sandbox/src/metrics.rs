//! OTEL observability facade for the Sandbox service.
//!
//! ```text
//! Pillar   │ Mechanism                              │ Output
//! ─────────┼────────────────────────────────────────┼─────────────────────────────────────
//! Traces   │ tracing::info_span! → OTEL bridge      │ logs/sandbox-traces-YYYY-MM-DD.jsonl
//! Metrics  │ SandboxTelemetry::record()             │ logs/sandbox-metrics-YYYY-MM-DD.jsonl
//! Logs     │ tracing::{info!,warn!,error!}          │ OTEL span events (same trace file)
//! Baggage  │ sdk_rust::attach_context()             │ propagated in-process via Context
//! ```
//!
//! Trace context is propagated from the Python AgentLoop via `_trace_id` / `_span_id`
//! fields injected into tool params by sdk-rust, making every sandbox span a child of
//! the originating Python span for end-to-end distributed traces.

use sdk_rust::{attach_context, ts_ms, MetricsWriter};
pub use sdk_rust::elapsed_ms;
use opentelemetry::{ContextGuard, KeyValue};
use serde_json::{json, Value};

/// OTEL observability facade for the Sandbox service.
///
/// Clone is cheap — [`MetricsWriter`] uses `Arc` internally.
#[derive(Debug, Clone)]
pub struct SandboxTelemetry {
    writer: MetricsWriter,
}

impl SandboxTelemetry {
    /// Create a new telemetry handle; opens (or creates) today's metrics file.
    ///
    /// # Errors
    ///
    /// Returns an error if the log directory cannot be created or the initial
    /// metrics file cannot be opened.
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            writer: MetricsWriter::new(logs_dir, "sandbox")
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        })
    }

    /// Append one metrics data point to today's JSONL file (best-effort).
    pub fn record(&self, data: &Value) {
        self.writer.record(data);
    }

    /// Attach remote trace context from MCP-lite params + baggage key-values.
    ///
    /// Keep the returned [`ContextGuard`] alive for the duration of the span.
    pub fn attach_context(params: &Value, baggage_kvs: Vec<KeyValue>) -> ContextGuard {
        attach_context(params, baggage_kvs)
    }
}

// ── Metric record builders ───────────────────────────────────────────────────

pub fn execute_ok(language: &str, sandbox_name: &str, duration_ms: f64, output_len: usize) -> Value {
    json!({
        "ts_ms":        ts_ms(),
        "service":      "sandbox",
        "op":           "execute",
        "status":       "ok",
        "language":     language,
        "sandbox_name": sandbox_name,
        "duration_ms":  duration_ms,
        "output_len":   output_len,
    })
}

pub fn execute_err(language: &str, sandbox_name: &str, duration_ms: f64) -> Value {
    json!({
        "ts_ms":        ts_ms(),
        "service":      "sandbox",
        "op":           "execute",
        "status":       "error",
        "language":     language,
        "sandbox_name": sandbox_name,
        "duration_ms":  duration_ms,
    })
}

pub fn shell_ok(sandbox_name: &str, duration_ms: f64, output_len: usize) -> Value {
    json!({
        "ts_ms":        ts_ms(),
        "service":      "sandbox",
        "op":           "shell",
        "status":       "ok",
        "sandbox_name": sandbox_name,
        "duration_ms":  duration_ms,
        "output_len":   output_len,
    })
}

pub fn shell_err(sandbox_name: &str, duration_ms: f64) -> Value {
    json!({
        "ts_ms":        ts_ms(),
        "service":      "sandbox",
        "op":           "shell",
        "status":       "error",
        "sandbox_name": sandbox_name,
        "duration_ms":  duration_ms,
    })
}
