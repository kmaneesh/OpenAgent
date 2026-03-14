use crate::action::catalog::ActionCatalog;
use crate::action::search::{search_catalog, SearchQuery, SearchResult};
use crate::config::CortexConfig;
use crate::llm::{build_prompt_with_action_context, complete, prompt_preview};
use crate::metrics::{elapsed_ms, step_err, step_ok, CortexTelemetry};
use crate::validator::maybe_validate_response;
use anyhow::{anyhow, Result};
use opentelemetry::KeyValue;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

const DEFAULT_TOOL_NAMES: &[&str] = &[
    "browser.open",
    "browser.navigate",
    "browser.snapshot",
    "sandbox.execute",
    "sandbox.shell",
];

#[derive(Clone)]
pub struct AppContext {
    tel: Arc<CortexTelemetry>,
    action_catalog: Arc<ActionCatalog>,
}

impl AppContext {
    pub fn new(tel: Arc<CortexTelemetry>, action_catalog: Arc<ActionCatalog>) -> Self {
        Self {
            tel,
            action_catalog,
        }
    }

    pub fn tel(&self) -> Arc<CortexTelemetry> {
        Arc::clone(&self.tel)
    }

    pub fn action_catalog(&self) -> Arc<ActionCatalog> {
        Arc::clone(&self.action_catalog)
    }
}

pub fn handle_describe_boundary() -> String {
    json!({
        "phase": "phase1",
        "status": "step-ready",
        "service_boundary": {
            "is_service": true,
            "transport": "mcp-lite-json-uds",
            "python_shell_role": "temporary pre-cortex shell",
            "llm_calling_rule": "cortex-only in target architecture"
        },
        "owns_now": [
            "service identity",
            "mcp-lite socket boundary",
            "config-backed system prompt loading",
            "single-step llm execution",
            "step observability"
        ],
        "does_not_own_yet": [
            "tool routing",
            "memory retrieval",
            "plan store",
            "segmented stm"
        ]
    })
    .to_string()
}

pub fn handle_step(
    params: Value,
    tel: Arc<CortexTelemetry>,
    catalog: Arc<ActionCatalog>,
) -> Result<String> {
    let p = params
        .as_object()
        .ok_or_else(|| anyhow!("params must be an object"))?;
    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let user_input = p
        .get("user_input")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let requested_agent = p.get("agent_name").and_then(|v| v.as_str()).map(str::trim);
    let turn_kind = p
        .get("turn_kind")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("generation")
        .to_string();

    if session_id.is_empty() {
        return Err(anyhow!("session_id is required"));
    }
    if user_input.is_empty() {
        return Err(anyhow!("user_input is required"));
    }

    let _cx_guard = CortexTelemetry::attach_context(
        &params,
        vec![
            KeyValue::new("service", "cortex"),
            KeyValue::new("op", "step"),
            KeyValue::new("session_id", session_id.clone()),
        ],
    );

    let span = tracing::info_span!(
        "cortex.step",
        session_id = %session_id,
        agent_name = tracing::field::Empty,
        provider_kind = tracing::field::Empty,
        model = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
        status = tracing::field::Empty,
        user_input_len = user_input.len(),
        output_len = tracing::field::Empty,
    );
    let _enter = span.enter();

    let started = Instant::now();
    let cfg_file = CortexConfig::load()?;
    let resolved = cfg_file
        .cfg
        .resolve_step_config(cfg_file.path.clone(), requested_agent);
    let default_tools = collect_default_tools(&catalog);
    let action_context = if turn_kind == "tool_call" {
        None
    } else {
        render_default_tool_context(&default_tools)
    };
    let structured_system_prompt = build_structured_system_prompt(&resolved.system_prompt);
    let prompt =
        build_prompt_with_action_context(&structured_system_prompt, &user_input, action_context);

    span.record("agent_name", resolved.agent_name.as_str());
    span.record("provider_kind", resolved.provider.kind.as_str());
    span.record("model", resolved.provider.model.as_str());

    info!(
        agent_name = %resolved.agent_name,
        provider_kind = %resolved.provider.kind,
        config_path = %resolved.source_path.display(),
        prompt_meta = %prompt_preview(&prompt).to_string(),
        turn_kind = %turn_kind,
        inject_default_tools = turn_kind != "tool_call",
        "cortex.step.start"
    );

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async { complete(&resolved.provider, &prompt).await })
    });

    match result {
        Ok(mut output) => {
            let validation = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { maybe_validate_response(&output.content).await })
            })?;
            let validator_repaired = validation.was_repaired;
            output.content = validation.content;
            let structured = parse_step_model_output(&output.content)?;
            let duration_ms = elapsed_ms(started);
            span.record("status", "ok");
            span.record("duration_ms", duration_ms);
            span.record("output_len", output.content.len() as i64);
            info!(
                agent_name = %resolved.agent_name,
                provider_kind = %output.provider_kind,
                model = %output.model,
                duration_ms,
                output_len = output.content.len(),
                validator_repaired,
                default_tool_count = default_tools.len(),
                action_injected = turn_kind != "tool_call",
                "cortex.step.ok"
            );
            tel.record(&step_ok(
                &session_id,
                &resolved.agent_name,
                &output.provider_kind,
                &output.model,
                &resolved.source_path.display().to_string(),
                duration_ms,
                user_input.len(),
                output.content.len(),
            ));

            Ok(json!({
                "session_id": session_id,
                "agent_name": resolved.agent_name,
                "provider_kind": output.provider_kind,
                "model": output.model,
                "response_type": structured.response_type,
                "response_text": structured.response_text,
                "tool_call": structured.tool_call,
                "action_activity_summary": {
                    "turn_kind": turn_kind,
                    "injected": turn_kind != "tool_call",
                    "candidate_count": default_tools.len(),
                    "candidates": default_tools.iter().map(|v| v.name.clone()).collect::<Vec<_>>()
                },
                "tool_activity_summary": {
                    "candidate_count": default_tools.len(),
                    "candidates": default_tools.iter().map(|v| v.name.clone()).collect::<Vec<_>>()
                }
            })
            .to_string())
        }
        Err(err) => {
            let duration_ms = elapsed_ms(started);
            span.record("status", "error");
            span.record("duration_ms", duration_ms);
            error!(
                agent_name = %resolved.agent_name,
                provider_kind = %resolved.provider.kind,
                model = %resolved.provider.model,
                duration_ms,
                error = %err,
                turn_kind = %turn_kind,
                action_injected = turn_kind != "tool_call",
                "cortex.step.error"
            );
            tel.record(&step_err(
                &session_id,
                &resolved.agent_name,
                &resolved.provider.kind,
                &resolved.provider.model,
                &resolved.source_path.display().to_string(),
                duration_ms,
                user_input.len(),
            ));
            Err(err)
        }
    }
}

