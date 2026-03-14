//! AutoAgents ToolT stub implementations for Phase 1B.
//!
//! Each tool returns fixed "stub" JSON.  Phase 2+ wires real MCP-lite calls to
//! the respective service sockets (memory, sandbox, browser).
//!
//! `ActionDispatcherTool` is the dynamic meta-tool: the LLM emits
//! `{"name": "<action>", "args": {...}}` and the dispatcher routes it.  Phase 2+
//! looks up the `ActionCatalog` and forwards over MCP-lite to the owning service.

use async_trait::async_trait;
use autoagents_core::tool::{ToolCallError, ToolRuntime, ToolT};
use serde_json::{json, Value};

// ── MemorySearchTool ──────────────────────────────────────────────────────────

/// Thin ToolT wrapper for semantic memory search.
/// Phase 3+ wires a real MCP-lite call to the memory service.
#[derive(Debug)]
pub struct MemorySearchTool;

#[async_trait]
impl ToolRuntime for MemorySearchTool {
    async fn execute(&self, _args: Value) -> Result<Value, ToolCallError> {
        Ok(json!({
            "status": "stub",
            "results": [],
            "message": "memory.search stub — Phase 3 wires MCP-lite call to memory service"
        }))
    }
}

impl ToolT for MemorySearchTool {
    fn name(&self) -> &str {
        "memory.search"
    }

    fn description(&self) -> &str {
        "Search semantic memory for relevant context. Returns top-k episodes and observations."
    }

    fn args_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }
}

// ── SandboxExecuteTool ────────────────────────────────────────────────────────

/// Thin ToolT wrapper for VM-isolated code execution via the sandbox service.
/// Phase 2+ wires a real MCP-lite call to `services/sandbox`.
#[derive(Debug)]
pub struct SandboxExecuteTool;

#[async_trait]
impl ToolRuntime for SandboxExecuteTool {
    async fn execute(&self, _args: Value) -> Result<Value, ToolCallError> {
        Ok(json!({
            "status": "stub",
            "stdout": "",
            "stderr": "",
            "exit_code": 0,
            "message": "sandbox.execute stub — Phase 2 wires MCP-lite call to sandbox service"
        }))
    }
}

impl ToolT for SandboxExecuteTool {
    fn name(&self) -> &str {
        "sandbox.execute"
    }

    fn description(&self) -> &str {
        "Execute code in a VM-isolated sandbox. Supports Python and Node.js."
    }

    fn args_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["python", "node"],
                    "description": "Programming language to run"
                },
                "code": {
                    "type": "string",
                    "description": "Source code to execute"
                }
            },
            "required": ["language", "code"]
        })
    }
}

// ── BrowserNavigateTool ───────────────────────────────────────────────────────

/// Thin ToolT wrapper for headless browser navigation via the browser service.
/// Phase 2+ wires a real MCP-lite call to `services/browser`.
#[derive(Debug)]
pub struct BrowserNavigateTool;

#[async_trait]
impl ToolRuntime for BrowserNavigateTool {
    async fn execute(&self, _args: Value) -> Result<Value, ToolCallError> {
        Ok(json!({
            "status": "stub",
            "url": null,
            "title": null,
            "message": "browser.navigate stub — Phase 2 wires MCP-lite call to browser service"
        }))
    }
}

impl ToolT for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser.navigate"
    }

    fn description(&self) -> &str {
        "Navigate the headless browser to a URL and return the page title."
    }

    fn args_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to navigate to"
                }
            },
            "required": ["url"]
        })
    }
}

// ── ActionDispatcherTool ──────────────────────────────────────────────────────

/// Dynamic meta-tool.  The LLM emits `{"name": "<action>", "args": {...}}` and
/// this tool routes the call.
///
/// Phase 2+: look up the `ActionCatalog` at call time and forward over MCP-lite
/// to the owning service.  The meta-tool keeps the LLM context lean — only action
/// summaries are injected, not every full tool schema.
#[derive(Debug)]
pub struct ActionDispatcherTool;

#[async_trait]
impl ToolRuntime for ActionDispatcherTool {
    async fn execute(&self, args: Value) -> Result<Value, ToolCallError> {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
            .to_owned();

        Ok(json!({
            "status": "stub",
            "action": name,
            "message": "action.call stub — Phase 2 wires ActionCatalog routing over MCP-lite"
        }))
    }
}

impl ToolT for ActionDispatcherTool {
    fn name(&self) -> &str {
        "action.call"
    }

    fn description(&self) -> &str {
        concat!(
            "Dispatch a named action from the action catalog. ",
            "Use this when you need an action beyond the default candidate set. ",
            "Supply the action name exactly as returned by cortex.discover."
        )
    }

    fn args_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Fully-qualified action name (e.g. browser.open, sandbox.shell)"
                },
                "args": {
                    "type": "object",
                    "description": "Action arguments as defined in the action schema"
                }
            },
            "required": ["name", "args"]
        })
    }
}

/// Construct the default tool set for Phase 1B CortexAgent.
/// All tools are stubs — Phase 2+ wires live MCP-lite calls.
pub fn default_tools() -> Vec<Box<dyn ToolT>> {
    vec![
        Box::new(MemorySearchTool),
        Box::new(SandboxExecuteTool),
        Box::new(BrowserNavigateTool),
        Box::new(ActionDispatcherTool),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_stable() {
        assert_eq!(MemorySearchTool.name(), "memory.search");
        assert_eq!(SandboxExecuteTool.name(), "sandbox.execute");
        assert_eq!(BrowserNavigateTool.name(), "browser.navigate");
        assert_eq!(ActionDispatcherTool.name(), "action.call");
    }

    #[test]
    fn args_schema_are_valid_json_objects() {
        for tool in default_tools() {
            let schema = tool.args_schema();
            assert!(schema.is_object(), "tool {} schema must be a JSON object", tool.name());
            assert_eq!(
                schema.get("type").and_then(Value::as_str),
                Some("object"),
                "tool {} schema must have type=object",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn stub_tools_return_status_stub() {
        for tool in default_tools() {
            let result = tool.execute(json!({})).await;
            assert!(result.is_ok(), "tool {} stub execute failed", tool.name());
            let output = result.unwrap();
            assert_eq!(
                output.get("status").and_then(Value::as_str),
                Some("stub"),
                "tool {} stub must return status=stub",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn action_dispatcher_extracts_name_from_args() {
        let tool = ActionDispatcherTool;
        let result = tool
            .execute(json!({"name": "browser.open", "args": {"url": "https://example.com"}}))
            .await
            .unwrap();
        assert_eq!(result.get("action").and_then(Value::as_str), Some("browser.open"));
    }
}
