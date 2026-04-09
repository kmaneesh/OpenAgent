//! AgentCore — implements AutoAgents AgentDeriveT + AgentExecutor + AgentHooks.
//!
//! Stateless by design: all persistent state lives in `HybridMemoryAdapter`.
//! The system prompt is pre-assembled by `handle_step` (base prompt + skill summaries).
//! Tool dispatch goes through `ToolRouter` over TCP (or in-process for skill.read).

use crate::agent::config::ProviderConfig;
use crate::agent::diary::write_diary_entry;
use crate::agent::llm::build_prompt_with_skill_context;
use crate::agent::tool_router::ToolRouter;
use async_trait::async_trait;
use autoagents_core::agent::task::Task;
use autoagents_core::agent::{
    AgentDeriveT, AgentExecutor, AgentHooks, AgentOutputT, Context, ExecutorConfig, HookOutcome,
};
use autoagents_core::tool::ToolCallResult;
use autoagents_llm::{FunctionCall, ToolCall as LlmToolCall};
use autoagents_llm::chat::{ChatMessageBuilder, ChatRole, FunctionTool, Tool};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

// ── ToolEntry ─────────────────────────────────────────────────────────────────

/// Lightweight tool descriptor passed from `handle_step` to `AgentCore`.
#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub params: Value,
}

impl ToolEntry {
    pub fn to_llm_tool(&self) -> Tool {
        Tool {
            tool_type: "function".to_string(),
            function: FunctionTool {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: self.params.clone(),
            },
        }
    }
}

// ── AgentError ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorKind {
    LlmCall,
    Memory,
    IterationLimit,
    Other,
}

#[derive(Debug)]
pub struct AgentError {
    pub kind: AgentErrorKind,
    message: String,
    cause: Option<String>,
}

