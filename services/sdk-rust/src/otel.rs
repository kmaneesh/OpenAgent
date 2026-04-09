//! OTEL tracing, logging, and metrics for Rust MCP-lite services.
//!
//! Each service calls [`setup_otel`] at startup to initialise **three** OTEL
//! providers that write to daily-rotating JSONL files under `logs_dir`:
//!
//! | Pillar   | File pattern                           | Destination              |
//! |----------|----------------------------------------|--------------------------|
//! | Traces   | `<svc>-traces-YYYY-MM-DD.jsonl`        | file + OTLP (if set)     |
//! | Logs     | `<svc>-logs-YYYY-MM-DD.jsonl`          | file + OTLP (if set)     |
//! | Metrics  | `<svc>-metrics-YYYY-MM-DD.jsonl`       | file only                |
//!
//! `tracing` macros (`info!`, `warn!`, `error!`, `debug!`) are bridged to
//! both the OTEL `LoggerProvider` (structured OTLP) **and** a human-readable
//! fmt sink on stderr.  Spans are bridged to the `TracerProvider` as before.
//!
//! If `OTEL_EXPORTER_OTLP_ENDPOINT` is set (e.g. `http://localhost:4318`),
//! traces **and** logs are also exported via OTLP/HTTP to Jaeger / any
//! compatible collector.  File export always runs regardless.
//!
//! # Usage
//! ```ignore
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let _otel = sdk_rust::setup_otel("my-svc", "logs")
//!         .inspect_err(|e| eprintln!("otel init failed: {e}"))
//!         .ok();  // hold until end of main — drops flush all three providers
//!
//!     info!("service started");   // → logs file + stderr + OTLP (if configured)
//!     // … instrument spans with #[tracing::instrument] or tracing::span!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use futures_util::future::BoxFuture;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::trace::Status as TraceStatus;
use opentelemetry_sdk::{
    export::{
        logs::{LogBatch, LogExporter},
        trace::{ExportResult, SpanData, SpanExporter},
    },
    logs::{LogResult, LoggerProvider},
    metrics::{
        data::{Gauge, Histogram, ResourceMetrics, Sum},
        exporter::PushMetricExporter,
        MetricResult, PeriodicReader, SdkMeterProvider, Temporality,
    },
    runtime,
    trace::TracerProvider,
    Resource,
};
use opentelemetry::KeyValue;
use serde_json::{json, Value};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// ---------------------------------------------------------------------------
// Daily rotating file writer
// ---------------------------------------------------------------------------

/// Thread-safe file writer that rotates daily and keeps 1 day of logs.
///
/// `Clone` is a shallow clone — the underlying file handle is shared.
#[derive(Clone)]
pub struct DailyFileWriter {
    logs_dir: PathBuf,
    prefix: String,
    inner: Arc<Mutex<DailyWriterInner>>,
}

impl std::fmt::Debug for DailyFileWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DailyFileWriter")
            .field("logs_dir", &self.logs_dir)
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct DailyWriterInner {
    pub(crate) file: File,
    pub(crate) current_date: String,
}

impl DailyFileWriter {
    /// Create a new daily-rotating file writer under `logs_dir`.
    pub fn new(logs_dir: impl Into<PathBuf>, prefix: impl Into<String>) -> crate::Result<Self> {
        let logs_dir = logs_dir.into();
        let prefix = prefix.into();
        fs::create_dir_all(&logs_dir)?;
        let today = today_str();
        let file = open_file(&logs_dir, &prefix, &today)?;
        Ok(Self {
            logs_dir,
            prefix,
            inner: Arc::new(Mutex::new(DailyWriterInner {
                file,
                current_date: today,
            })),
        })
    }

