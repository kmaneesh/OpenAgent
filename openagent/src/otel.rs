/// OTEL three-pillar observability for the openagent runtime.
///
/// Same pattern as `services/sdk-rust/src/otel.rs` — daily-rotating JSONL files
/// for traces, logs, and metrics under `logs/`.  Optional OTLP/HTTP export via
/// `OTEL_EXPORTER_OTLP_ENDPOINT`.
///
/// Call [`setup_otel`] once at startup and hold the returned [`OTELGuard`] for
/// the process lifetime:
/// ```ignore
/// let _otel = setup_otel("openagent", &logs_dir)
///     .inspect_err(|e| eprintln!("otel init failed: {e}"))
///     .ok();
/// ```
use anyhow::Result;
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
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

#[derive(Debug)]
pub(crate) struct DailyWriterInner {
    pub(crate) file: File,
    pub(crate) current_date: String,
}

/// Thread-safe file writer that rotates daily and purges files older than 1 day.
#[derive(Clone)]
pub struct DailyFileWriter {
    logs_dir: PathBuf,
    prefix: String,
    inner: Arc<Mutex<DailyWriterInner>>,
}

impl fmt::Debug for DailyFileWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DailyFileWriter")
            .field("logs_dir", &self.logs_dir)
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

impl DailyFileWriter {
    pub fn new(logs_dir: impl Into<PathBuf>, prefix: impl Into<String>) -> Result<Self> {
        let logs_dir = logs_dir.into();
        let prefix = prefix.into();
        fs::create_dir_all(&logs_dir)?;
        let today = today_str();
        let file = open_log_file(&logs_dir, &prefix, &today)?;
        Ok(Self {
            logs_dir,
            prefix,
            inner: Arc::new(Mutex::new(DailyWriterInner { file, current_date: today })),
        })
    }

    pub fn write_line(&self, line: &str) -> Result<()> {
        let mut g = self.inner.lock().expect("log file mutex poisoned");
        let today = today_str();
        if g.current_date != today {
            g.file = open_log_file(&self.logs_dir, &self.prefix, &today)?;
            g.current_date = today.clone();
            self.purge_old(&today);
        }
        writeln!(g.file, "{line}")?;
        g.file.flush()?;
        Ok(())
    }

    fn purge_old(&self, today: &str) {
        let Ok(entries) = fs::read_dir(&self.logs_dir) else { return };
        let prefix_dash = format!("{}-", self.prefix);
        let today_dt = approx_date(today);
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(&prefix_dash) { continue; }
            let rest = &name[prefix_dash.len()..];
            if rest.len() < 10 { continue; }
            let file_dt = approx_date(&rest[..10]);
            if let (Some(t), Some(f)) = (today_dt, file_dt) {
                if t > f + 1 { let _ = fs::remove_file(entry.path()); }
            }
        }
    }
}

fn open_log_file(dir: &PathBuf, prefix: &str, date: &str) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{prefix}-{date}.jsonl")))
}

pub(crate) fn today_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let (y, m, d) = days_to_ymd((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

fn approx_date(s: &str) -> Option<u64> {
    let p: Vec<&str> = s.split('-').collect();
    if p.len() != 3 { return None; }
    Some(p[0].parse::<u64>().ok()? * 365 + p[1].parse::<u64>().ok()? * 30 + p[2].parse::<u64>().ok()?)
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m as u32, d as u32)
}

// ---------------------------------------------------------------------------
// Span exporter
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileSpanExporter {
    inner: Arc<Mutex<DailyWriterInner>>,
    logs_dir: PathBuf,
    prefix: String,
}

impl SpanExporter for FileSpanExporter {
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
        let lines: Vec<String> = batch.iter().map(serialize_span).collect();
        let inner = self.inner.clone();
        let logs_dir = self.logs_dir.clone();
        let prefix = self.prefix.clone();
        Box::pin(async move {
            let mut g = inner.lock().expect("trace file mutex poisoned");
            let today = today_str();
            if g.current_date != today {
                match open_log_file(&logs_dir, &prefix, &today) {
                    Ok(f) => { g.file = f; g.current_date = today; }
                    Err(e) => return Err(opentelemetry::trace::TraceError::from(e.to_string())),
                }
            }
            for line in &lines {
                if let Err(e) = writeln!(g.file, "{}", line) {
                    return Err(opentelemetry::trace::TraceError::from(e.to_string()));
                }
            }
            let _ = g.file.flush();
            Ok(())
        })
    }
}

