//! CortexAgent — implements AutoAgents AgentDeriveT + AgentExecutor + AgentHooks.
//!
//! Phase 1B entry point: handle_step() calls `CortexAgent::step()` directly.
//! Phase 2+: wire `AgentBuilder<CortexAgent, DirectAgent>` → `DirectAgentHandle::run(task)`,
//!           which routes through `AgentExecutor::execute()`.

use crate::config::ProviderConfig;
use crate::llm::{build_prompt_with_action_context, complete, complete_messages, StepOutput};
use crate::response::parse_step_model_output;
use crate::tool_router::ToolRouter;
use crate::validator::maybe_validate_response;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use autoagents_llm::chat::{ChatMessageBuilder, ChatRole};
use std::fmt;
use tracing::{info, warn};
use autoagents_core::agent::{
    AgentDeriveT, AgentExecutor, AgentHooks, Context, ExecutorConfig,
};
use autoagents_core::agent::task::Task;
use autoagents_core::tool::ToolT;
use serde_json::Value;
use std::sync::Arc;

/// Error type for AgentExecutor — wraps anyhow::Error as a std::error::Error.
///
/// `anyhow::Error` intentionally does not implement `std::error::Error` to avoid
/// coherence issues; this newtype bridges the gap for the AgentExecutor trait bound.
#[derive(Debug)]
pub struct CortexAgentError(String);

impl fmt::Display for CortexAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CortexAgentError {}

impl From<anyhow::Error> for CortexAgentError {
    fn from(e: anyhow::Error) -> Self {
        Self(format!("{e:#}"))
    }
}

// ── CortexAgent ───────────────────────────────────────────────────────────────

/// Cortex reasoning agent.
///
/// Constructed fresh per `cortex.step` request — stateless by design in Phase 1B.
/// Stores the pre-built structured system prompt and pre-computed action context so
/// `step()` / `execute()` can be called without further config loading.
#[derive(Debug)]
pub struct CortexAgent {
    /// Agent name from YAML config (e.g. "default", "researcher").
    agent_name: String,
    /// Human-readable description for AgentDeriveT introspection.
    description: String,
    /// Fully-assembled system prompt (includes JSON format instructions injected by
    /// `build_structured_system_prompt`).
    pub system_prompt: String,
    /// Pre-computed candidate action context injected on generation turns.
    /// `None` on tool_call turns.
    pub action_context: Option<String>,
    /// Provider config for direct LLM calls in Phase 1B.
    pub provider_config: ProviderConfig,
    /// Tool set declared for AgentDeriveT — stubs wired in Phase 2+.
    tools: Vec<Box<dyn ToolT>>,
}

impl CortexAgent {
    pub fn new(
        agent_name: String,
        system_prompt: String,
        action_context: Option<String>,
        provider_config: ProviderConfig,
        tools: Vec<Box<dyn ToolT>>,
    ) -> Self {
        Self {
            description: format!("Cortex reasoning agent: {agent_name}"),
            agent_name,
            system_prompt,
            action_context,
            provider_config,
            tools,
        }
    }

    /// Single-turn execution — called by `AgentExecutor::execute` and used in testing.
    ///
    /// Builds the final prompt (system + action context + user input), calls the
    /// configured LLM, and returns the raw `StepOutput`.
    /// Validation and JSON parsing are handled by the caller.
    pub async fn step(&self, user_input: &str) -> Result<StepOutput> {
        let prompt = build_prompt_with_action_context(
            &self.system_prompt,
            user_input,
            self.action_context.clone(),
        );
        complete(&self.provider_config, &prompt).await
    }

