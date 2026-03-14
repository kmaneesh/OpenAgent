use crate::config::ProviderConfig;
use anyhow::{anyhow, Result};
use autoagents_llm::backends::anthropic::Anthropic;
use autoagents_llm::backends::openai::OpenAI;
use autoagents_llm::builder::LLMBuilder;
use autoagents_llm::chat::{
    ChatMessage, ChatMessageBuilder, ChatProvider, ChatResponse, ChatRole, StructuredOutputFormat,
};
use serde_json::{json, Value};
use tracing::info;

#[derive(Debug, Clone)]
pub struct StepPrompt {
    pub system_prompt: String,
    pub user_input: String,
    pub action_context: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StepOutput {
    pub content: String,
    pub provider_kind: String,
    pub model: String,
}

pub async fn complete(provider: &ProviderConfig, prompt: &StepPrompt) -> Result<StepOutput> {
    let model_label = requested_model(provider)?;
    let messages = build_messages(prompt);

    if provider.debug_llm {
        info!(
            provider_kind = %provider.kind,
            model = %model_label,
            llm_http_request = %build_debug_request(provider, prompt),
            "cortex.llm.http.request"
        );
    }

    // Dispatch on provider kind at compile time — autoagents-llm uses generic builders.
    // All OpenAI-compatible endpoints (LM Studio, Ollama /v1, local servers) use OpenAI builder.
    let content = match provider.kind.trim() {
        "anthropic" => {
            let p = LLMBuilder::<Anthropic>::new()
                .api_key(&provider.api_key)
                .base_url(&provider.base_url)
                .model(&provider.model)
                .timeout_seconds(provider.timeout as u64)
                .max_tokens(provider.max_tokens)
                .build()
                .map_err(|e| anyhow!("anthropic provider build failed: {e}"))?;
            if provider.debug_llm {
                info!(provider_kind = "anthropic", model = %model_label, "cortex.llm.call");
            }
            let resp: Box<dyn ChatResponse> = p
                .chat(&messages, None::<StructuredOutputFormat>)
                .await
                .map_err(|e| anyhow!("anthropic llm call failed: {e}"))?;
            let text = extract_text(resp)?;
            if provider.debug_llm {
                info!(
                    provider_kind = "anthropic",
                    model = %model_label,
                    response_len = text.len(),
                    llm_response_text = %text,
                    "cortex.llm.http.response"
                );
            }
            text
        }
        // openai, openai_compat, ollama, or any unrecognised kind — all speak OpenAI /v1
        _ => {
            let api_key = if provider.api_key.is_empty() {
                "none"
            } else {
                &provider.api_key
            };
            let p = LLMBuilder::<OpenAI>::new()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .model(&provider.model)
                .timeout_seconds(provider.timeout as u64)
                .max_tokens(provider.max_tokens)
                .build()
                .map_err(|e| anyhow!("openai provider build failed: {e}"))?;
            if provider.debug_llm {
                info!(provider_kind = %provider.kind, model = %model_label, "cortex.llm.call");
            }
            let resp: Box<dyn ChatResponse> = p
                .chat(&messages, None::<StructuredOutputFormat>)
                .await
                .map_err(|e| anyhow!("openai llm call failed: {e}"))?;
            let text = extract_text(resp)?;
            if provider.debug_llm {
                info!(
                    provider_kind = %provider.kind,
                    model = %model_label,
                    response_len = text.len(),
                    llm_response_text = %text,
                    "cortex.llm.http.response"
                );
            }
            text
        }
    };

    Ok(StepOutput {
        content,
        provider_kind: provider.kind.clone(),
        model: model_label,
    })
}

fn build_messages(prompt: &StepPrompt) -> Vec<ChatMessage> {
    // ChatMessage has no system() shortcut — use ChatMessageBuilder with ChatRole::System.
    vec![
        ChatMessageBuilder::new(ChatRole::System)
            .content(&prompt.system_prompt)
            .build(),
        ChatMessage::user().content(&prompt.user_input).build(),
    ]
}

fn extract_text(resp: Box<dyn ChatResponse>) -> Result<String> {
    // ChatResponse is a trait; .text() is the canonical text accessor.
    let text = resp
        .text()
        .ok_or_else(|| anyhow!("provider returned no readable text"))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("provider returned empty text"));
    }
    Ok(trimmed.to_string())
}

fn requested_model(provider: &ProviderConfig) -> Result<String> {
    let model = provider.model.trim().to_string();
    if model.is_empty() {
        return Err(anyhow!("provider.model is required for Cortex Phase 1"));
    }
    // Display label normalises "openai_compat" → "openai" for logs/metrics.
    Ok(format!("{}::{}", kind_display_label(&provider.kind), model))
}

