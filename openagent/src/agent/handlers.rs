use crate::agent::action::catalog::{ActionCatalog, ActionKind};
use crate::agent::action::search::{search_catalog, SearchQuery};
use crate::agent::core::{AgentCore, ToolEntry};
use crate::agent::classifier;
use crate::agent::config::AgentCoreConfig;
use crate::agent::llm::{build_llm_provider, build_prompt_with_skill_context};
use crate::agent::memory_adapter::{HybridMemoryAdapter, DEFAULT_STM_WINDOW};
use crate::agent::metrics::{elapsed_ms, step_err, step_ok, AgentTelemetry};
use crate::agent::tool_router::ToolRouter;
use anyhow::{anyhow, Result};
use autoagents_core::agent::task::Task;
use autoagents_core::agent::{BaseAgent, DirectAgent};
use autoagents_protocol::Event;
use opentelemetry::KeyValue;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::info;

const SKILL_SEARCH_LIMIT: usize = 8;
const PINNED_SKILLS: &[&str] = &["agent-browser"];

#[derive(Clone, Debug)]
pub struct AgentContext {
    tel: Arc<AgentTelemetry>,
    action_catalog: Arc<ActionCatalog>,
    tool_router: Arc<ToolRouter>,
    project_root: std::path::PathBuf,
}

impl AgentContext {
    pub fn new(
        tel: Arc<AgentTelemetry>,
        action_catalog: Arc<ActionCatalog>,
        tool_router: Arc<ToolRouter>,
        project_root: std::path::PathBuf,
    ) -> Self {
        Self { tel, action_catalog, tool_router, project_root }
    }

    pub fn tel(&self) -> Arc<AgentTelemetry> { Arc::clone(&self.tel) }
    pub fn action_catalog(&self) -> Arc<ActionCatalog> { Arc::clone(&self.action_catalog) }
    pub fn tool_router(&self) -> Arc<ToolRouter> { Arc::clone(&self.tool_router) }
    pub fn project_root(&self) -> &std::path::Path { &self.project_root }
}