impl AgentError {
    pub fn new(kind: AgentErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), cause: None }
    }

    pub fn with_cause(kind: AgentErrorKind, message: impl Into<String>, cause: impl fmt::Display) -> Self {
        Self { kind, message: message.into(), cause: Some(cause.to_string()) }
    }

    pub fn llm_call(cause: impl fmt::Display) -> Self {
        Self::with_cause(AgentErrorKind::LlmCall, "LLM call failed", cause)
    }

    pub fn memory(cause: impl fmt::Display) -> Self {
        Self::with_cause(AgentErrorKind::Memory, "memory operation failed", cause)
    }

    pub fn iteration_limit(max: usize) -> Self {
        Self::new(AgentErrorKind::IterationLimit, format!("react loop reached {max} iterations without a final response"))
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(ref cause) = self.cause {
            write!(f, ": {cause}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AgentError {}

impl From<anyhow::Error> for AgentError {
    fn from(e: anyhow::Error) -> Self {
        Self::with_cause(AgentErrorKind::Other, "internal error", format!("{e:#}"))
    }
}

impl From<AgentError> for autoagents_core::agent::error::RunnableAgentError {
    fn from(e: AgentError) -> Self {
        autoagents_core::agent::error::RunnableAgentError::ExecutorError(e.to_string())
    }
}

// ── ReActOutput ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActOutput {
    pub response_text: String,
    pub provider_kind: String,
    pub model: String,
    pub iterations: usize,
    pub tool_calls_made: Vec<String>,
}

impl AgentOutputT for ReActOutput {
    fn output_schema() -> &'static str {
        r#"{"type":"object","properties":{"response_text":{"type":"string"},"provider_kind":{"type":"string"},"model":{"type":"string"},"iterations":{"type":"integer"},"tool_calls_made":{"type":"array","items":{"type":"string"}}},"required":["response_text","provider_kind","model","iterations","tool_calls_made"]}"#
    }

    fn structured_output_format() -> serde_json::Value {
        serde_json::json!({
            "name": "ReActOutput",
            "description": "Output from a completed agent ReAct loop",
            "schema": {
                "type": "object",
                "properties": {
                    "response_text": {"type": "string"},
                    "provider_kind": {"type": "string"},
                    "model": {"type": "string"},
                    "iterations": {"type": "integer"},
                    "tool_calls_made": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["response_text", "provider_kind", "model", "iterations", "tool_calls_made"]
            },
            "strict": true
        })
    }
}

// ── AgentCore ─────────────────────────────────────────────────────────────────

/// Agent reasoning core — constructed fresh per `handle_step` request.
///
/// Stateless by design: all persistent state lives in `HybridMemoryAdapter`.
#[derive(Debug)]
pub struct AgentCore {
    agent_name: String,
    description: String,
    pub system_prompt: String,
    pub tool_entries: Vec<ToolEntry>,
    pub provider_config: ProviderConfig,
    router: Arc<ToolRouter>,
    session_id: String,
    diary_dir: PathBuf,
}

impl AgentCore {
    pub fn new(
        agent_name: String,
        system_prompt: String,
        tool_entries: Vec<ToolEntry>,
        provider_config: ProviderConfig,
        router: Arc<ToolRouter>,
        session_id: String,
        diary_dir: PathBuf,
    ) -> Self {
        Self {
            description: format!("Agent: {agent_name}"),
            agent_name,
            system_prompt,
            tool_entries,
            provider_config,
            router,
            session_id,
            diary_dir,
        }
    }
}

const MAX_REACT_ITERATIONS: usize = 100;

impl AgentDeriveT for AgentCore {
    type Output = ReActOutput;

    fn name(&self) -> &str { &self.agent_name }
    fn description(&self) -> &str { &self.description }

    fn output_schema(&self) -> Option<Value> {
        Some(ReActOutput::structured_output_format())
    }

    fn tools(&self) -> Vec<Box<dyn autoagents_core::tool::ToolT>> {
        // Tool dispatch goes through ToolRouter — not the framework's ToolProcessor.
        vec![]
    }
}

#[async_trait]
impl AgentExecutor for AgentCore {
    type Output = ReActOutput;
    type Error = AgentError;

    fn config(&self) -> ExecutorConfig {
        ExecutorConfig { max_turns: MAX_REACT_ITERATIONS }
    }

    async fn execute(&self, task: &Task, context: Arc<Context>) -> Result<ReActOutput, AgentError> {
        let user_input = task.prompt.trim();

        let prompt = build_prompt_with_skill_context(&self.system_prompt, None);

        let memory = context.memory();
        let history = if let Some(ref mem) = memory {
            mem.lock().await.recall(user_input, None).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        let user_msg = ChatMessageBuilder::new(ChatRole::User).content(user_input).build();
        let mut messages = Vec::with_capacity(2 + history.len());
        messages.push(ChatMessageBuilder::new(ChatRole::System).content(&prompt.system_prompt).build());
        messages.extend(history);
        messages.push(user_msg.clone());

        if let Some(ref mem) = memory {
            mem.lock().await.remember(&user_msg).await.map_err(AgentError::memory)?;
        }

        let llm_tools: Vec<Tool> = self.tool_entries.iter().map(ToolEntry::to_llm_tool).collect();
        let tools_slice: Option<&[Tool]> = if llm_tools.is_empty() { None } else { Some(&llm_tools) };

        let mut tool_calls_made: Vec<String> = Vec::new();

        for iteration in 0..MAX_REACT_ITERATIONS {
            self.on_turn_start(iteration, &context).await;

            let response = context
                .llm()
                .chat_with_tools(&messages, tools_slice, None)
                .await
                .map_err(|e| AgentError::llm_call(e))?;

            if self.provider_config.debug_llm {
                info!(
                    provider_kind = %self.provider_config.kind,
                    model = %self.provider_config.model,
                    text = ?response.text(),
                    tool_calls = ?response.tool_calls(),
                    "agent.llm.response"
                );
            }

            let native_tool_calls = response.tool_calls().unwrap_or_default();

            if native_tool_calls.is_empty() {
                let text = response.text().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    return Err(AgentError::llm_call("provider returned empty response with no tool calls"));
                }

                info!(
                    iterations = iteration + 1,
                    tool_calls = tool_calls_made.len(),
                    provider_kind = %self.provider_config.kind,
                    model = %self.provider_config.model,
                    "agent.react.complete"
                );

                let final_msg = ChatMessageBuilder::new(ChatRole::Assistant).content(&text).build();
                if let Some(ref mem) = memory {
                    let _ = mem.lock().await.remember(&final_msg).await;
                }

                tokio::spawn(write_diary_entry(
                    self.session_id.clone(),
                    self.diary_dir.clone(),
                    user_input.to_string(),
                    text.clone(),
                    tool_calls_made.clone(),
                    Arc::clone(&self.router),
                ));

                self.on_turn_complete(iteration, &context).await;

                let (provider_kind, model) = telemetry_labels(&self.provider_config);
                return Ok(ReActOutput {
                    response_text: text,
                    provider_kind,
                    model,
                    iterations: iteration + 1,
                    tool_calls_made,
                });
            }

            let assistant_msg = ChatMessageBuilder::new(ChatRole::Assistant)
                .tool_use(native_tool_calls.clone())
                .content(response.text().unwrap_or_default())
                .build();
            if let Some(ref mem) = memory {
                let _ = mem.lock().await.remember(&assistant_msg).await;
            }
            messages.push(assistant_msg);

            for tc in &native_tool_calls {
                let tool_name = &tc.function.name;
                let args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();

                info!(tool = %tool_name, iteration = iteration + 1, "agent.react.tool_call");

                let framework_tc = LlmToolCall {
                    id: tc.id.clone(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: tool_name.clone(),
                        arguments: tc.function.arguments.clone(),
                    },
                };

                if self.on_tool_call(&framework_tc, &context).await == HookOutcome::Abort {
                    warn!(tool = %tool_name, "agent.react.tool_call.aborted_by_hook");
                    let abort_tc = LlmToolCall {
                        id: tc.id.clone(),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: tool_name.clone(),
                            arguments: r#"{"error":"tool call was aborted"}"#.to_string(),
                        },
                    };
                    let tool_msg = ChatMessageBuilder::new(ChatRole::Tool)
                        .tool_result(vec![abort_tc])
                        .build();
                    messages.push(tool_msg);
                    continue;
                }

                self.on_tool_start(&framework_tc, &context).await;

                let result = match self.router.call(tool_name, &args).await {
                    Ok(r) => {
                        info!(tool = %tool_name, result_len = r.len(), "agent.react.tool_result");
                        let call_result = ToolCallResult {
                            tool_name: tool_name.clone(),
                            success: true,
                            arguments: args.clone(),
                            result: serde_json::json!(r.clone()),
                        };
                        self.on_tool_result(&framework_tc, &call_result, &context).await;
                        r
                    }
                    Err(e) => {
                        warn!(tool = %tool_name, error = %e, "agent.react.tool_error");
                        let err_val = serde_json::json!(e.to_string());
                        self.on_tool_error(&framework_tc, err_val, &context).await;
                        format!(r#"{{"error":"{e}","tool":"{tool_name}"}}"#)
                    }
                };

                tool_calls_made.push(tool_name.clone());

                let result_tc = LlmToolCall {
                    id: tc.id.clone(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: tool_name.clone(),
                        arguments: result,
                    },
                };
                let tool_msg = ChatMessageBuilder::new(ChatRole::Tool)
                    .tool_result(vec![result_tc])
                    .build();
                if let Some(ref mem) = memory {
                    let _ = mem.lock().await.remember(&tool_msg).await;
                }
                messages.push(tool_msg);
            }

            self.on_turn_complete(iteration, &context).await;
        }

        Err(AgentError::iteration_limit(MAX_REACT_ITERATIONS))
    }
}

#[async_trait]
impl AgentHooks for AgentCore {}

fn telemetry_labels(config: &ProviderConfig) -> (String, String) {
    let provider_kind = config.kind.clone();
    let display_kind = match config.kind.trim() {
        "openai" | "openai_compat" => "openai",
        "anthropic" => "anthropic",
        "ollama" => "ollama",
        other => other,
    };
    let model = format!("{}::{}", display_kind, config.model.trim());
    (provider_kind, model)
}

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

    fn make_router() -> Arc<ToolRouter> {
        Arc::new(ToolRouter::new(std::collections::HashMap::new(), PathBuf::from("data")))
    }

    fn make_agent(name: &str) -> AgentCore {
        AgentCore::new(
            name.to_string(),
            "System prompt".to_string(),
            vec![],
            dummy_provider(),
            make_router(),
            "test-session".to_string(),
            PathBuf::from("data/diary/test-session"),
        )
    }

    #[test]
    fn agent_name_and_description() {
        let agent = make_agent("researcher");
        assert_eq!(agent.name(), "researcher");
        assert!(agent.description().contains("researcher"));
    }

    #[test]
    fn output_schema_is_some_with_react_output_schema() {
        let agent = make_agent("default");
        let schema = agent.output_schema().unwrap();
        assert_eq!(schema["name"], "ReActOutput");
    }

    #[test]
    fn tools_returns_empty_framework_tools() {
        let agent = make_agent("default");
        assert!(agent.tools().is_empty());
    }

    #[test]
    fn executor_config_exposes_max_react_iterations() {
        let agent = make_agent("default");
        assert_eq!(agent.config().max_turns, MAX_REACT_ITERATIONS);
    }

    #[test]
    fn tool_entry_converts_to_llm_tool() {
        let entry = ToolEntry {
            name: "web.search".to_string(),
            description: "Search the web".to_string(),
            params: serde_json::json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
        };
        let tool = entry.to_llm_tool();
        assert_eq!(tool.function.name, "web.search");
        assert_eq!(tool.tool_type, "function");
    }

    #[test]
    fn telemetry_labels_normalises_openai_compat() {
        let config = dummy_provider();
        let (kind, model) = telemetry_labels(&config);
        assert_eq!(kind, "openai_compat");
        assert_eq!(model, "openai::test-model");
    }
}
