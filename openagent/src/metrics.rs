/// openagent runtime metric shapes.
///
/// Each function returns a `serde_json::Value` suitable for `MetricsWriter::record`.
/// All metrics include `ts_ms`, `service = "openagent"`, and `op`.
use crate::telemetry::{elapsed_ms, ts_ms};
use serde_json::{json, Value};
use std::time::Instant;

/// Metric for a completed `POST /step` call.
pub fn step_metric(
    platform: &str,
    channel_id: &str,
    session_id: &str,
    status: &str, // "ok" | "error"
    started: Instant,
) -> Value {
    json!({
        "ts_ms":      ts_ms(),
        "service":    "openagent",
        "op":         "step",
        "platform":   platform,
        "channel_id": channel_id,
        "session_id": session_id,
        "status":     status,
        "duration_ms": elapsed_ms(started),
    })
}

/// Metric for a guard check result.
pub fn guard_metric(
    platform: &str,
    channel_id: &str,
    outcome: &str, // "allowed" | "blocked" | "web_bypass" | "unavailable"
    started: Instant,
) -> Value {
    json!({
        "ts_ms":      ts_ms(),
        "service":    "openagent",
        "op":         "guard.check",
        "platform":   platform,
        "channel_id": channel_id,
        "outcome":    outcome,
        "duration_ms": elapsed_ms(started),
    })
}

/// Metric for an STT transcription in `stt_middleware`.
pub fn stt_metric(
    audio_path: &str,
    status: &str, // "ok" | "error" | "empty"
    started: Instant,
) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "openagent",
        "op":          "stt.transcribe",
        "audio_path":  audio_path,
        "status":      status,
        "duration_ms": elapsed_ms(started),
    })
}

/// Metric for a TTS synthesis in `tts_middleware`.
pub fn tts_metric(
    session_id: &str,
    status: &str, // "ok" | "error" | "skipped"
    started: Instant,
) -> Value {
    json!({
        "ts_ms":       ts_ms(),
        "service":     "openagent",
        "op":          "tts.synthesize",
        "session_id":  session_id,
        "status":      status,
        "duration_ms": elapsed_ms(started),
    })
}

/// Metric for a generic `POST /tool/:name` call.
pub fn tool_metric(
    tool: &str,
    status: &str, // "ok" | "error"
    started: Instant,
) -> Value {
    json!({
        "ts_ms":      ts_ms(),
        "service":    "openagent",
        "op":         "tool.call",
        "tool":       tool,
        "status":     status,
        "duration_ms": elapsed_ms(started),
    })
}