    /// Append a line to today's log file, rotating if the date has changed.
    pub fn write_line(&self, line: &str) -> crate::Result<()> {
        let mut guard = self.inner.lock().expect("log file mutex poisoned");
        let today = today_str();
        if guard.current_date != today {
            let new_file = open_file(&self.logs_dir, &self.prefix, &today)?;
            guard.file = new_file;
            guard.current_date = today.clone();
            self.purge_old(&today);
        }
        writeln!(guard.file, "{line}")?;
        guard.file.flush()?;
        Ok(())
    }

    fn purge_old(&self, today: &str) {
        let Ok(entries) = fs::read_dir(&self.logs_dir) else {
            return;
        };
        let prefix_dash = format!("{}-", self.prefix);
        let today_dt = approx_date(today);
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(&prefix_dash) {
                continue;
            }
            let rest = &name[prefix_dash.len()..];
            if rest.len() < 10 {
                continue;
            }
            let date_str = &rest[..10];
            let file_dt = approx_date(date_str);
            if let (Some(t), Some(f)) = (today_dt, file_dt) {
                if t > f + 1 {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

fn open_file(dir: &PathBuf, prefix: &str, date: &str) -> std::io::Result<File> {
    let path = dir.join(format!("{prefix}-{date}.jsonl"));
    OpenOptions::new().create(true).append(true).open(path)
}

fn today_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86_400) as i64;
    let (y, m, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn approx_date(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let d: u64 = parts[2].parse().ok()?;
    Some(y * 365 + m * 30 + d)
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Convert days since Unix epoch (1970-01-01 UTC) to a real Gregorian date.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}

// ---------------------------------------------------------------------------
// Trace file exporter  (unchanged behaviour, uses DailyFileWriter directly)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileSpanExporter {
    inner: Arc<Mutex<DailyWriterInner>>,
    logs_dir: PathBuf,
    prefix: String,
    service_name: String,
}

impl SpanExporter for FileSpanExporter {
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
        let svc = self.service_name.clone();
        let lines: Vec<String> = batch.iter().map(|s| serialize_span(s, &svc)).collect();
        let inner = self.inner.clone();
        let logs_dir = self.logs_dir.clone();
        let prefix = self.prefix.clone();

        Box::pin(async move {
            let mut guard = inner.lock().expect("trace file mutex poisoned");
            let today = today_str();
            if guard.current_date != today {
                match open_file(&logs_dir, &prefix, &today) {
                    Ok(f) => {
                        guard.file = f;
                        guard.current_date = today;
                    }
                    Err(e) => {
                        return Err(opentelemetry::trace::TraceError::from(e.to_string()))
                    }
                }
            }
            for line in &lines {
                if let Err(e) = writeln!(guard.file, "{}", line) {
                    return Err(opentelemetry::trace::TraceError::from(e.to_string()));
                }
            }
            let _ = guard.file.flush();
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// Log file exporter  (new: routes OTEL log records to daily JSONL)
// ---------------------------------------------------------------------------

/// Exports OTEL log records (bridged from `tracing` macros) to a daily JSONL.
///
/// Each line is an OTLP-envelope JSON record matching the log signal shape.
#[derive(Debug)]
struct FileLogExporter {
    writer: DailyFileWriter,
    /// Actual service name for the OTLP `resource.service.name` attribute.
    /// The OTEL SDK provides the instrumentation library name (scope), not the
    /// service name, so we carry it explicitly.
    service_name: String,
}

#[async_trait]
impl LogExporter for FileLogExporter {
    async fn export(&mut self, batch: LogBatch<'_>) -> LogResult<()> {
        for (record, scope) in batch.iter() {
            let line = serialize_log_record(record, scope, &self.service_name);
            let _ = self.writer.write_line(&line);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Metrics file exporter  (new: routes OTEL metric data points to daily JSONL)
// ---------------------------------------------------------------------------

/// Exports OTEL metric data points to a daily-rotating JSONL file.
///
/// Uses `PushMetricExporter` so services can record counters/histograms via
/// `opentelemetry::global::meter("my-svc").u64_counter("requests").init()`.
#[derive(Debug)]
struct FileMetricsExporter {
    writer: DailyFileWriter,
}

#[async_trait]
impl PushMetricExporter for FileMetricsExporter {
    async fn export(&self, metrics: &mut ResourceMetrics) -> MetricResult<()> {
        let line = serialize_metrics(metrics);
        let _ = self.writer.write_line(&line);
        Ok(())
    }

    async fn force_flush(&self) -> MetricResult<()> {
        Ok(())
    }

    fn shutdown(&self) -> MetricResult<()> {
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

// ---------------------------------------------------------------------------
// OTEL setup
// ---------------------------------------------------------------------------

/// Guard returned by [`setup_otel`].
///
/// **Must be held for the lifetime of the process** — typically `let _otel =
/// setup_otel(...).ok()` at the top of `main`.  Dropping it flushes and
/// shuts down all three providers (traces, logs, metrics).
pub struct OTELGuard {
    tracer_provider: TracerProvider,
    logger_provider: LoggerProvider,
    meter_provider: SdkMeterProvider,
}

impl OTELGuard {
    /// Returns a [`opentelemetry::metrics::Meter`] from the service's
    /// `SdkMeterProvider`.  Prefer `opentelemetry::global::meter(name)` for
    /// convenience; both refer to the same underlying provider.
    pub fn meter(&self, name: &'static str) -> opentelemetry::metrics::Meter {
        use opentelemetry::metrics::MeterProvider as _;
        self.meter_provider.meter(name)
    }
}

impl fmt::Debug for OTELGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OTELGuard").finish_non_exhaustive()
    }
}

impl Drop for OTELGuard {
    fn drop(&mut self) {
        // Flush traces
        for result in self.tracer_provider.force_flush() {
            if let Err(e) = result {
                eprintln!("otel tracer flush error: {e}");
            }
        }
        // Flush logs
        for result in self.logger_provider.force_flush() {
            if let Err(e) = result {
                eprintln!("otel logger flush error: {e}");
            }
        }
        // Flush metrics
        if let Err(e) = self.meter_provider.force_flush() {
            eprintln!("otel meter flush error: {e}");
        }
        let _ = self.logger_provider.shutdown();
        let _ = self.meter_provider.shutdown();
    }
}

/// Probe whether an OTLP collector is reachable at `endpoint`.
/// Prevents BatchSpanProcessor.Flush.ExportError spam when the env var is set
/// but no collector is actually running. Uses a 500 ms blocking TCP connect —
/// called once at startup.
fn otlp_reachable(endpoint: &str) -> bool {
    let without_scheme = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    let addr = if host_port.contains(':') {
        host_port.to_owned()
    } else {
        format!("{host_port}:4318")
    };
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = addr.to_socket_addrs() else { return false };
    let Some(sock) = addrs.next() else { return false };
    std::net::TcpStream::connect_timeout(&sock, std::time::Duration::from_millis(500)).is_ok()
}

/// Initialise all three OTEL providers and bridge `tracing` macros into them.
///
/// Returns an [`OTELGuard`] that must be held for the process lifetime.
///
/// # Errors
/// Returns [`crate::Error::OtelSetup`] if the log directory cannot be created,
/// a file cannot be opened, or the subscriber cannot be installed.
pub fn setup_otel(service_name: &str, logs_dir: &str) -> crate::Result<OTELGuard> {
    setup_otel_inner(service_name, logs_dir)
        .map_err(|e| crate::Error::OtelSetup(e.to_string()))
}

#[allow(clippy::too_many_lines)] // intentional: single coherent setup function
fn setup_otel_inner(service_name: &str, logs_dir: &str) -> anyhow::Result<OTELGuard> {
    let logs_path = PathBuf::from(logs_dir);
    fs::create_dir_all(&logs_path)?;

    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.to_owned()),
        KeyValue::new("telemetry.sdk.language", "rust"),
    ]);

    // -----------------------------------------------------------------------
    // Traces
    // -----------------------------------------------------------------------
    let trace_prefix = format!("{service_name}-traces");
    let today = today_str();
    let trace_file = open_file(&logs_path, &trace_prefix, &today)?;
    let trace_inner = Arc::new(Mutex::new(DailyWriterInner {
        file: trace_file,
        current_date: today.clone(),
    }));
    let file_span_exporter = FileSpanExporter {
        inner: trace_inner,
        logs_dir: logs_path.clone(),
        prefix: trace_prefix,
        service_name: service_name.to_owned(),
    };

    let mut trace_builder = TracerProvider::builder()
        .with_batch_exporter(file_span_exporter, runtime::Tokio)
        .with_resource(resource.clone());

    // Probe once — skip OTLP entirely if the collector is not reachable.
    let otlp_ep = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|ep| {
            let ok = otlp_reachable(ep);
            if !ok { eprintln!("OTEL: collector at {ep} unreachable — file-only export"); }
            ok
        });

    if let Some(ref ep) = otlp_ep {
        use opentelemetry_otlp::WithExportConfig as _;
        let url = format!("{}/v1/traces", ep.trim_end_matches('/'));
        match opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(url)
            .build()
        {
            Ok(otlp) => {
                trace_builder = trace_builder.with_batch_exporter(otlp, runtime::Tokio);
            }
            Err(e) => eprintln!("OTLP span exporter init failed: {e}"),
        }
    }

    let tracer_provider = trace_builder.build();
    let tracer = tracer_provider.tracer(service_name.to_owned());

    // -----------------------------------------------------------------------
    // Logs  (FileLogExporter + optional OTLP)
    // -----------------------------------------------------------------------
    let log_writer =
        DailyFileWriter::new(logs_path.clone(), format!("{service_name}-logs"))?;
    let file_log_exporter = FileLogExporter { writer: log_writer, service_name: service_name.to_owned() };

    let mut log_builder = LoggerProvider::builder()
        .with_batch_exporter(file_log_exporter, runtime::Tokio)
        .with_resource(resource.clone());

    if let Some(ref ep) = otlp_ep {
        use opentelemetry_otlp::WithExportConfig as _;
        let url = format!("{}/v1/logs", ep.trim_end_matches('/'));
        match opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_endpoint(url)
            .build()
        {
            Ok(otlp) => {
                log_builder = log_builder.with_batch_exporter(otlp, runtime::Tokio);
            }
            Err(e) => eprintln!("OTLP log exporter init failed: {e}"),
        }
    }

    let logger_provider = log_builder.build();

    // -----------------------------------------------------------------------
    // Metrics  (FileMetricsExporter via PeriodicReader)
    // -----------------------------------------------------------------------
    let metrics_writer =
        DailyFileWriter::new(logs_path.clone(), format!("{service_name}-metrics"))?;
    let file_metrics_exporter = FileMetricsExporter { writer: metrics_writer };

    let meter_provider = SdkMeterProvider::builder()
        .with_reader(
            PeriodicReader::builder(file_metrics_exporter, runtime::Tokio).build(),
        )
        .with_resource(resource)
        .build();

    // Make the meter provider globally accessible so services can call
    // `opentelemetry::global::meter("my-svc")` without passing OTELGuard.
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    // -----------------------------------------------------------------------
    // tracing subscriber
    //
    // Layers (in order of evaluation):
    //   1. EnvFilter          — level gating (RUST_LOG, default INFO)
    //   2. OpenTelemetryLayer — tracing spans → TracerProvider → OTLP JSONL file
    //   3. OtelTracingBridge  — tracing events → LoggerProvider → OTLP JSONL file
    //   4. fmt stderr layer   — human-readable output for journald / Docker / dev
    //
    // The tracing-appender rolling JSON sink is intentionally absent — it
    // produced a second, non-OTEL file (<svc>-logs.YYYY-MM-DD) with the same
    // data. The FileLogExporter writes the canonical OTLP JSONL file instead.
    // -----------------------------------------------------------------------
    let otel_log_bridge =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{}=debug,sdk_rust=debug,info",
            service_name.replace('-', "_")
        ))
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(OpenTelemetryLayer::new(tracer))
        .with(otel_log_bridge)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .try_init()
        .ok(); // "already set" on repeated calls in tests — not an error

    Ok(OTELGuard {
        tracer_provider,
        logger_provider,
        meter_provider,
    })
}

// ---------------------------------------------------------------------------
// Trace context extraction from MCP-lite frames
// ---------------------------------------------------------------------------

/// Extract a remote span context from trace_id / span_id hex strings
/// propagated in a MCP-lite `ToolCallRequest` frame.
pub fn context_from_ids(trace_id_hex: &str, span_id_hex: &str) -> Option<opentelemetry::Context> {
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
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
        true,
        TraceState::default(),
    );
    Some(opentelemetry::Context::new().with_remote_span_context(sc))
}

// ---------------------------------------------------------------------------
// Serialisation helpers — traces
// ---------------------------------------------------------------------------

fn span_to_value(span: &SpanData) -> Value {
    let ctx = &span.span_context;
    let trace_id = format!("{:032x}", ctx.trace_id());
    let span_id = format!("{:016x}", ctx.span_id());
    let parent_span_id = if span.parent_span_id != opentelemetry::trace::SpanId::INVALID {
        format!("{:016x}", span.parent_span_id)
    } else {
        String::new()
    };

    let attrs: Vec<Value> = span
        .attributes
        .iter()
        .map(|kv| json!({"key": kv.key.as_str(), "value": otel_kv_to_json(&kv.value)}))
        .collect();

    let events: Vec<Value> = span
        .events
        .iter()
        .map(|e| {
            let ev_attrs: Vec<Value> = e
                .attributes
                .iter()
                .map(|kv| json!({"key": kv.key.as_str(), "value": otel_kv_to_json(&kv.value)}))
                .collect();
            json!({
                "timeUnixNano": e.timestamp.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default().as_nanos().to_string(),
                "name": e.name.as_ref(),
                "attributes": ev_attrs,
            })
        })
        .collect();

    let start_ns = span
        .start_time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let end_ns = span
        .end_time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();

    let (status_code, status_msg) = match &span.status {
        TraceStatus::Ok => (1i32, String::new()),
        TraceStatus::Error { description } => (2, description.to_string()),
        _ => (0, String::new()),
    };

    json!({
        "traceId": trace_id,
        "spanId": span_id,
        "parentSpanId": parent_span_id,
        "name": span.name.as_ref(),
        "kind": span.span_kind.clone() as i32,
        "startTimeUnixNano": start_ns,
        "endTimeUnixNano": end_ns,
        "attributes": attrs,
        "events": events,
        "status": { "code": status_code, "message": status_msg },
    })
}

fn serialize_span(span: &SpanData, service_name: &str) -> String {
    // `service_name` comes from setup_otel (the actual service, e.g. "browser").
    // `span.instrumentation_scope.name()` is the library name — use it for the
    // inner scope but NOT for the resource `service.name` attribute.
    let scope_name = span.instrumentation_scope.name();
    let obj = json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [{"key": "service.name", "value": {"stringValue": service_name}}]
            },
            "scopeSpans": [{
                "scope": { "name": scope_name },
                "spans": [span_to_value(span)],
            }]
        }]
    });
    obj.to_string()
}

