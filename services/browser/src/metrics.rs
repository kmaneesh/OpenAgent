//! OTEL observability facade for the Browser service.
//!
//! ```text
//! Pillar   │ Mechanism                              │ Output
//! ─────────┼────────────────────────────────────────┼──────────────────────────────────────
//! Traces   │ tracing::info_span! → OTEL bridge      │ logs/browser-traces-YYYY-MM-DD.jsonl
//! Metrics  │ BrowserTelemetry::record()             │ logs/browser-metrics-YYYY-MM-DD.jsonl
//! Logs     │ tracing::{info!,warn!,error!}          │ OTEL span events (same trace file)
//! Baggage  │ sdk_rust::attach_context()             │ propagated in-process via Context
//! ```
//!
//! Due to the large number of browser tools (37), OTEL instrumentation is applied
//! at the registration layer in `main.rs` via a wrapping macro, rather than in each
//! individual handler. All tools share the same span/metric schema.

use opentelemetry::{ContextGuard, KeyValue};
pub use sdk_rust::elapsed_ms;
use sdk_rust::{attach_context, ts_ms, MetricsWriter};
use serde_json::{json, Value};

/// OTEL observability facade for the Browser service.
///
/// Clone is cheap — [`MetricsWriter`] uses `Arc` internally.
#[derive(Debug, Clone)]
pub struct BrowserTelemetry {
    writer: MetricsWriter,
}

impl BrowserTelemetry {
    /// Create a new telemetry handle; opens (or creates) today's metrics file.
    ///
    /// # Errors
    ///
    /// Returns an error if the log directory cannot be created or the initial
    /// metrics file cannot be opened.
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            writer: MetricsWriter::new(logs_dir, "browser").map_err(|e| anyhow::anyhow!("{e}"))?,
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

// ── Metric record builder ────────────────────────────────────────────────────

pub fn tool_metric(tool: &str, session_id: Option<&str>, status: &str, duration_ms: f64) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "browser",
        "op":          tool,
        "status":      status,
        "session_id":  session_id,
        "duration_ms": duration_ms,
    })
}
