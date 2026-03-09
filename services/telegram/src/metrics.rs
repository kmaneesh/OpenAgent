//! OTEL observability facade for the Telegram service.
//!
//! ```text
//! Pillar   │ Mechanism                              │ Output
//! ─────────┼────────────────────────────────────────┼──────────────────────────────────────
//! Traces   │ tracing::info_span! → OTEL bridge      │ logs/telegram-traces-YYYY-MM-DD.jsonl
//! Metrics  │ TelegramTelemetry::record()            │ logs/telegram-metrics-YYYY-MM-DD.jsonl
//! Logs     │ tracing::{info!,warn!,error!}          │ OTEL span events (same trace file)
//! Baggage  │ sdk_rust::attach_context()             │ propagated in-process via Context
//! ```

use sdk_rust::{attach_context, ts_ms, MetricsWriter};
pub use sdk_rust::elapsed_ms;
use opentelemetry::{ContextGuard, KeyValue};
use serde_json::{json, Value};

/// OTEL observability facade for the Telegram service.
///
/// Clone is cheap — [`MetricsWriter`] uses `Arc` internally.
#[derive(Debug, Clone)]
pub struct TelegramTelemetry {
    writer: MetricsWriter,
}

impl TelegramTelemetry {
    /// Create a new telemetry handle; opens (or creates) today's metrics file.
    ///
    /// # Errors
    ///
    /// Returns an error if the log directory cannot be created or the initial
    /// metrics file cannot be opened.
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            writer: MetricsWriter::new(logs_dir, "telegram")
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

pub fn send_ok(user_id: i64, duration_ms: f64) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "telegram",
        "op":          "send_message",
        "status":      "ok",
        "user_id":     user_id,
        "duration_ms": duration_ms,
    })
}

pub fn send_err(user_id: i64, duration_ms: f64) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "telegram",
        "op":          "send_message",
        "status":      "error",
        "user_id":     user_id,
        "duration_ms": duration_ms,
    })
}