fn otel_kv_to_json(v: &opentelemetry::Value) -> Value {
    match v {
        opentelemetry::Value::String(s) => json!({ "stringValue": s.as_str() }),
        opentelemetry::Value::Bool(b) => json!({ "boolValue": b }),
        opentelemetry::Value::I64(i) => json!({ "intValue": i.to_string() }),
        opentelemetry::Value::F64(f) => json!({ "doubleValue": f }),
        opentelemetry::Value::Array(_) | &_ => json!({ "stringValue": v.to_string() }),
    }
}

// ---------------------------------------------------------------------------
// Serialisation helpers — logs
// ---------------------------------------------------------------------------

fn serialize_log_record(
    record: &opentelemetry_sdk::logs::LogRecord,
    scope: &opentelemetry::InstrumentationScope,
    service_name: &str,
) -> String {
    use std::time::UNIX_EPOCH;

    let timestamp_ns = record
        .timestamp
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().to_string());

    let observed_ns = record
        .observed_timestamp
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().to_string());

    let severity = record
        .severity_text
        .map(|s| s.to_owned())
        .or_else(|| record.severity_number.map(|n| format!("{n:?}")));

    let body = record.body.as_ref().map(anyvalue_to_json);

    let attrs: Vec<Value> = record
        .attributes_iter()
        .map(|(k, v)| json!({"key": k.as_str(), "value": anyvalue_to_json(v)}))
        .collect();

    let trace_ctx = record.trace_context.as_ref().map(|tc| {
        json!({
            "traceId": format!("{:032x}", tc.trace_id),
            "spanId":  format!("{:016x}", tc.span_id),
            "traceFlags": tc.trace_flags.map(|f| f.to_u8()),
        })
    });

    let obj = json!({
        "resourceLogs": [{
            "resource": { "attributes": [{"key": "service.name", "value": {"stringValue": service_name}}] },
            "scopeLogs": [{
                "scope": { "name": scope.name() },
                "logRecords": [{
                    "timeUnixNano":         timestamp_ns,
                    "observedTimeUnixNano": observed_ns,
                    "severityText":  severity,
                    "body":          body,
                    "attributes":    attrs,
                    "traceContext":  trace_ctx,
                    "eventName":     record.event_name,
                    "target":        record.target.as_deref(),
                }]
            }]
        }]
    });
    obj.to_string()
}