pub fn handle_step(params: Value, ctx: Arc<AgentContext>) -> Result<String> {
    let tel = ctx.tel();
    let catalog = ctx.action_catalog();
    let router = ctx.tool_router();
    let p = params
        .as_object()
        .ok_or_else(|| anyhow!("params must be an object"))?;

    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow!("session_id is required"))?
        .to_string();
    let user_input = p
        .get("user_input")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow!("user_input is required"))?
        .to_string();
    let requested_agent = p.get("agent_name").and_then(|v| v.as_str()).map(str::trim);

    let _cx_guard = AgentTelemetry::attach_context(
        &params,
        vec![
            KeyValue::new("service", "agent"),
            KeyValue::new("op", "step"),
            KeyValue::new("session_id", session_id.clone()),
        ],
    );

    let span = tracing::info_span!(
        "agent.step",
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

    // Close any browser sessions left open from the previous step.
    let close_router = Arc::clone(&router);
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let _ = close_router.call("browser.close_all", &json!({})).await;
        })
    });

    let cfg_file = AgentCoreConfig::load()?;
    let resolved = cfg_file
        .cfg
        .resolve_step_config(cfg_file.path.clone(), requested_agent);

    let base_system_prompt = crate::agent::prompt::render_step_system(&resolved.system_prompt)
        .map_err(|e| anyhow!("system prompt render failed: {e}"))?;

    let (skill_context, tool_entries) = build_step_context(&catalog, &user_input);

    let structured_system_prompt =
        build_prompt_with_skill_context(&base_system_prompt, skill_context.clone()).system_prompt;

    let selected_provider = match &resolved.fast_provider {
        Some(fast) => {
            let tier = classifier::classify(&user_input);
            if tier == classifier::ProviderTier::Fast {
                info!(model = %fast.model, "agent.classifier.fast");
                fast.clone()
            } else {
                resolved.provider.clone()
            }
        }
        None => resolved.provider.clone(),
    };

    let data_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let diary_dir = data_root
        .join(&cfg_file.cfg.memory.diary_path)
        .join(&session_id);

    let agent_core = AgentCore::new(
        resolved.agent_name.clone(),
        structured_system_prompt,
        tool_entries.clone(),
        selected_provider.clone(),
        Arc::clone(&router),
        session_id.clone(),
        diary_dir,
    );

    span.record("agent_name", resolved.agent_name.as_str());
    span.record("provider_kind", selected_provider.kind.as_str());
    span.record("model", selected_provider.model.as_str());

    info!(
        agent_name = %resolved.agent_name,
        provider_kind = %selected_provider.kind,
        model = %selected_provider.model,
        config_path = %resolved.source_path.display(),
        tool_count = tool_entries.len(),
        has_skill_context = skill_context.is_some(),
        "agent.step.start"
    );

    let stm_dir = data_root.join("data").join("stm").join(&session_id);
    let memory_adapter = HybridMemoryAdapter::new(
        &session_id,
        DEFAULT_STM_WINDOW,
        stm_dir,
        Arc::clone(&router),
    );

    let llm_provider = build_llm_provider(&selected_provider)
        .map_err(|e| anyhow!("llm provider build failed: {e}"))?;
    let (tx, _rx) = mpsc::channel::<Event>(32);

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let base_agent =
                BaseAgent::<AgentCore, DirectAgent>::new(
                    agent_core,
                    llm_provider,
                    Some(Box::new(memory_adapter)),
                    tx,
                    false,
                )
                .await
                .map_err(|e| anyhow!("base agent construction failed: {e}"))?;

            base_agent.run(Task::new(&user_input)).await.map_err(|e| anyhow!("{e}"))
        })
    });

    match result {
        Ok(react_output) => {
            let duration_ms = elapsed_ms(started);
            span.record("status", "ok");
            span.record("duration_ms", duration_ms);
            span.record("output_len", react_output.response_text.len() as i64);
            info!(
                agent_name = %resolved.agent_name,
                provider_kind = %react_output.provider_kind,
                model = %react_output.model,
                duration_ms,
                output_len = react_output.response_text.len(),
                iterations = react_output.iterations,
                tool_calls = ?react_output.tool_calls_made,
                "agent.step.ok"
            );
            tel.record(&step_ok(
                &session_id,
                &resolved.agent_name,
                &react_output.provider_kind,
                &react_output.model,
                &resolved.source_path.display().to_string(),
                duration_ms,
                user_input.len(),
                react_output.response_text.len(),
            ));

            Ok(json!({
                "session_id": session_id,
                "agent_name": resolved.agent_name,
                "provider_kind": react_output.provider_kind,
                "model": react_output.model,
                "response_type": "final",
                "response_text": react_output.response_text,
                "react_summary": {
                    "iterations": react_output.iterations,
                    "tool_calls_made": react_output.tool_calls_made,
                    "tool_count": tool_entries.len(),
                    "tools": tool_entries.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
                }
            })
            .to_string())
        }
        Err(err) => {
            let duration_ms = elapsed_ms(started);
            span.record("status", "error");
            span.record("duration_ms", duration_ms);
            tracing::error!(
                agent_name = %resolved.agent_name,
                provider_kind = %selected_provider.kind,
                model = %selected_provider.model,
                duration_ms,
                error = %err,
                "agent.step.error"
            );
            tel.record(&step_err(
                &session_id,
                &resolved.agent_name,
                &selected_provider.kind,
                &selected_provider.model,
                &resolved.source_path.display().to_string(),
                duration_ms,
                user_input.len(),
            ));
            Err(err)
        }
    }
}

fn build_step_context(
    catalog: &ActionCatalog,
    user_input: &str,
) -> (Option<String>, Vec<ToolEntry>) {
    let search_results = search_catalog(
        catalog,
        SearchQuery {
            query: user_input.to_string(),
            kind: None,
            owner: None,
            limit: SKILL_SEARCH_LIMIT,
            include_params: false,
        },
    );

    let mut skill_lines: Vec<String> = search_results
        .results
        .iter()
        .filter(|r| r.kind == "skill_guidance")
        .map(|r| format!("skill: {}\ndescription: {}", r.name, r.summary))
        .collect();

    for skill_name in PINNED_SKILLS {
        let already_present = skill_lines.iter().any(|l| l.contains(skill_name));
        if !already_present {
            if let Some(entry) = catalog.entries().iter().find(|e| e.name == *skill_name) {
                skill_lines.push(format!("skill: {}\ndescription: {}", entry.name, entry.summary));
            }
        }
    }

    let skill_context = if skill_lines.is_empty() { None } else { Some(skill_lines.join("\n\n")) };

    // All catalog tools — no research tools (research service deleted).
    let mut tool_entries: Vec<ToolEntry> = catalog
        .entries()
        .iter()
        .filter(|e| {
            matches!(e.kind, ActionKind::Tool)
                && !e.params.is_null()
        })
        .map(|e| ToolEntry {
            name: e.name.clone(),
            description: e.summary.clone(),
            params: e.params.clone(),
        })
        .collect();

    // skill.read is always available — handled in-process by ToolRouter.
    tool_entries.push(skill_read_tool_entry());

    (skill_context, tool_entries)
}

