/// Tower middleware for the openagent control plane.
///
/// GuardLayer — inline whitelist check before every `/step` request.
///   Uses GuardDb (direct SQLite, no network hop) for a fast, deterministic check.
///   Allowed  → passes through to Cortex.
///   Blocked  → returns HTTP 403 with JSON error body.
///   DB error → fails open with a warning (a DB error should not brick the platform).
///
/// SttLayer, TtsLayer — same pattern, registered after GuardLayer.
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};
use std::time::Instant;
use tracing::{info, warn};

use crate::metrics::guard_metric;
use crate::scrub;
use crate::state::AppState;

/// Axum `from_fn_with_state` middleware that enforces the Guard whitelist.
///
/// Reads the request body once, parses `platform` + `channel_id`, calls
/// `guard_db.check()` (direct SQLite — no TCP), then reconstructs the request
/// with the buffered bytes before handing it to the next layer.
pub async fn guard_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let (parts, body) = req.into_parts();

    // Buffer the full body — needed so we can read platform/channel_id and
    // still pass the bytes through to the route handler.
    let mut bytes = match axum::body::to_bytes(body, 4 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "guard.body.read.error");
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"error": "body_read_error"})),
            )
                .into_response();
        }
    };

    let guard_started = Instant::now();

    // Only run the check if the body parses as a JSON object with platform/channel_id.
    // Requests without these fields (e.g. GET /health) bypass the guard.
    if let Ok(mut body_json) = serde_json::from_slice::<Value>(&bytes) {
        // Scrub credentials and detect injection in user_input before it reaches
        // STT or Cortex.  Runs even if the guard check is skipped (no platform field).
        if let Some(raw) = body_json.get("user_input").and_then(Value::as_str) {
            let ctx = format!(
                "platform:{} channel_id:{}",
                body_json.get("platform").and_then(Value::as_str).unwrap_or("?"),
                body_json.get("channel_id").and_then(Value::as_str).unwrap_or("?"),
            );
            let cleaned = scrub::process(raw, &ctx);
            if cleaned != raw {
                body_json["user_input"] = Value::String(cleaned);
                bytes = serde_json::to_vec(&body_json)
                    .unwrap_or(bytes.to_vec())
                    .into();
            }
        }

        if let (Some(platform), Some(channel_id)) = (
            body_json.get("platform").and_then(Value::as_str),
            body_json.get("channel_id").and_then(Value::as_str),
        ) {
            let guard_db    = state.guard_db.clone();
            let platform_s  = platform.to_string();
            let channel_id_s = channel_id.to_string();

            // rusqlite is sync — run in a blocking thread so we don't stall the
            // async executor.  The lookup is a single indexed SQLite read: ~50µs.
            let check_result = tokio::task::spawn_blocking(move || {
                guard_db.check(&platform_s, &channel_id_s)
            })
            .await;

            match check_result {
                Ok(Ok((allowed, reason))) => {
                    if allowed {
                        info!(platform, channel_id, reason, "guard.allowed");
                        state.metrics.record(&guard_metric(platform, channel_id, &reason, guard_started));
                    } else {
                        info!(platform, channel_id, reason, "guard.blocked");
                        state.metrics.record(&guard_metric(platform, channel_id, "blocked", guard_started));
                        return (
                            StatusCode::FORBIDDEN,
                            axum::Json(json!({
                                "error": "access_denied",
                                "reason": reason,
                                "platform": platform,
                                "channel_id": channel_id,
                            })),
                        )
                            .into_response();
                    }
                }
                Ok(Err(e)) => {
                    // DB error — fail open, log warning.
                    warn!(platform, channel_id, error = %e, "guard.check.db_error — failing open");
                    state.metrics.record(&guard_metric(platform, channel_id, "db_error", guard_started));
                }
                Err(e) => {
                    // spawn_blocking panicked — should never happen.
                    warn!(platform, channel_id, error = %e, "guard.check.spawn_error — failing open");
                    state.metrics.record(&guard_metric(platform, channel_id, "spawn_error", guard_started));
                }
            }
        }
    }

    // Reconstruct request with the (possibly scrubbed) body bytes and pass through.
    let req = Request::from_parts(parts, Body::from(bytes));
    next.run(req).await
}