#[derive(Debug)]
struct StructuredStepOutput {
    response_type: String,
    response_text: String,
    tool_call: Value,
}

fn build_structured_system_prompt(system_prompt: &str) -> String {
    format!(
        concat!(
            "{system_prompt}\n\n",
            "You must respond with exactly one JSON object and no surrounding prose.\n",
            "Allowed shapes:\n",
            "1. Final answer:\n",
            "{{\"type\":\"final\",\"content\":\"...\"}}\n",
            "2. Tool call:\n",
            "{{\"type\":\"tool_call\",\"tool\":\"browser.open\",\"arguments\":{{...}}}}\n",
            "3. Discovery request:\n",
            "{{\"type\":\"discover\",\"query\":\"...\",\"kind\":\"tool|skill_guidance|all\",\"owner\":\"optional\"}}\n",
            "Rules:\n",
            "- Use only the provided default tools unless you need more and then use type=discover.\n",
            "- If you use type=tool_call, tool must be one of the provided tools.\n",
            "- If you use type=discover, do not invent tools.\n",
            "- Never return pseudo-code like browser.open(...). Return valid JSON only."
        ),
        system_prompt = system_prompt.trim()
    )
}

fn collect_default_tools(catalog: &ActionCatalog) -> Vec<SearchResult> {
    let mut by_name = DEFAULT_TOOL_NAMES
        .iter()
        .filter_map(|name| {
            catalog
                .entries()
                .iter()
                .find(|entry| entry.name == *name)
                .map(|entry| SearchResult {
                    kind: entry.kind.as_str().to_string(),
                    owner: entry.owner.clone(),
                    runtime: entry.runtime.clone(),
                    manifest_path: entry.manifest_path.display().to_string(),
                    name: entry.name.clone(),
                    summary: entry.summary.clone(),
                    required: entry.required.clone(),
                    param_names: entry.param_names.clone(),
                    allowed_tools: entry.allowed_tools.clone(),
                    steps: entry.steps.clone(),
                    constraints: entry.constraints.clone(),
                    completion_criteria: entry.completion_criteria.clone(),
                    guidance: entry.guidance.clone(),
                    params: Some(entry.params.clone()),
                })
        })
        .collect::<Vec<_>>();
    by_name.push(discover_tool_result());
    by_name
}

