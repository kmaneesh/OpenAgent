use anyhow::{anyhow, Result};
use sdk_rust::codec::{Decoder, Encoder};
use sdk_rust::types::Frame;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UnixStream;
use tokio::time::timeout;

const VALIDATOR_SOCKET_PATH: &str = "data/sockets/validator.sock";
const VALIDATOR_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    pub content: String,
    pub was_repaired: bool,
}

pub async fn maybe_validate_response(content: &str) -> Result<ValidationOutcome> {
    let candidate = extract_json_candidate(content).unwrap_or_else(|| content.trim().to_string());
    if !looks_like_json_candidate(&candidate) {
        return Ok(ValidationOutcome {
            content: content.to_string(),
            was_repaired: false,
        });
    }

    match repair_json(&candidate).await {
        Ok(Some(repaired)) => {
            let was_repaired = repaired != candidate;
            Ok(ValidationOutcome {
                content: repaired,
                was_repaired,
            })
        }
        Ok(None) => Ok(ValidationOutcome {
            content: content.to_string(),
            was_repaired: false,
        }),
        Err(_) => Ok(ValidationOutcome {
            content: content.to_string(),
            was_repaired: false,
        }),
    }
}

async fn repair_json(text: &str) -> Result<Option<String>> {
    let stream = timeout(
        VALIDATOR_TIMEOUT,
        UnixStream::connect(VALIDATOR_SOCKET_PATH),
    )
    .await
    .map_err(|_| anyhow!("validator connect timed out"))??;
    let (read_half, write_half) = stream.into_split();
    let mut decoder = Decoder::new(read_half);
    let mut encoder = Encoder::new(write_half);
    let request_id = request_id();

    encoder
        .write_frame(&Frame::ToolCallRequest {
            id: request_id.clone(),
            tool: "validator.repair_json".to_string(),
            params: json!({
                "text": text,
                "mode": "auto",
            }),
            trace_id: None,
            span_id: None,
        })
        .await?;

    let frame = timeout(VALIDATOR_TIMEOUT, decoder.next_frame())
        .await
        .map_err(|_| anyhow!("validator response timed out"))??;

    let Some(frame) = frame else {
        return Err(anyhow!("validator closed connection without a response"));
    };

    match frame {
        Frame::ToolCallResponse { id, result, error } if id == request_id => {
            if let Some(error) = error {
                return Err(anyhow!("validator error: {error}"));
            }
            parse_validator_result(result.as_deref())
        }
        Frame::ErrorResponse { id, code, message } if id == request_id => {
            Err(anyhow!("validator protocol error {code}: {message}"))
        }
        other => Err(anyhow!("unexpected validator frame: {other:?}")),
    }
}

fn parse_validator_result(result: Option<&str>) -> Result<Option<String>> {
    let payload = result.ok_or_else(|| anyhow!("validator returned empty result"))?;
    let parsed: Value = serde_json::from_str(payload)?;
    let ok = parsed.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        return Ok(None);
    }
    Ok(parsed
        .get("json")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned))
}

fn looks_like_json_candidate(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.starts_with("```json")
        || trimmed.starts_with("```")
}

fn extract_json_candidate(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest
            .strip_suffix("```")
            .map(str::trim)
            .map(ToOwned::to_owned);
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest
            .strip_suffix("```")
            .map(str::trim)
            .map(ToOwned::to_owned);
    }
    None
}

fn request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("cortex-validator-{nanos}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_json_candidate() {
        let candidate = extract_json_candidate("```json\n{\"ok\":true}\n```");
        assert_eq!(candidate.as_deref(), Some("{\"ok\":true}"));
    }

    #[test]
    fn detects_json_like_content() {
        assert!(looks_like_json_candidate("{\"a\":1}"));
        assert!(looks_like_json_candidate("```json\n{\"a\":1}\n```"));
        assert!(!looks_like_json_candidate("hello world"));
    }
}
