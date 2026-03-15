/// Axum route handlers for the openagent control plane.
///
/// POST /step   — run one Cortex reasoning step (guard-checked by middleware)
/// GET  /health — liveness + registered service names
/// GET  /tools  — all tools registered from all running services
/// POST /tool/:name — raw tool call (internal / debug use)
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;
use tracing::{error, info};

use crate::metrics::{step_metric, tool_metric};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /health
// ---------------------------------------------------------------------------

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let tool_count = state.manager.tools().await.len();
    Json(json!({
        "status": "ok",
        "tool_count": tool_count,
    }))
}

// ---------------------------------------------------------------------------
// GET /tools
// ---------------------------------------------------------------------------

pub async fn list_tools(State(state): State<AppState>) -> impl IntoResponse {
    let tools = state.manager.tools().await;
    let entries: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "service": t.service,
                "name": t.definition.get("name").cloned().unwrap_or_default(),
                "description": t.definition.get("description").cloned().unwrap_or_default(),
            })
        })
        .collect();
    Json(json!({"tools": entries, "count": entries.len()}))
}

// ---------------------------------------------------------------------------
// POST /step
// ---------------------------------------------------------------------------

/// Request body for `POST /step`.
///
/// `platform` + `channel_id` are consumed by the guard middleware before the
/// request reaches this handler; they are still part of the body so the
/// middleware can read them without a separate header convention.
#[derive(Debug, Deserialize)]
pub struct StepRequest {
    /// Platform of the originating message (e.g. "telegram", "discord").
    pub platform: String,
    /// Platform-specific sender/channel identifier.
    pub channel_id: String,
    /// Session identifier — passed through to Cortex for memory continuity.
    pub session_id: String,
    /// The user's message text.
    pub user_input: String,
    /// Optional agent name; Cortex resolves to `default` if omitted.
    pub agent_name: Option<String>,
    /// "generation" (default) or "tool_call".
    pub turn_kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StepResponse {
    pub session_id: String,
    pub agent_name: String,
    pub response_text: String,
    pub provider_kind: String,
    pub model: String,
    pub react_summary: Value,
}

pub async fn step(
    State(state): State<AppState>,
    Json(req): Json<StepRequest>,
) -> impl IntoResponse {
    let started = Instant::now();
    info!(
        platform = %req.platform,
        channel_id = %req.channel_id,
        session_id = %req.session_id,
        user_input_len = req.user_input.len(),
        "openagent.step.start"
    );

    let mut params = json!({
        "session_id": req.session_id,
        "user_input": req.user_input,
    });

    if let Some(name) = &req.agent_name {
        params["agent_name"] = Value::String(name.clone());
    }
    if let Some(kind) = &req.turn_kind {
        params["turn_kind"] = Value::String(kind.clone());
    }

    match state
        .manager
        .call_tool("cortex.step", params, 120_000)
        .await
    {
        Ok(payload) => {
            info!(session_id = %req.session_id, "openagent.step.ok");
            state.metrics.record(&step_metric(&req.platform, &req.channel_id, &req.session_id, "ok", started));
            match serde_json::from_str::<Value>(&payload) {
                Ok(v) => (StatusCode::OK, Json(v)).into_response(),
                Err(e) => {
                    error!(error = %e, "openagent.step.parse_error");
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "response_parse_error", "detail": e.to_string()}))).into_response()
                }
            }
        }
        Err(e) => {
            error!(session_id = %req.session_id, error = %e, "openagent.step.error");
            state.metrics.record(&step_metric(&req.platform, &req.channel_id, &req.session_id, "error", started));
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "step_failed", "detail": e.to_string()}))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// POST /tool/:name  (internal / debug)
// ---------------------------------------------------------------------------

pub async fn call_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let started = Instant::now();
    let params: Value = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid_json", "detail": e.to_string()})),
                )
                    .into_response()
            }
        }
    };

    match state.manager.call_tool(&name, params, 30_000).await {
        Ok(result) => {
            state.metrics.record(&tool_metric(&name, "ok", started));
            let v: Value = serde_json::from_str(&result).unwrap_or(Value::String(result));
            (StatusCode::OK, Json(v)).into_response()
        }
        Err(e) => {
            state.metrics.record(&tool_metric(&name, "error", started));
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}