fn anyvalue_to_json(v: &opentelemetry::logs::AnyValue) -> Value {
    use opentelemetry::logs::AnyValue;
    match v {
        AnyValue::Int(i) => json!({ "intValue": i }),
        AnyValue::Double(f) => json!({ "doubleValue": f }),
        AnyValue::String(s) => json!({ "stringValue": s.as_ref() }),
        AnyValue::Boolean(b) => json!({ "boolValue": b }),
        AnyValue::Bytes(b) => json!({ "bytesValue": hex::encode(b.as_slice()) }),
        AnyValue::ListAny(items) => {
            let arr: Vec<Value> = items.iter().map(anyvalue_to_json).collect();
            json!({ "arrayValue": arr })
        }
        AnyValue::Map(m) => {
            let entries: Vec<Value> = m
                .iter()
                .map(|(k, v)| json!({"key": k.as_str(), "value": anyvalue_to_json(v)}))
                .collect();
            json!({ "kvlistValue": entries })
        }
        // non-exhaustive — future AnyValue variants
        _ => json!({ "stringValue": format!("{v:?}") }),
    }
}

// ---------------------------------------------------------------------------
// Serialisation helpers — metrics
// ---------------------------------------------------------------------------

fn serialize_metrics(rm: &ResourceMetrics) -> String {
    let mut scope_arr: Vec<Value> = Vec::new();

    for sm in &rm.scope_metrics {
        let mut metric_arr: Vec<Value> = Vec::new();
        for m in &sm.metrics {
            let data_val = serialize_aggregation(m.data.as_ref());
            metric_arr.push(json!({
                "name":        m.name.as_ref(),
                "description": m.description.as_ref(),
                "unit":        m.unit.as_ref(),
                "data":        data_val,
            }));
        }
        scope_arr.push(json!({
            "scope":   sm.scope.name(),
            "metrics": metric_arr,
        }));
    }

    json!({ "resourceMetrics": [{ "scopeMetrics": scope_arr }] }).to_string()
}

