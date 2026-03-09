//! Tool definitions and MCP-lite handler registration for the Discord service.

use crate::state::DiscordState;
use anyhow::Context as _;
use sdk_rust::{McpLiteServer, ToolDefinition};
use serenity::model::id::{ChannelId, MessageId};
use std::sync::Arc;
// Handle::current().block_on() bridges sync tool handlers to async Serenity HTTP.
use tokio::runtime::Handle;

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "discord.status".into(),
            description: "Return current Discord service status.".into(),
            params: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "discord.link_state".into(),
            description: "Return current Discord connection and auth state.".into(),
            params: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolDefinition {
            name: "discord.send_message".into(),
            description: "Send a message to a Discord channel.".into(),
            params: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": { "type": "string", "description": "Discord channel ID." },
                    "text":       { "type": "string", "description": "Message text." }
                },
                "required": ["channel_id", "text"]
            }),
        },
        ToolDefinition {
            name: "discord.edit_message".into(),
            description: "Edit an existing Discord message.".into(),
            params: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": { "type": "string", "description": "Discord channel ID." },
                    "message_id": { "type": "string", "description": "Discord message ID to edit." },
                    "text":       { "type": "string", "description": "New message text." }
                },
                "required": ["channel_id", "message_id", "text"]
            }),
        },
    ]
}

pub fn register_handlers(server: &mut McpLiteServer, state: Arc<DiscordState>) {
    let s = Arc::clone(&state);
    server.register_tool("discord.status", move |_params| {
        Ok(s.status_json().to_string())
    });

    let s = Arc::clone(&state);
    server.register_tool("discord.link_state", move |_params| {
        Ok(s.link_state_json().to_string())
    });

    let s = Arc::clone(&state);
    server.register_tool("discord.send_message", move |params| {
        let channel_id = params["channel_id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("channel_id is required"))?
            .to_string();
        let text = params["text"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("text is required"))?
            .to_string();

        let http = s.http.lock().expect("http poisoned").clone();
        let http = http.ok_or_else(|| anyhow::anyhow!("discord not connected"))?;

        let cid: u64 = channel_id.parse().context("invalid channel_id")?;

        let msg = tokio::task::block_in_place(|| {
            Handle::current().block_on(ChannelId::new(cid).say(&*http, &text))
        })
        .map_err(|e| {
            s.set_error(&e.to_string());
            s.emit_connection_status();
            anyhow::anyhow!("{e}")
        })?;

        Ok(serde_json::json!({
            "ok":         true,
            "id":         msg.id.to_string(),
            "channel_id": msg.channel_id.to_string(),
        })
        .to_string())
    });

    let s = Arc::clone(&state);
    server.register_tool("discord.edit_message", move |params| {
        let channel_id = params["channel_id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("channel_id is required"))?
            .to_string();
        let message_id = params["message_id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("message_id is required"))?
            .to_string();
        let text = params["text"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("text is required"))?
            .to_string();

        let http = s.http.lock().expect("http poisoned").clone();
        let http = http.ok_or_else(|| anyhow::anyhow!("discord not connected"))?;

        let cid: u64 = channel_id.parse().context("invalid channel_id")?;
        let mid: u64 = message_id.parse().context("invalid message_id")?;

        let msg = tokio::task::block_in_place(|| {
            Handle::current().block_on(
                ChannelId::new(cid).edit_message(
                    &*http,
                    MessageId::new(mid),
                    serenity::builder::EditMessage::new().content(&text),
                ),
            )
        })
        .map_err(|e| {
            s.set_error(&e.to_string());
            s.emit_connection_status();
            anyhow::anyhow!("{e}")
        })?;

        Ok(serde_json::json!({
            "ok":         true,
            "id":         msg.id.to_string(),
            "channel_id": msg.channel_id.to_string(),
        })
        .to_string())
    });
}
