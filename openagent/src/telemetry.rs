/// Telemetry helpers for the openagent runtime.
///
/// Same pattern as `services/sdk-rust/src/telemetry.rs`:
/// - [`MetricsWriter`] — daily JSONL metrics sink
/// - [`ts_ms`] / [`elapsed_ms`] — time helpers
use crate::otel::DailyFileWriter;
use serde_json::Value;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// MetricsWriter
// ---------------------------------------------------------------------------

/// Daily-rotating JSONL writer for openagent metrics data points.
///
/// Each [`record`][MetricsWriter::record] call appends one JSON line to
/// `logs/openagent-metrics-YYYY-MM-DD.jsonl`, rotating on date change.
#[derive(Debug, Clone)]
pub struct MetricsWriter(DailyFileWriter);

impl MetricsWriter {
    pub fn new(logs_dir: &str, service: &str) -> anyhow::Result<Self> {
        DailyFileWriter::new(logs_dir, format!("{service}-metrics"))
            .map(Self)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Append one metrics data point to today's file. Best-effort — silently
    /// ignores I/O errors so a metrics write can never crash the runtime.
    pub fn record(&self, data: &Value) {
        if let Ok(line) = serde_json::to_string(data) {
            let _ = self.0.write_line(&line);
        }
    }
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Current Unix timestamp in milliseconds.
#[must_use]
pub fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Elapsed time since `start`, rounded to one decimal millisecond.
#[must_use]
pub fn elapsed_ms(start: Instant) -> f64 {
    (start.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
}
