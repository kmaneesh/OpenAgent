//! Tool definitions and MCP-lite handler registration for the Telegram service.

use crate::handlers::handle_send_message;
use crate::metrics::TelegramTelemetry;
use crate::state::TelegramState;
use sdk_rust::{McpLiteServer, ToolDefinition};
use serde_json::json;
use std::sync::Arc;

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "telegram.status".into(),
            description: "Return current Telegram service status.".into(),
            params: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "telegram.link_state".into(),
            description: "Return Telegram bot authorization and connection state.".into(),
            params: json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "telegram.send_message".into(),
            description: concat!(
                "Send a Telegram message to a user or chat. ",
                "user_id is the Telegram chat_id for private messages. ",
                "access_hash is accepted but ignored (Bot API does not require it)."
            )
            .into(),
            params: json!({
                "type": "object",
                "properties": {
                    "user_id":     { "type": "integer", "description": "Telegram user/chat ID." },
                    "access_hash": { "type": "integer", "description": "Ignored (Bot API)." },
                    "text":        { "type": "string",  "description": "Message text." }
                },
                "required": ["user_id", "text"]
            }),
        },
    ]
}

pub fn register_handlers(
    server: &mut McpLiteServer,
    state: Arc<TelegramState>,
    tel: Arc<TelegramTelemetry>,
) {
    let s = Arc::clone(&state);
    server.register_tool("telegram.status", move |_params| {
        Ok(s.status_json().to_string())
    });

    let s = Arc::clone(&state);
    server.register_tool("telegram.link_state", move |_params| {
        Ok(s.link_state_json().to_string())
    });

    let s = Arc::clone(&state);
    let t = Arc::clone(&tel);
    server.register_tool("telegram.send_message", move |params| {
        handle_send_message(params, Arc::clone(&s), Arc::clone(&t))
    });
}
