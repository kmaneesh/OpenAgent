//! OTEL observability — all four pillars — for the sandbox service.
//!
//! ```text
//! Pillar   │ Mechanism                              │ Output
//! ─────────┼────────────────────────────────────────┼─────────────────────────────────────
//! Traces   │ tracing::info_span! → OTEL bridge      │ logs/sandbox-traces-YYYY-MM-DD.jsonl
//! Metrics  │ SandboxTelemetry::record()             │ logs/sandbox-metrics-YYYY-MM-DD.jsonl
//! Logs     │ tracing::{info!,warn!,error!}          │ OTEL span events (same trace file)
//! Baggage  │ opentelemetry::Context + BaggageExt    │ propagated in-process via Context
//! ```
//!
//! Trace context is propagated from the Python AgentLoop via `_trace_id` / `_span_id`
//! fields that sdk-rust injects into tool params from the MCP-lite `ToolCallRequest`.
//! This makes every sandbox span a child of the originating Python span, giving end-to-end
//! distributed traces across the Python ↔ Rust boundary.

use opentelemetry::{baggage::BaggageExt as _, Context, ContextGuard, KeyValue};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

// ---------------------------------------------------------------------------
// SandboxTelemetry — OTEL observability facade
// ---------------------------------------------------------------------------

/// Combines all four OTEL pillars into a single cloneable handle.
///
/// - Clone is cheap (Arc under the hood).
/// - Create once at startup; share via `Arc::clone` into each tool handler.
#[derive(Debug, Clone)]
pub struct SandboxTelemetry {
    inner: Arc<TelemetryInner>,
}

#[derive(Debug)]
struct TelemetryInner {
    logs_dir: PathBuf,
    state: Mutex<MetricsState>,
}

#[derive(Debug)]
struct MetricsState {
    file: File,
    current_date: String,
}

impl SandboxTelemetry {
    /// Create a new telemetry handle.  Opens (or creates) the metrics file for today.
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        let dir = PathBuf::from(logs_dir);
        fs::create_dir_all(&dir)?;
        let today = today_date();
        let file = open_metric_file(&dir, &today)?;
        Ok(Self {
            inner: Arc::new(TelemetryInner {
                logs_dir: dir,
                state: Mutex::new(MetricsState { file, current_date: today }),
            }),
        })
    }

    // ── Pillar: Metrics ──────────────────────────────────────────────────────

    /// Append one OTEL-compatible metrics data point to the daily JSONL file.
    ///
    /// Each line has OTEL-style fields: `ts_ms`, `service`, `op`, `status`,
    /// plus tool-specific attributes (`language`, `duration_ms`, `output_len`, …).
    pub fn record(&self, data: &Value) {
        let mut guard = self.inner.state.lock().expect("metrics mutex poisoned");
        let today = today_date();
        if guard.current_date != today {
            match open_metric_file(&self.inner.logs_dir, &today) {
                Ok(f) => {
                    guard.file = f;
                    guard.current_date = today;
                }
                Err(e) => {
                    eprintln!("metric file rotate error: {e}");
                    return;
                }
            }
        }
        if let Ok(line) = serde_json::to_string(data) {
            let _ = writeln!(guard.file, "{line}");
            let _ = guard.file.flush();
        }
    }

    // ── Pillar: Baggage + Traces (context propagation) ───────────────────────

    /// Build an OTEL `Context` from optional MCP-lite trace propagation fields
    /// (`_trace_id` / `_span_id`) in the tool params, attach caller-supplied
    /// baggage key-values, and install it as the current context.
    ///
    /// **Keep the returned [`ContextGuard`] alive for the duration of the
    /// span** — dropping it restores the previous context.
    ///
    /// ## Trace propagation (Pillar: Traces)
    ///
    /// sdk-rust injects `_trace_id` and `_span_id` into tool params from the
    /// MCP-lite `ToolCallRequest`.  When present, this service's spans become
    /// children of the Python AgentLoop span, enabling end-to-end traces
    /// across the Python ↔ Rust boundary.
    ///
    /// ## Baggage (Pillar: Baggage)
    ///
    /// Baggage key-values are attached to the context and readable by any
    /// code downstream in the same async task (via `Context::current()`).
    /// Typical baggage: `("tool", "sandbox.execute")`, `("language", "python")`.
    pub fn attach_context(params: &Value, baggage_kvs: Vec<KeyValue>) -> ContextGuard {
        // Start from the current ambient context (may already carry OTEL state).
        let mut cx = Context::current();

        // ── Remote parent span from Python AgentLoop ─────────────────────────
        if let (Some(tid), Some(sid)) = (
            params.get("_trace_id").and_then(|v| v.as_str()),
            params.get("_span_id").and_then(|v| v.as_str()),
        ) {
            if let Some(remote_cx) = remote_context_from_ids(tid, sid) {
                cx = remote_cx;
            }
        }

        // ── Baggage ──────────────────────────────────────────────────────────
        if !baggage_kvs.is_empty() {
            cx = cx.with_baggage(baggage_kvs);
        }

        cx.attach()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open (or create, append) the daily metric JSONL file.
fn open_metric_file(dir: &PathBuf, date: &str) -> anyhow::Result<File> {
    let path = dir.join(format!("sandbox-metric-{date}.jsonl"));
    Ok(OpenOptions::new().create(true).append(true).open(path)?)
}

/// Current Unix timestamp in milliseconds — used as the OTEL `ts_ms` field.
pub fn ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Elapsed milliseconds since `start` — rounds to 1 decimal place.
pub fn elapsed_ms(start: Instant) -> f64 {
    let raw = start.elapsed().as_secs_f64() * 1000.0;
    (raw * 10.0).round() / 10.0
}

/// `YYYY-MM-DD` string for today (no chrono dependency).
fn today_date() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = secs / 86400;
    let y = 1970 + days / 365;
    let rem = days % 365;
    let m = (1 + rem / 30).min(12);
    let d = (1 + rem % 30).min(28);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Reconstruct a remote OTEL `Context` from hex-encoded `trace_id` (32 chars)
/// and `span_id` (16 chars) strings propagated in the MCP-lite frame.
///
/// Returns `None` if either string is malformed.
fn remote_context_from_ids(trace_id_hex: &str, span_id_hex: &str) -> Option<Context> {
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt as _, TraceFlags, TraceId, TraceState,
    };

    if trace_id_hex.len() != 32 || span_id_hex.len() != 16 {
        return None;
    }
    let tid_bytes = hex::decode(trace_id_hex).ok()?;
    let sid_bytes = hex::decode(span_id_hex).ok()?;
    if tid_bytes.len() != 16 || sid_bytes.len() != 8 {
        return None;
    }
    let mut tid_arr = [0u8; 16];
    let mut sid_arr = [0u8; 8];
    tid_arr.copy_from_slice(&tid_bytes);
    sid_arr.copy_from_slice(&sid_bytes);

    let sc = SpanContext::new(
        TraceId::from_bytes(tid_arr),
        SpanId::from_bytes(sid_arr),
        TraceFlags::SAMPLED,
        true, // remote parent
        TraceState::default(),
    );
    Some(Context::new().with_remote_span_context(sc))
}

// ---------------------------------------------------------------------------
// Convenience builders (used by handlers.rs)
// ---------------------------------------------------------------------------

/// Build a metrics record for a successful `sandbox.execute` invocation.
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

/// Build a metrics record for a failed `sandbox.execute` invocation.
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

/// Build a metrics record for a successful `sandbox.shell` invocation.
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

/// Build a metrics record for a failed `sandbox.shell` invocation.
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