fn kind_display_label(kind: &str) -> &str {
    match kind.trim() {
        "openai" | "openai_compat" => "openai",
        "anthropic" => "anthropic",
        "ollama" => "ollama",
        other => other,
    }
}

fn build_debug_request(provider: &ProviderConfig, prompt: &StepPrompt) -> Value {
    let base_url = normalize_base_url(&provider.base_url);
    let path = match provider.kind.trim() {
        "anthropic" => "messages",
        _ => "chat/completions",
    };
    let url = if base_url.is_empty() {
        path.to_string()
    } else {
        format!("{base_url}{path}")
    };
    json!({
        "method": "POST",
        "url": url,
        "payload": {
            "model": provider.model.trim(),
            "messages": [
                {"role": "system", "content": prompt.system_prompt},
                {"role": "user", "content": prompt.user_input}
            ],
            "stream": false,
            "max_tokens": provider.max_tokens,
        }
    })
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

pub fn build_prompt_with_action_context(
    system_prompt: &str,
    user_input: &str,
    action_context: Option<String>,
) -> StepPrompt {
    StepPrompt {
        system_prompt: append_action_context(system_prompt.trim(), action_context.as_deref()),
        user_input: user_input.trim().to_string(),
        action_context,
    }
}

pub fn prompt_preview(prompt: &StepPrompt) -> Value {
    json!({
        "system_prompt_len": prompt.system_prompt.len(),
        "user_input_len": prompt.user_input.len(),
        "action_context_len": prompt.action_context.as_ref().map_or(0, String::len),
    })
}

fn append_action_context(system_prompt: &str, action_context: Option<&str>) -> String {
    let Some(action_context) = action_context.map(str::trim).filter(|v| !v.is_empty()) else {
        return system_prompt.to_string();
    };
    format!(
        "{system_prompt}\n\nAvailable candidate actions for this generation turn:\n{action_context}\n\nIf action use becomes necessary, prefer only these candidates."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_trims_both_parts() {
        let prompt = build_prompt_with_action_context("  system  ", "  user  ", None);
        assert_eq!(prompt.system_prompt, "system");
        assert_eq!(prompt.user_input, "user");
    }

    #[test]
    fn build_prompt_appends_action_context() {
        let prompt = build_prompt_with_action_context(
            "system",
            "user",
            Some("- browser.open [browser] - Open a URL".to_string()),
        );
        assert!(prompt
            .system_prompt
            .contains("Available candidate actions for this generation turn"));
        assert!(prompt.system_prompt.contains("browser.open"));
    }

    #[test]
    fn requested_model_normalises_openai_compat_kind() {
        let provider = ProviderConfig {
            kind: "openai_compat".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            model: "qwen2.5-7b-instruct".to_string(),
            timeout: 60.0,
            max_tokens: 2048,
            debug_llm: false,
        };
        assert_eq!(
            requested_model(&provider).expect("model should resolve"),
            "openai::qwen2.5-7b-instruct"
        );
    }

    #[test]
    fn requested_model_rejects_empty_model() {
        let provider = ProviderConfig {
            kind: "openai_compat".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            model: String::new(),
            timeout: 60.0,
            max_tokens: 2048,
            debug_llm: false,
        };
        assert!(requested_model(&provider).is_err());
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
    fn build_debug_request_uses_openai_chat_completions_endpoint() {
        let provider = ProviderConfig {
            kind: "openai_compat".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            model: "qwen2.5-7b-instruct".to_string(),
            timeout: 60.0,
            max_tokens: 2048,
            debug_llm: true,
        };
        let prompt = StepPrompt {
            system_prompt: "system".to_string(),
            user_input: "user".to_string(),
            action_context: None,
        };
        let request = build_debug_request(&provider, &prompt);
        assert_eq!(
            request.get("url").and_then(Value::as_str),
            Some("http://localhost:1234/v1/chat/completions")
        );
    }

    #[test]
    fn build_debug_request_uses_anthropic_messages_endpoint() {
        let provider = ProviderConfig {
            kind: "anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            timeout: 60.0,
            max_tokens: 2048,
            debug_llm: true,
        };
        let prompt = StepPrompt {
            system_prompt: "system".to_string(),
            user_input: "user".to_string(),
            action_context: None,
        };
        let request = build_debug_request(&provider, &prompt);
        assert_eq!(
            request.get("url").and_then(Value::as_str),
            Some("https://api.anthropic.com/v1/messages")
        );
    }
}
