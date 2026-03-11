use crate::config::ProviderConfig;
use anyhow::{anyhow, Result};
use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatResponse};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{Client, ModelIden, ServiceTarget, WebConfig};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Clone)]
pub struct StepPrompt {
    pub system_prompt: String,
    pub user_input: String,
}

#[derive(Debug, Clone)]
pub struct StepOutput {
    pub content: String,
    pub provider_kind: String,
    pub model: String,
}

pub async fn complete(provider: &ProviderConfig, prompt: &StepPrompt) -> Result<StepOutput> {
    let requested_model = requested_model(provider)?;
    let client = build_client(provider);
    let chat_req = ChatRequest::new(vec![ChatMessage::user(prompt.user_input.clone())])
        .with_system(prompt.system_prompt.clone());
    let chat_options = ChatOptions::default()
        .with_max_tokens(provider.max_tokens)
        .with_capture_raw_body(true)
        .with_normalize_reasoning_content(true);
    let chat_res = client
        .exec_chat(&requested_model, chat_req, Some(&chat_options))
        .await?;
    if provider.log_raw_response {
        if let Some(raw) = chat_res.captured_raw_body.as_ref() {
            info!(
                provider_kind = %provider.kind,
                model = %requested_model,
                raw_llm_response = %raw,
                "cortex.llm.raw_response"
            );
        } else {
            info!(
                provider_kind = %provider.kind,
                model = %requested_model,
                "cortex.llm.raw_response.unavailable"
            );
        }
    }
    let content = extract_text(chat_res)?;

    Ok(StepOutput {
        content,
        provider_kind: provider.kind.clone(),
        model: requested_model,
    })
}

fn extract_text(chat_res: ChatResponse) -> Result<String> {
    let ChatResponse {
        content,
        reasoning_content,
        captured_raw_body,
        ..
    } = chat_res;

    if let Some(text) = content.into_joined_texts() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Some(reasoning) = reasoning_content {
        let trimmed = reasoning.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Some(raw) = captured_raw_body {
        if let Some(text) = extract_text_from_raw_body(&raw) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        return Err(anyhow!("provider returned no readable text: {}", raw));
    }

    Err(anyhow!("provider returned no readable text"))
}

fn extract_text_from_raw_body(body: &Value) -> Option<String> {
    let choice = body.get("choices")?.get(0)?;

    if let Some(text) = choice
        .get("message")
        .and_then(|message| extract_text_from_message_content(message.get("content")))
    {
        return Some(text);
    }

    if let Some(text) = choice.get("text").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    body.get("output")
        .and_then(Value::as_array)
        .and_then(|items| {
            let texts: Vec<String> = items
                .iter()
                .filter_map(|item| extract_text_from_output_item(item))
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n\n"))
            }
        })
}

fn extract_text_from_output_item(item: &Value) -> Option<String> {
    let content = item.get("content")?.as_array()?;
    let texts: Vec<String> = content
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n\n"))
    }
}

fn extract_text_from_message_content(content: Option<&Value>) -> Option<String> {
    let content = content?;
    match content {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(parts) => {
            let texts: Vec<String> = parts
                .iter()
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| part.get("content").and_then(Value::as_str))
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n\n"))
            }
        }
        _ => None,
    }
}

fn build_client(provider: &ProviderConfig) -> Client {
    let cfg = provider.clone();
    let resolver_cfg = cfg.clone();
    let resolver = ServiceTargetResolver::from_resolver_fn(
        move |target: ServiceTarget| -> Result<ServiceTarget, genai::resolver::Error> {
            let adapter_kind = adapter_kind_for(&resolver_cfg);
            let model_name = raw_model_name(&resolver_cfg);
            let endpoint = if resolver_cfg.base_url.trim().is_empty() {
                target.endpoint
            } else {
                Endpoint::from_owned(normalize_base_url(&resolver_cfg.base_url))
            };
            let auth = AuthData::from_single(resolver_cfg.api_key.clone());
            Ok(ServiceTarget {
                endpoint,
                auth,
                model: ModelIden::new(adapter_kind, model_name),
            })
        },
    );

    Client::builder()
        .with_web_config(WebConfig::default().with_timeout(Duration::from_secs_f64(cfg.timeout)))
        .with_service_target_resolver(resolver)
        .build()
}

fn requested_model(provider: &ProviderConfig) -> Result<String> {
    let model = raw_model_name(provider);
    if model.is_empty() {
        return Err(anyhow!("provider.model is required for Cortex Phase 1"));
    }
    Ok(format!(
        "{}::{}",
        adapter_kind_for(provider).as_lower_str(),
        model
    ))
}

fn raw_model_name(provider: &ProviderConfig) -> String {
    provider.model.trim().to_string()
}

fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

fn adapter_kind_for(provider: &ProviderConfig) -> AdapterKind {
    match provider.kind.as_str() {
        "anthropic" => AdapterKind::Anthropic,
        "openai" | "openai_compat" => AdapterKind::OpenAI,
        _ => AdapterKind::OpenAI,
    }
}

pub fn build_prompt(system_prompt: &str, user_input: &str) -> StepPrompt {
    StepPrompt {
        system_prompt: system_prompt.trim().to_string(),
        user_input: user_input.trim().to_string(),
    }
}

pub fn prompt_preview(prompt: &StepPrompt) -> Value {
    json!({
        "system_prompt_len": prompt.system_prompt.len(),
        "user_input_len": prompt.user_input.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_trims_both_parts() {
        let prompt = build_prompt("  system  ", "  user  ");
        assert_eq!(prompt.system_prompt, "system");
        assert_eq!(prompt.user_input, "user");
    }

    #[test]
    fn requested_model_uses_provider_namespace() {
        let provider = ProviderConfig {
            kind: "openai_compat".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            model: "qwen2.5-7b-instruct".to_string(),
            timeout: 60.0,
            max_tokens: 2048,
            log_raw_response: false,
        };
        assert_eq!(
            requested_model(&provider).expect("model should resolve"),
            "openai::qwen2.5-7b-instruct"
        );
    }

    #[test]
    fn normalize_base_url_preserves_v1_path() {
        assert_eq!(
            normalize_base_url("http://localhost:1234/v1"),
            "http://localhost:1234/v1/"
        );
        assert_eq!(
            normalize_base_url("http://localhost:1234/v1/"),
            "http://localhost:1234/v1/"
        );
    }

    #[test]
    fn extract_text_from_raw_body_supports_array_message_content() {
        let raw = json!({
            "choices": [{
                "message": {
                    "content": [
                        {"type": "text", "text": "hello"},
                        {"type": "text", "text": "world"}
                    ]
                }
            }]
        });

        assert_eq!(
            extract_text_from_raw_body(&raw).as_deref(),
            Some("hello\n\nworld")
        );
    }

    #[test]
    fn extract_text_from_raw_body_supports_choice_text() {
        let raw = json!({
            "choices": [{
                "text": "plain text response"
            }]
        });

        assert_eq!(
            extract_text_from_raw_body(&raw).as_deref(),
            Some("plain text response")
        );
    }
}
