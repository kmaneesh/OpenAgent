use crate::agent::config::ProviderConfig;
use anyhow::Result;
use autoagents_llm::backends::anthropic::Anthropic;
use autoagents_llm::backends::openai::OpenAI;
use autoagents_llm::builder::LLMBuilder;
use std::sync::Arc;
use tracing::warn;

/// Resolved system prompt passed to `AgentCore`.
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

/// Return a `StepPrompt` with skill summaries optionally appended.
pub fn build_prompt_with_skill_context(
    system_prompt: &str,
    skill_context: Option<String>,
) -> StepPrompt {
    StepPrompt {
        system_prompt: append_skill_context(system_prompt.trim(), skill_context.as_deref()),
    }
}

fn append_skill_context(system_prompt: &str, skill_context: Option<&str>) -> String {
    crate::agent::prompt::render_skill_context(system_prompt, skill_context.unwrap_or(""))
        .unwrap_or_else(|e| {
            warn!(error = %e, "skill_context render failed — using bare system prompt");
            system_prompt.to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_trims_system_prompt() {
        let prompt = build_prompt_with_skill_context("  system  ", None);
        assert_eq!(prompt.system_prompt, "system");
    }

    #[test]
    fn build_prompt_appends_skill_context() {
        let prompt = build_prompt_with_skill_context(
            "system",
            Some("skill: agent-browser\ndescription: Browser automation.".to_string()),
        );
        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(prompt.system_prompt.contains("agent-browser"));
    }

    #[test]
    fn build_prompt_no_skills_returns_bare() {
        let prompt = build_prompt_with_skill_context("system", None);
        assert_eq!(prompt.system_prompt, "system");
    }
}