fn serialize_span(span: &SpanData) -> String {
    serde_json::to_string(&json!({
        "trace_id": format!("{:032x}", span.span_context.trace_id()),
        "span_id":  format!("{:016x}", span.span_context.span_id()),
        "name":     span.name.as_ref(),
        "start_ms": span.start_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
        "end_ms":   span.end_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
        "status":   format!("{:?}", span.status),
        "attrs":    span.attributes.iter().map(|kv| json!({kv.key.as_str(): kv.value.as_str()})).collect::<Vec<_>>(),
    })).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Log exporter
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileLogExporter {
    writer: DailyFileWriter,
}

#[async_trait]
impl LogExporter for FileLogExporter {
    async fn export(&mut self, batch: LogBatch<'_>) -> LogResult<()> {
        for (record, _scope) in batch.iter() {
            let body = record.body.as_ref().map_or_else(String::new, |b| format!("{b:?}"));
            let severity = format!("{:?}", record.severity_number);
            let line = serde_json::to_string(&json!({"ts": record.observed_timestamp.map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64), "severity": severity, "body": body})).unwrap_or_default();
            let _ = self.writer.write_line(&line);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Metrics exporter
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileMetricsExporter {
    writer: DailyFileWriter,
}

#[async_trait]
impl PushMetricExporter for FileMetricsExporter {
    async fn export(&self, metrics: &mut ResourceMetrics) -> MetricResult<()> {
        let _ = self.writer.write_line(&serialize_metrics(metrics));
        Ok(())
    }
    async fn force_flush(&self) -> MetricResult<()> { Ok(()) }
    fn shutdown(&self) -> MetricResult<()> { Ok(()) }
    fn temporality(&self) -> Temporality { Temporality::Cumulative }
}

fn serialize_metrics(rm: &ResourceMetrics) -> String {
    let points: Vec<Value> = rm.scope_metrics.iter().flat_map(|sm| {
        sm.metrics.iter().map(|m| {
            let data: Value = if let Some(h) = m.data.as_any().downcast_ref::<Histogram<u64>>() {
                json!({"kind":"histogram","name":m.name,"count":h.data_points.iter().map(|p| p.count).sum::<u64>()})
            } else if let Some(s) = m.data.as_any().downcast_ref::<Sum<u64>>() {
                json!({"kind":"sum","name":m.name,"value":s.data_points.iter().map(|p| p.value).sum::<u64>()})
            } else if let Some(g) = m.data.as_any().downcast_ref::<Gauge<u64>>() {
                json!({"kind":"gauge","name":m.name,"value":g.data_points.last().map_or(0,|p| p.value)})
            } else {
                json!({"kind":"unknown","name":m.name})
            };
            data
        })
    }).collect();
    serde_json::to_string(&json!({"ts_ms": crate::telemetry::ts_ms(), "service": "openagent", "metrics": points})).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// OTELGuard + setup_otel
// ---------------------------------------------------------------------------

pub struct OTELGuard {
    tracer_provider: TracerProvider,
    logger_provider: LoggerProvider,
    meter_provider: SdkMeterProvider,
    _log_guard: tracing_appender::non_blocking::WorkerGuard,
}

impl fmt::Debug for OTELGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OTELGuard").finish_non_exhaustive()
    }
}

impl Drop for OTELGuard {
    fn drop(&mut self) {
        for r in self.tracer_provider.force_flush() { if let Err(e) = r { eprintln!("otel trace flush: {e}"); } }
        for r in self.logger_provider.force_flush() { if let Err(e) = r { eprintln!("otel log flush: {e}"); } }
        if let Err(e) = self.meter_provider.force_flush() { eprintln!("otel meter flush: {e}"); }
        let _ = self.logger_provider.shutdown();
        let _ = self.meter_provider.shutdown();
    }
}

pub fn setup_otel(service_name: &str, logs_dir: &str) -> Result<OTELGuard> {
    let logs_path = PathBuf::from(logs_dir);
    fs::create_dir_all(&logs_path)?;

    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.to_owned()),
        KeyValue::new("telemetry.sdk.language", "rust"),
    ]);

    // ---- Traces -------------------------------------------------------------
    let trace_prefix = format!("{service_name}-traces");
    let today = today_str();
    let trace_file = open_log_file(&logs_path, &trace_prefix, &today)?;
    let trace_inner = Arc::new(Mutex::new(DailyWriterInner { file: trace_file, current_date: today }));
    let mut trace_builder = TracerProvider::builder()
        .with_batch_exporter(FileSpanExporter { inner: trace_inner, logs_dir: logs_path.clone(), prefix: trace_prefix }, runtime::Tokio)
        .with_resource(resource.clone());

    if let Ok(ep) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        use opentelemetry_otlp::WithExportConfig as _;
        let url = format!("{}/v1/traces", ep.trim_end_matches('/'));
        match opentelemetry_otlp::SpanExporter::builder().with_http().with_endpoint(url).build() {
            Ok(otlp) => { trace_builder = trace_builder.with_batch_exporter(otlp, runtime::Tokio); }
            Err(e) => eprintln!("OTLP span exporter init failed: {e}"),
        }
    }
    let tracer_provider = trace_builder.build();
    let tracer = tracer_provider.tracer(service_name.to_owned());

    // ---- Logs ---------------------------------------------------------------
    let log_writer = DailyFileWriter::new(logs_path.clone(), format!("{service_name}-logs"))?;
    let mut log_builder = LoggerProvider::builder()
        .with_batch_exporter(FileLogExporter { writer: log_writer }, runtime::Tokio)
        .with_resource(resource.clone());

    if let Ok(ep) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        use opentelemetry_otlp::WithExportConfig as _;
        let url = format!("{}/v1/logs", ep.trim_end_matches('/'));
        match opentelemetry_otlp::LogExporter::builder().with_http().with_endpoint(url).build() {
            Ok(otlp) => { log_builder = log_builder.with_batch_exporter(otlp, runtime::Tokio); }
            Err(e) => eprintln!("OTLP log exporter init failed: {e}"),
        }
    }
    let logger_provider = log_builder.build();

    // ---- Metrics ------------------------------------------------------------
    let metrics_writer = DailyFileWriter::new(logs_path.clone(), format!("{service_name}-metrics"))?;
    let mut meter_builder = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(FileMetricsExporter { writer: metrics_writer }, runtime::Tokio).build())
        .with_resource(resource.clone());

    // Optional OTLP metrics export (e.g. Jaeger / Prometheus via OTLP collector).
    if let Ok(ep) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        use opentelemetry_otlp::WithExportConfig as _;
        let url = format!("{}/v1/metrics", ep.trim_end_matches('/'));
        match opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(url)
            .with_temporality(Temporality::Cumulative)
            .build()
        {
            Ok(otlp) => {
                meter_builder = meter_builder.with_reader(
                    PeriodicReader::builder(otlp, runtime::Tokio).build(),
                );
            }
            Err(e) => eprintln!("OTLP metric exporter init failed: {e}"),
        }
    }

    let meter_provider = meter_builder.with_resource(resource).build();
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    // ---- tracing subscriber -------------------------------------------------
    let otel_log_bridge = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);
    let file_appender = tracing_appender::rolling::daily(logs_dir, format!("{service_name}-logs"));
    let (non_blocking, log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(OpenTelemetryLayer::new(tracer))
        .with(otel_log_bridge)
        .with(tracing_subscriber::fmt::layer().json().with_writer(non_blocking))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    Ok(OTELGuard { tracer_provider, logger_provider, meter_provider, _log_guard: log_guard })
}
