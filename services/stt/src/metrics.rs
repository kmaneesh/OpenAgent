//! OTEL observability facade for the STT service.
//!
//! ```text
//! Pillar   │ Mechanism                              │ Output
//! ─────────┼────────────────────────────────────────┼──────────────────────────────────────
//! Traces   │ tracing::info_span! → OTEL bridge      │ logs/stt-traces-YYYY-MM-DD.jsonl
//! Metrics  │ SttTelemetry::record()                 │ logs/stt-metrics-YYYY-MM-DD.jsonl
//! Logs     │ tracing::{info!,warn!,error!}          │ OTEL span events (same trace file)
//! Baggage  │ sdk_rust::attach_context()             │ propagated in-process via Context
//! ```

use sdk_rust::{attach_context, ts_ms, MetricsWriter};
pub use sdk_rust::elapsed_ms;
use opentelemetry::{ContextGuard, KeyValue};
use serde_json::{json, Value};

/// OTEL observability facade for the STT service.
///
/// Clone is cheap — [`MetricsWriter`] uses `Arc` internally.
#[derive(Debug, Clone)]
pub struct SttTelemetry {
    writer: MetricsWriter,
}

impl SttTelemetry {
    /// Create a new telemetry handle; opens (or creates) today's metrics file.
    ///
    /// # Errors
    ///
    /// Returns an error if the log directory cannot be created or the initial
    /// metrics file cannot be opened.
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            writer: MetricsWriter::new(logs_dir, "stt")
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

pub fn transcribe_ok(path: &str, lang: &str, duration_ms: f64, chars: usize) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "stt",
        "op":          "transcribe",
        "status":      "ok",
        "path":        path,
        "lang":        lang,
        "duration_ms": duration_ms,
        "chars":       chars,
    })
}

pub fn transcribe_err(path: &str, lang: &str, duration_ms: f64) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "stt",
        "op":          "transcribe",
        "status":      "error",
        "path":        path,
        "lang":        lang,
        "duration_ms": duration_ms,
    })
}