fn skill_read_tool_entry() -> ToolEntry {
    ToolEntry {
        name: "skill.read".to_string(),
        description: "Load a skill's full body or a deep-dive reference file on demand. \
            Call with name only to get a table of contents; add reference/script/asset to \
            load a specific file."
            .to_string(),
        params: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name as shown in your context (e.g. agent-browser)"
                },
                "reference": {
                    "type": "string",
                    "description": "Reference file name (without .md) from the skill's references/ directory"
                },
                "script": {
                    "type": "string",
                    "description": "Script file name from the skill's scripts/ directory"
                },
                "asset": {
                    "type": "string",
                    "description": "Asset file name from the skill's assets/ directory"
                }
            },
            "required": ["name"]
        }),
    }
}

// ── skill.read handler ─────────────────────────────────────────────────────────

pub fn handle_skill_read(params: &Value, project_root: &std::path::Path) -> String {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(n) => n,
        None => return r#"{"error":"name is required"}"#.to_string(),
    };

    let skill_dir = project_root.join("skills").join(name);
    if !skill_dir.is_dir() {
        return json!({"error": format!("skill '{}' not found", name)}).to_string();
    }

    if let Some(file) = params.get("reference").and_then(Value::as_str) {
        let path = skill_dir.join("references").join(format!("{}.md", file));
        return serve_skill_file(name, "reference", file, &path);
    }

    if let Some(file) = params.get("script").and_then(Value::as_str) {
        let path = skill_dir.join("scripts").join(file);
        return serve_skill_file(name, "script", file, &path);
    }

    if let Some(file) = params.get("asset").and_then(Value::as_str) {
        let path = skill_dir.join("assets").join(file);
        return serve_skill_file(name, "asset", file, &path);
    }

    json!({
        "skill": name,
        "note": "Use the fields below to load specific bundled resources.",
        "references": list_dir_files(&skill_dir.join("references"), &["md"], name, "reference"),
        "scripts":    list_dir_files(&skill_dir.join("scripts"),    &["sh", "py", "js", "ts", "rb"], name, "script"),
        "assets":     list_dir_files(&skill_dir.join("assets"),     &[], name, "asset"),
    })
    .to_string()
}

fn serve_skill_file(skill: &str, kind: &str, file: &str, path: &std::path::Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(content) => json!({"skill": skill, kind: file, "content": content}).to_string(),
        Err(_) => json!({"error": format!("{} '{}' not found in skill '{}'", kind, file, skill)}).to_string(),
    }
}

fn list_dir_files(
    dir: &std::path::Path,
    extensions: &[&str],
    skill: &str,
    param: &str,
) -> Vec<serde_json::Value> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_file() {
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                if extensions.is_empty() || extensions.contains(&ext) {
                    return p.file_name().and_then(|s| s.to_str()).map(ToOwned::to_owned);
                }
            }
            None
        })
        .collect();
    names.sort();
    names
        .iter()
        .map(|n| json!({"file": n, "how_to_read": format!("skill.read(name=\"{}\", {}=\"{}\")", skill, param, n)}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_read_tool_entry_has_correct_name() {
        let entry = skill_read_tool_entry();
        assert_eq!(entry.name, "skill.read");
        assert!(entry.params["required"].as_array().unwrap().contains(&json!("name")));
    }

    #[test]
    fn handle_skill_read_missing_name_returns_error() {
        let result = handle_skill_read(&json!({}), std::path::Path::new("/tmp"));
        assert!(result.contains("error"));
        assert!(result.contains("name is required"));
    }

    #[test]
    fn handle_skill_read_unknown_skill_returns_error() {
        let result = handle_skill_read(
            &json!({"name": "nonexistent-skill-xyz"}),
            std::path::Path::new("/tmp"),
        );
        assert!(result.contains("error"));
        assert!(result.contains("not found"));
    }
}
