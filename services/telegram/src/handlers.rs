//! Tool handler implementations: telegram.send_message.
//!
//! Wires all four OTEL pillars via TelegramTelemetry:
//!   Traces  — tracing::info_span! with per-operation attributes
//!   Metrics — TelegramTelemetry::record() on success and error
//!   Logs    — structured tracing::{info!, error!} events on every path
//!   Baggage — attach_context() propagates remote parent + tool/user tags

use crate::metrics::{elapsed_ms, send_err, send_ok, TelegramTelemetry};
use crate::state::TelegramState;
use anyhow::Result;
use opentelemetry::KeyValue;
use serde_json::Value;
use std::sync::{atomic::Ordering, Arc};
use std::time::Instant;
use teloxide::{prelude::Requester, types::ChatId};
use tokio::runtime::Handle;
use tracing::{error, info, info_span};

/// Parse a Telegram user/chat ID from a JSON value.
///
/// Accepts both number (`123456`) and string (`"123456"`) forms since
/// some platforms serialise large integers as strings to avoid precision loss.
pub fn parse_i64(value: &Value) -> Result<i64> {
    match value {
        Value::Number(n) => n.as_i64().ok_or_else(|| anyhow::anyhow!("invalid number")),
        Value::String(s) => s.parse().map_err(|e| anyhow::anyhow!("invalid user_id: {e}")),
        _ => anyhow::bail!("user_id must be number or string"),
    }
}

pub fn handle_send_message(
    params: Value,
    state: Arc<TelegramState>,
    tel: Arc<TelegramTelemetry>,
) -> Result<String> {
    let user_id = params
        .get("user_id")
        .ok_or_else(|| anyhow::anyhow!("user_id is required"))?;
    let user_id = parse_i64(user_id)?;

    let text = params["text"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("text is required"))?
        .to_string();
    if !state.connected.load(Ordering::Acquire) {
        anyhow::bail!("telegram not connected");
    }

    // ── Pillar: Baggage ───────────────────────────────────────────────────────
    let _cx_guard = TelegramTelemetry::attach_context(
        &params,
        vec![
            KeyValue::new("tool", "telegram.send_message"),
            KeyValue::new("user_id", user_id.to_string()),
        ],
    );

    // ── Pillar: Traces ────────────────────────────────────────────────────────
    let span = info_span!(
        "telegram.send_message",
        user_id = user_id,
        duration_ms = tracing::field::Empty,
        status = tracing::field::Empty,
    );
    let _enter = span.enter();

    // ── Pillar: Logs ─────────────────────────────────────────────────────────
    info!(user_id, "telegram.send_message start");

    let bot = state.bot.lock().expect("bot poisoned").clone();
    let bot = bot.ok_or_else(|| anyhow::anyhow!("telegram not connected"))?;

    let t_start = Instant::now();
    let result: Result<_, teloxide::RequestError> = tokio::task::block_in_place(|| {
        Handle::current()
            .block_on(async { bot.send_message(ChatId(user_id), &text).await })
    });
    let duration_ms = elapsed_ms(t_start);

    // ── Pillar: Traces + Logs + Metrics ──────────────────────────────────────
    match result {
        Ok(_) => {
            span.record("duration_ms", duration_ms);
            span.record("status", "ok");
            info!(user_id, duration_ms, "telegram.send_message ok");
            tel.record(&send_ok(user_id, duration_ms));
            Ok(serde_json::json!({ "ok": true, "user_id": user_id }).to_string())
        }
        Err(e) => {
            span.record("duration_ms", duration_ms);
            span.record("status", "error");
            error!(user_id, duration_ms, error = %e, "telegram.send_message error");
            state.set_error(&e.to_string());
            state.emit_connection_status();
            tel.record(&send_err(user_id, duration_ms));
            Err(anyhow::anyhow!("{e}"))
        }
    }
}