fn render_default_tool_context(results: &[SearchResult]) -> Option<String> {
    if results.is_empty() {
        return None;
    }

    Some(
        results
            .iter()
            .map(render_tool_schema)
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

fn render_tool_schema(result: &SearchResult) -> String {
    let params = result
        .params
        .as_ref()
        .cloned()
        .unwrap_or_else(|| json!({"type": "object", "properties": {}, "required": []}));
    format!(
        concat!(
            "tool: {}\n",
            "kind: {}\n",
            "owner: {}\n",
            "summary: {}\n",
            "params_schema: {}"
        ),
        result.name, result.kind, result.owner, result.summary, params
    )
}

fn discover_tool_result() -> SearchResult {
    SearchResult {
        kind: "tool".to_string(),
        owner: "cortex".to_string(),
        runtime: "rust".to_string(),
        manifest_path: "services/cortex/service.json".to_string(),
        name: "cortex.discover".to_string(),
        summary: "Discover additional tools and guidance skills beyond the default six. Use kind=tool|skill_guidance|all."
            .to_string(),
        required: vec!["query".to_string()],
        param_names: vec![
            "query".to_string(),
            "kind".to_string(),
            "owner".to_string(),
            "limit".to_string(),
            "include_params".to_string(),
        ],
        allowed_tools: Vec::new(),
        steps: Vec::new(),
        constraints: Vec::new(),
        completion_criteria: Vec::new(),
        guidance: "Use this only when the default six tools are insufficient.".to_string(),
        params: Some(json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for tools and skills"
                },
                "kind": {
                    "type": "string",
                    "enum": ["tool", "skill_guidance", "all"],
                    "description": "Optional discovery mode. Default is all."
                },
                "owner": {
                    "type": "string",
                    "description": "Optional owner filter such as browser, sandbox, or skill folder"
                },
                "limit": {
                    "type": "number",
                    "description": "Max results to return"
                },
                "include_params": {
                    "type": "boolean",
                    "description": "Include full params schema for discovered tools"
                }
            },
            "required": ["query"]
        })),
    }
}

fn parse_step_model_output(raw: &str) -> Result<StructuredStepOutput> {
    let parsed: Value = serde_json::from_str(raw)
        .map_err(|err| anyhow!("cortex model output must be valid JSON: {err}"))?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| anyhow!("cortex model output must be a JSON object"))?;
    let response_type = obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    match response_type.as_str() {
        "final" => {
            let content = obj
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() {
                return Err(anyhow!("final response requires non-empty content"));
            }
            Ok(StructuredStepOutput {
                response_type,
                response_text: content,
                tool_call: Value::Null,
            })
        }
        "tool_call" => {
            let tool = obj
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if tool.is_empty() {
                return Err(anyhow!("tool_call response requires tool"));
            }
            let arguments = obj
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return Err(anyhow!("tool_call arguments must be an object"));
            }
            Ok(StructuredStepOutput {
                response_type,
                response_text: String::new(),
                tool_call: json!({
                    "tool": tool,
                    "arguments": arguments,
                }),
            })
        }
        "discover" => {
            let query = obj
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if query.is_empty() {
                return Err(anyhow!("discover response requires query"));
            }
            let kind = obj
                .get("kind")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("all");
            let owner = obj
                .get("owner")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            Ok(StructuredStepOutput {
                response_type,
                response_text: String::new(),
                tool_call: json!({
                    "tool": "cortex.discover",
                    "arguments": {
                        "query": query,
                        "kind": kind,
                        "owner": owner,
                    }
                }),
            })
        }
        _ => Err(anyhow!("unsupported cortex response type: {}", response_type)),
    }
}

pub fn handle_search_actions(params: Value, catalog: Arc<ActionCatalog>) -> Result<String> {
    let p = params
        .as_object()
        .ok_or_else(|| anyhow!("params must be an object"))?;
    let query = p
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if query.is_empty() {
        return Err(anyhow!("query is required"));
    }

    let kind = p
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|value| {
            if value == "all" {
                String::new()
            } else {
                value.to_string()
            }
        })
        .filter(|v| !v.is_empty());
    let owner = p
        .get("owner")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let include_params = p
        .get("include_params")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(8)
        .clamp(1, 25);

    let response = search_catalog(
        &catalog,
        SearchQuery {
            query,
            kind,
            owner,
            limit,
            include_params,
        },
    );
    Ok(serde_json::to_string(&response)?)
}

pub fn handle_search_tools(params: Value, catalog: Arc<ActionCatalog>) -> Result<String> {
    handle_search_actions(params, catalog)
}

pub fn handle_discover(params: Value, catalog: Arc<ActionCatalog>) -> Result<String> {
    handle_search_actions(params, catalog)
}

#[cfg(test)]
mod tests {
    use super::parse_step_model_output;

    #[test]
    fn parses_final_output() {
        let parsed = parse_step_model_output(r#"{"type":"final","content":"hello"}"#)
            .expect("final output should parse");
        assert_eq!(parsed.response_type, "final");
        assert_eq!(parsed.response_text, "hello");
        assert!(parsed.tool_call.is_null());
    }

    #[test]
    fn parses_tool_call_output() {
        let parsed = parse_step_model_output(
            r#"{"type":"tool_call","tool":"browser.open","arguments":{"url":"https://weather.com"}}"#,
        )
        .expect("tool_call output should parse");
        assert_eq!(parsed.response_type, "tool_call");
        assert_eq!(parsed.tool_call["tool"].as_str(), Some("browser.open"));
        assert_eq!(
            parsed.tool_call["arguments"]["url"].as_str(),
            Some("https://weather.com")
        );
    }

    #[test]
    fn parses_discover_output() {
        let parsed =
            parse_step_model_output(r#"{"type":"discover","query":"weather","kind":"all"}"#)
                .expect("discover output should parse");
        assert_eq!(parsed.response_type, "discover");
        assert_eq!(parsed.tool_call["tool"].as_str(), Some("cortex.discover"));
        assert_eq!(parsed.tool_call["arguments"]["query"].as_str(), Some("weather"));
    }
}
