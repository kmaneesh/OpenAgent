use sdk_rust::{McpLiteServer, ToolDefinition};
use serde_json::json;
use std::sync::Arc;

use crate::handlers::{
    handle_describe_boundary, handle_discover, handle_search_tools, handle_step, AppContext,
};

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "cortex.describe_boundary".to_string(),
            description: "Describe Cortex boundaries, ownership, and current implementation scope."
                .to_string(),
            params: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "cortex.step".to_string(),
            description: concat!(
                "Execute a reasoning step, optionally as a named worker agent. ",
                "Primary supervisor use: dispatch a research task to a specialised worker ",
                "by setting agent_name (e.g. search-agent, analysis-agent, code-agent). ",
                "The worker runs its own full ReAct loop and returns a result string. ",
                "Pass the task description as user_input, include user_key so the worker ",
                "has research context, and call research.task_done with the result after ",
                "the worker returns. Omit agent_name to run as the default agent."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "Stable session identifier for this turn"
                    },
                    "user_input": {
                        "type": "string",
                        "description": "Raw user message to send to Cortex"
                    },
                    "agent_name": {
                        "type": "string",
                        "description": "Optional configured agent name. Defaults to the first agent in openagent config."
                    },
                    "turn_kind": {
                        "type": "string",
                        "description": "Optional turn type. Use generation for normal LLM turns and tool_call for deterministic execution turns.",
                        "enum": ["generation", "tool_call"]
                    },
                    "user_key": {
                        "type": "string",
                        "description": "Optional user key for research context injection. Used to look up the active research DAG for this user. Defaults to session_id when omitted."
                    }
                },
                "required": ["session_id", "user_input"]
            }),
        },
        // cortex.discover and cortex.search_tools temporarily disabled for deterministic tool exposure only
        // ToolDefinition { ... cortex.discover ... },
        // ToolDefinition { ... cortex.search_tools ... },
    ]
}

pub fn register_handlers(server: &mut McpLiteServer, ctx: Arc<AppContext>) {
    server.register_tool("cortex.describe_boundary", |_params| {
        Ok(handle_describe_boundary())
    });

    let step_ctx = Arc::clone(&ctx);
    server.register_tool("cortex.step", move |params| {
        handle_step(params, Arc::clone(&step_ctx))
    });
    // cortex.discover and cortex.search_tools handler registration temporarily disabled
}