    /// Full ReAct loop — Phase 2 execution path called from `handle_step`.
    ///
    /// Runs LLM → tool → LLM until the model returns `{"type":"final",...}` or
    /// `MAX_REACT_ITERATIONS` is reached.  Tool results are fed back as user
    /// messages so the model has full context for each subsequent turn.
    pub async fn run(&self, user_input: &str, router: &ToolRouter) -> Result<ReActOutput> {
        // Build the first-turn message list: [system+context, user].
        let prompt = build_prompt_with_action_context(
            &self.system_prompt,
            user_input,
            self.action_context.clone(),
        );
        let mut messages = vec![
            ChatMessageBuilder::new(ChatRole::System)
                .content(&prompt.system_prompt)
                .build(),
            ChatMessageBuilder::new(ChatRole::User)
                .content(user_input.trim())
                .build(),
        ];

        let mut tool_calls_made: Vec<String> = Vec::new();
        let mut last_model = String::new();
        let mut last_provider_kind = String::new();

        for iteration in 0..MAX_REACT_ITERATIONS {
            let output = complete_messages(&self.provider_config, &messages).await?;
            last_model = output.model.clone();
            last_provider_kind = output.provider_kind.clone();

            let validation = maybe_validate_response(&output.content).await?;
            let parsed = parse_step_model_output(&validation.content)?;

            match parsed.response_type.as_str() {
                "final" => {
                    info!(
                        iterations = iteration + 1,
                        tool_calls = tool_calls_made.len(),
                        provider_kind = %last_provider_kind,
                        model = %last_model,
                        "cortex.react.complete"
                    );
                    return Ok(ReActOutput {
                        response_text: parsed.response_text,
                        provider_kind: last_provider_kind,
                        model: last_model,
                        iterations: iteration + 1,
                        tool_calls_made,
                    });
                }
                "tool_call" => {
                    let tool_name = parsed.tool_call["tool"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let arguments = parsed.tool_call["arguments"].clone();

                    info!(
                        tool = %tool_name,
                        iteration = iteration + 1,
                        "cortex.react.tool_call"
                    );

                    // Append the model's tool_call JSON as an assistant turn.
                    messages.push(
                        ChatMessageBuilder::new(ChatRole::Assistant)
                            .content(&validation.content)
                            .build(),
                    );

                    // Execute the tool.  On failure, feed the error back so the
                    // model can decide how to recover.
                    let tool_result = match router.call(&tool_name, &arguments).await {
                        Ok(result) => {
                            info!(
                                tool = %tool_name,
                                result_len = result.len(),
                                "cortex.react.tool_result"
                            );
                            result
                        }
                        Err(e) => {
                            warn!(tool = %tool_name, error = %e, "cortex.react.tool_error");
                            format!("{{\"error\":\"{e}\",\"tool\":\"{tool_name}\"}}")
                        }
                    };

                    tool_calls_made.push(tool_name.clone());

                    // Append tool result as the next user turn.
                    messages.push(
                        ChatMessageBuilder::new(ChatRole::User)
                            .content(&format!("Tool result for {tool_name}:\n{tool_result}"))
                            .build(),
                    );
                }
                other => {
                    return Err(anyhow!("unsupported response type in react loop: {other}"));
                }
            }
        }

        Err(anyhow!(
            "react loop reached {} iterations without a final response",
            MAX_REACT_ITERATIONS
        ))
    }
}

/// Maximum number of LLM→tool→LLM turns per `cortex.step` request.
const MAX_REACT_ITERATIONS: usize = 10;

/// Output from a completed `CortexAgent::run()` loop.
#[derive(Debug)]
pub struct ReActOutput {
    /// The model's final answer text.
    pub response_text: String,
    /// Provider kind label (e.g. "openai", "anthropic") for telemetry.
    pub provider_kind: String,
    /// Full model label (e.g. "openai::qwen2.5-7b-instruct") for telemetry.
    pub model: String,
    /// Number of LLM turns used (1 = direct final answer, >1 = tool calls made).
    pub iterations: usize,
    /// Ordered list of tool names called during the loop, for telemetry.
    pub tool_calls_made: Vec<String>,
}

// ── AgentDeriveT ─────────────────────────────────────────────────────────────

impl AgentDeriveT for CortexAgent {
    /// Plain String output — validated LLM response content.
    type Output = String;

    fn name(&self) -> &str {
        &self.agent_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn output_schema(&self) -> Option<Value> {
        // Phase 1B: CortexAgent parses JSON through prompt instructions — no structured output.
        // Phase 2+: return the JSON schema when using the framework's LLM path.
        None
    }

    fn tools(&self) -> Vec<Box<dyn ToolT>> {
        // Phase 1B: action dispatch is done via action catalog injection into the prompt.
        // Phase 2+: return self.tools for framework-managed tool calls.
        vec![]
    }
}

// ── AgentExecutor ─────────────────────────────────────────────────────────────

#[async_trait]
impl AgentExecutor for CortexAgent {
    /// Plain-string output — validated LLM response content.
    type Output = String;
    type Error = CortexAgentError;

    fn config(&self) -> ExecutorConfig {
        ExecutorConfig { max_turns: 1 }
    }

    /// Phase 2+ entry point — called by `DirectAgentHandle::run(task)`.
    ///
    /// `context.llm()` is available but Phase 1B uses `provider_config` directly.
    /// Phase 2+: switch to `context.llm().chat_stream()` when Arc<dyn LLMProvider>
    /// is wired through `AgentBuilder`.
    async fn execute(
        &self,
        task: &Task,
        _context: Arc<Context>,
    ) -> Result<String, CortexAgentError> {
        let output = self.step(&task.prompt).await.map_err(CortexAgentError::from)?;
        Ok(output.content)
    }
}

// ── AgentHooks ────────────────────────────────────────────────────────────────

/// All lifecycle hooks use default no-op implementations in Phase 1B.
/// Phase 3+: override `on_turn_complete` to fire episode writes to the memory service.
#[async_trait]
impl AgentHooks for CortexAgent {}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_provider() -> ProviderConfig {
        ProviderConfig {
            kind: "openai_compat".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            model: "test-model".to_string(),
            timeout: 10.0,
            max_tokens: 512,
            debug_llm: false,
        }
    }

    #[test]
    fn agent_name_and_description() {
        let agent = CortexAgent::new(
            "researcher".to_string(),
            "System prompt".to_string(),
            None,
            dummy_provider(),
            vec![],
        );
        assert_eq!(agent.name(), "researcher");
        assert!(agent.description().contains("researcher"));
    }

    #[test]
    fn output_schema_is_none_in_phase1b() {
        let agent = CortexAgent::new(
            "default".to_string(),
            "System".to_string(),
            None,
            dummy_provider(),
            vec![],
        );
        assert!(agent.output_schema().is_none());
    }

    #[test]
    fn tools_returns_empty_for_framework_in_phase1b() {
        let agent = CortexAgent::new(
            "default".to_string(),
            "System".to_string(),
            None,
            dummy_provider(),
            vec![],
        );
        assert!(agent.tools().is_empty());
    }
}
