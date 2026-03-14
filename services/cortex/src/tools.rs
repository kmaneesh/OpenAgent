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
                "Execute one Cortex Phase 1 reasoning step. ",
                "Loads the configured system prompt from OpenAgent config, ",
                "combines it with the user input, calls the configured LLM provider, ",
                "and returns plain response text without tool use or planning."
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
                    }
                },
                "required": ["session_id", "user_input"]
            }),
        },
        ToolDefinition {
            name: "cortex.discover".to_string(),
            description: concat!(
                "Discover additional tools and guidance skills beyond the default six tools. ",
                "Searches the boot-time Cortex action catalog from in-memory discovery without rescanning files."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query for action names, summaries, owners, skill steps, and params"
                    },
                    "kind": {
                        "type": "string",
                        "description": "Optional discovery kind filter: tool, skill_guidance, or all",
                        "enum": ["tool", "skill_guidance", "all"]
                    },
                    "owner": {
                        "type": "string",
                        "description": "Optional owner filter such as browser, sandbox, or a local skill folder name"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max results to return (default 8, max 25)"
                    },
                    "include_params": {
                        "type": "boolean",
                        "description": "Include full params schema for tool results"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "cortex.search_tools".to_string(),
            description: concat!(
                "Compatibility alias for cortex.discover. ",
                "Searches the boot-time Cortex action catalog and returns tools and guidance skills."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query for action names, summaries, owners, skill steps, and params"
                    },
                    "kind": {
                        "type": "string",
                        "description": "Optional discovery kind filter: tool, skill_guidance, or all",
                        "enum": ["tool", "skill_guidance", "all"]
                    },
                    "owner": {
                        "type": "string",
                        "description": "Optional owner filter such as browser, sandbox, or a local skill folder name"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max results to return (default 8, max 25)"
                    },
                    "include_params": {
                        "type": "boolean",
                        "description": "Include full params schema in each result"
                    }
                },
                "required": ["query"]
            }),
        },
    ]
}

pub fn register_handlers(server: &mut McpLiteServer, ctx: Arc<AppContext>) {
    server.register_tool("cortex.describe_boundary", |_params| {
        Ok(handle_describe_boundary())
    });

    let step_ctx = Arc::clone(&ctx);
    server.register_tool("cortex.step", move |params| {
        handle_step(params, step_ctx.tel(), step_ctx.action_catalog())
    });

    let discover_ctx = Arc::clone(&ctx);
    server.register_tool("cortex.discover", move |params| {
        handle_discover(params, discover_ctx.action_catalog())
    });

    let search_ctx = Arc::clone(&ctx);
    server.register_tool("cortex.search_tools", move |params| {
        handle_search_tools(params, search_ctx.action_catalog())
    });
}
