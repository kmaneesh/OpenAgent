use opentelemetry::KeyValue;
use sdk_rust::{attach_context, ts_ms, MetricsWriter};
use serde_json::{json, Value};

pub use sdk_rust::elapsed_ms;

#[derive(Debug, Clone)]
pub struct ValidatorTelemetry {
    writer: MetricsWriter,
}

impl ValidatorTelemetry {
    pub fn new(logs_dir: &str) -> anyhow::Result<Self> {
        Ok(Self {
            writer: MetricsWriter::new(logs_dir, "validator")
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

#[allow(clippy::too_many_arguments)]
pub fn repair_metric(
    mode: &str,
    status: &str,
    was_repaired: bool,
    changed: bool,
    input_len: usize,
    output_len: usize,
    duration_ms: f64,
) -> Value {
    json!({
        "ts_ms": ts_ms(),
        "service": "validator",
        "op": "validator.repair_json",
        "backend": "llm_json",
        "mode": mode,
        "status": status,
        "was_repaired": was_repaired,
        "changed": changed,
        "input_len": input_len,
        "output_len": output_len,
        "duration_ms": duration_ms
    })
}
