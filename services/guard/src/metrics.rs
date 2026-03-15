use opentelemetry::KeyValue;
use sdk_rust::{attach_context, elapsed_ms, ts_ms, MetricsWriter};
use serde_json::{json, Value};
use std::time::Instant;

pub use sdk_rust::elapsed_ms as sdk_elapsed_ms;

#[derive(Debug, Clone)]
pub struct GuardTelemetry {
    writer: MetricsWriter,
}

impl GuardTelemetry {
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            writer: MetricsWriter::new(logs_dir, "guard")
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        })
    }

    pub fn record(&self, data: &Value) {
        self.writer.record(data);
    }

    pub fn attach_context(params: &Value, baggage_kvs: Vec<KeyValue>) -> opentelemetry::ContextGuard {
        attach_context(params, baggage_kvs)
    }
}

pub fn check_metric(
    platform: &str,
    status: &str,   // "allowed" | "blocked" | "web_bypass" | "error"
    started: Instant,
) -> Value {
    json!({
        "ts_ms": ts_ms(),
        "service": "guard",
        "op": "guard.check",
        "platform": platform,
        "status": status,
        "duration_ms": elapsed_ms(started),
    })
}

pub fn write_metric(
    op: &str,       // "guard.add" | "guard.remove" | "guard.list"
    platform: &str,
    status: &str,   // "ok" | "not_found" | "error"
    started: Instant,
) -> Value {
    json!({
        "ts_ms": ts_ms(),
        "service": "guard",
        "op": op,
        "platform": platform,
        "status": status,
        "duration_ms": elapsed_ms(started),
    })
}
