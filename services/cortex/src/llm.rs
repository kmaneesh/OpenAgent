use crate::config::ProviderConfig;
use anyhow::Result;
use autoagents_llm::backends::anthropic::Anthropic;
use autoagents_llm::backends::openai::OpenAI;
use autoagents_llm::builder::LLMBuilder;
use std::sync::Arc;
use tracing::warn;

/// Resolved system prompt passed to `CortexAgent`. Only `system_prompt` is read
/// by the agent; the struct wraps it so callers get a named return type.
#[derive(Debug, Clone)]
pub struct StepPrompt {
    pub system_prompt: String,
}

/// Build a boxed `LLMProvider` from a `ProviderConfig`.
pub fn build_llm_provider(config: &ProviderConfig) -> Result<Arc<dyn autoagents_llm::LLMProvider>> {
    match config.kind.trim() {
        "anthropic" => {
            let p = LLMBuilder::<Anthropic>::new()
                .api_key(&config.api_key)
                .base_url(&config.base_url)
                .model(&config.model)
                .timeout_seconds(config.timeout as u64)
                .max_tokens(config.max_tokens)
                .build()
                .map_err(|e| anyhow::anyhow!("anthropic provider build failed: {e}"))?;
            Ok(p)
        }
        _ => {
            let api_key = if config.api_key.is_empty() { "none" } else { &config.api_key };
            let p = LLMBuilder::<OpenAI>::new()
                .api_key(api_key)
                .base_url(&config.base_url)
                .model(&config.model)
                .timeout_seconds(config.timeout as u64)
                .max_tokens(config.max_tokens)
                .build()
                .map_err(|e| anyhow::anyhow!("openai provider build failed: {e}"))?;
            Ok(p)
        }
    }
}

pub fn build_prompt_with_action_context(
    system_prompt: &str,
    _user_input: &str,
    action_context: Option<String>,
) -> StepPrompt {
    StepPrompt {
        system_prompt: append_action_context(system_prompt.trim(), action_context.as_deref()),
    }
}

fn append_action_context(system_prompt: &str, action_context: Option<&str>) -> String {
    crate::prompt::render_tool_context(system_prompt, action_context.unwrap_or(""))
        .unwrap_or_else(|e| {
            warn!(error = %e, "tool_context render failed — using bare system prompt");
            system_prompt.to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_trims_system_prompt() {
        let prompt = build_prompt_with_action_context("  system  ", "user", None);
        assert_eq!(prompt.system_prompt, "system");
    }

    #[test]
    fn build_prompt_appends_action_context() {
        let prompt = build_prompt_with_action_context(
            "system",
            "user",
            Some("- browser.open [browser] - Open a URL".to_string()),
        );
        assert!(prompt.system_prompt.contains("## Available tools"));
        assert!(prompt.system_prompt.contains("browser.open"));
        assert!(prompt.system_prompt.contains("Start your response with"));
    }
}
