//! Shared telemetry utilities for MCP-lite service metrics.
//!
//! Provides [`MetricsWriter`] — a daily-rotating JSONL writer for service-specific
//! metrics — plus time helpers ([`ts_ms`], [`elapsed_ms`]) and [`attach_context`]
//! for propagating remote OTEL trace context from MCP-lite tool parameters.
//!
//! Every Rust service should embed a `{Service}Telemetry` that wraps [`MetricsWriter`]
//! instead of re-implementing the file-rotation and context-propagation logic.

use crate::otel::{context_from_ids, DailyFileWriter};
use opentelemetry::{baggage::BaggageExt as _, Context, ContextGuard, KeyValue};
use serde_json::Value;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// MetricsWriter
// ---------------------------------------------------------------------------

/// Daily-rotating JSONL writer for service metrics data points.
///
/// Each call to [`record`][MetricsWriter::record] appends one JSON object to
/// `<logs_dir>/<service>-metrics-YYYY-MM-DD.jsonl`, rotating the file on date
/// change and purging files older than 1 day.
///
/// `Clone` performs a shallow clone — the underlying file handle is shared.
#[derive(Debug, Clone)]
pub struct MetricsWriter(DailyFileWriter);

impl MetricsWriter {
    /// Create a new writer that appends to `<logs_dir>/<service>-metrics-YYYY-MM-DD.jsonl`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or the initial file
    /// cannot be opened.
    pub fn new(logs_dir: &str, service: &str) -> crate::Result<Self> {
        let prefix = format!("{service}-metrics");
        DailyFileWriter::new(logs_dir, prefix).map(Self)
    }

    /// Append one metrics data point (serialised as JSON) to today's JSONL file.
    ///
    /// Silently ignores serialisation and I/O errors — metrics are best-effort.
    pub fn record(&self, data: &Value) {
        if let Ok(line) = serde_json::to_string(data) {
            let _ = self.0.write_line(&line);
        }
    }
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Returns the current Unix timestamp in milliseconds.
#[must_use]
pub fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Returns elapsed time since `start` in milliseconds, rounded to 1 decimal place.
#[must_use]
pub fn elapsed_ms(start: Instant) -> f64 {
    let raw = start.elapsed().as_secs_f64() * 1000.0;
    (raw * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------
// OTEL context propagation
// ---------------------------------------------------------------------------

/// Attach remote trace context from MCP-lite `_trace_id`/`_span_id` params
/// and optional baggage key-values to the current OTEL context.
///
/// Reads `_trace_id` (32-char hex) and `_span_id` (16-char hex) from `params`.
/// When present, creates a remote parent span context so this service's spans
/// become children of the originating Python `AgentLoop` span, enabling
/// end-to-end distributed traces across the Python ↔ Rust boundary.
///
/// **Keep the returned [`ContextGuard`] alive for the duration of the span** —
/// dropping it restores the previous context.
pub fn attach_context(params: &Value, baggage_kvs: Vec<KeyValue>) -> ContextGuard {
    let mut cx = Context::current();

    if let (Some(tid), Some(sid)) = (
        params.get("_trace_id").and_then(|v| v.as_str()),
        params.get("_span_id").and_then(|v| v.as_str()),
    ) {
        if let Some(remote_cx) = context_from_ids(tid, sid) {
            cx = remote_cx;
        }
    }

    if !baggage_kvs.is_empty() {
        cx = cx.with_baggage(baggage_kvs);
    }

    cx.attach()
}