fn serialize_aggregation(
    agg: &dyn opentelemetry_sdk::metrics::data::Aggregation,
) -> Value {
    // Gauge variants
    if let Some(g) = agg.as_any().downcast_ref::<Gauge<f64>>() {
        let pts: Vec<Value> = g
            .data_points
            .iter()
            .map(|dp| json!({"value": dp.value, "attributes": metric_attrs(&dp.attributes)}))
            .collect();
        return json!({"type": "gauge_f64", "dataPoints": pts});
    }
    if let Some(g) = agg.as_any().downcast_ref::<Gauge<i64>>() {
        let pts: Vec<Value> = g
            .data_points
            .iter()
            .map(|dp| json!({"value": dp.value, "attributes": metric_attrs(&dp.attributes)}))
            .collect();
        return json!({"type": "gauge_i64", "dataPoints": pts});
    }
    if let Some(g) = agg.as_any().downcast_ref::<Gauge<u64>>() {
        let pts: Vec<Value> = g
            .data_points
            .iter()
            .map(|dp| json!({"value": dp.value, "attributes": metric_attrs(&dp.attributes)}))
            .collect();
        return json!({"type": "gauge_u64", "dataPoints": pts});
    }
    // Sum (counter) variants
    if let Some(s) = agg.as_any().downcast_ref::<Sum<u64>>() {
        let pts: Vec<Value> = s
            .data_points
            .iter()
            .map(|dp| json!({"value": dp.value, "attributes": metric_attrs(&dp.attributes)}))
            .collect();
        return json!({"type": "sum_u64", "monotonic": s.is_monotonic, "dataPoints": pts});
    }
    if let Some(s) = agg.as_any().downcast_ref::<Sum<i64>>() {
        let pts: Vec<Value> = s
            .data_points
            .iter()
            .map(|dp| json!({"value": dp.value, "attributes": metric_attrs(&dp.attributes)}))
            .collect();
        return json!({"type": "sum_i64", "monotonic": s.is_monotonic, "dataPoints": pts});
    }
    if let Some(s) = agg.as_any().downcast_ref::<Sum<f64>>() {
        let pts: Vec<Value> = s
            .data_points
            .iter()
            .map(|dp| json!({"value": dp.value, "attributes": metric_attrs(&dp.attributes)}))
            .collect();
        return json!({"type": "sum_f64", "monotonic": s.is_monotonic, "dataPoints": pts});
    }
    // Histogram variants
    if let Some(h) = agg.as_any().downcast_ref::<Histogram<f64>>() {
        let pts: Vec<Value> = h
            .data_points
            .iter()
            .map(|dp| {
                json!({
                    "count":        dp.count,
                    "sum":          dp.sum,
                    "bounds":       dp.bounds,
                    "bucketCounts": dp.bucket_counts,
                    "attributes":   metric_attrs(&dp.attributes),
                })
            })
            .collect();
        return json!({"type": "histogram_f64", "dataPoints": pts});
    }
    if let Some(h) = agg.as_any().downcast_ref::<Histogram<u64>>() {
        let pts: Vec<Value> = h
            .data_points
            .iter()
            .map(|dp| {
                json!({
                    "count":        dp.count,
                    "sum":          dp.sum,
                    "bounds":       dp.bounds,
                    "bucketCounts": dp.bucket_counts,
                    "attributes":   metric_attrs(&dp.attributes),
                })
            })
            .collect();
        return json!({"type": "histogram_u64", "dataPoints": pts});
    }

    json!({"type": "unknown"})
}

fn metric_attrs(attrs: &[opentelemetry::KeyValue]) -> Vec<Value> {
    attrs
        .iter()
        .map(|kv| json!({"key": kv.key.as_str(), "value": otel_kv_to_json(&kv.value)}))
        .collect()
}
