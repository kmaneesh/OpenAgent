mod address;
mod traits;
mod discord;
mod slack;
mod telegram;
mod imessage;
mod irc;
mod mattermost;
mod signal;

use address::ChannelAddress;
use sdk_rust::McpLiteServer;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn, error};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let socket_path = std::env::var("OPENAGENT_SOCKET_PATH")
        .unwrap_or_else(|_| "data/sockets/channels.sock".to_string());

    let logs_dir = std::env::var("OPENAGENT_LOGS_DIR").unwrap_or_else(|_| "logs".to_string());
    
    let _otel_guard = sdk_rust::setup_otel("channels", &logs_dir)
        .inspect_err(|e| eprintln!("{{\"level\":\"WARN\",\"message\":\"otel init failed\",\"error\":\"{e}\"}}"))
        .ok();

    info!(socket = %socket_path, "channels.start");

    let mut server = McpLiteServer::new(
        vec![
            sdk_rust::Tool::new(
                "channel.send",
                "Send a message to a channel route address (e.g. discord://guild/channel)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["address", "content"]
                })
            ),
            sdk_rust::Tool::new(
                "channel.update_draft",
                "Update a streaming response draft (e.g. edit discord message)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": { "type": "string" },
                        "message_id": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["address", "message_id", "content"]
                })
            )
        ],
        "ready",
    );

    server.register_handler("channel.send", |params| async move {
        let address_str = params.get("address").and_then(|v| v.as_str()).unwrap_or_default();
        let _content = params.get("content").and_then(|v| v.as_str()).unwrap_or_default();

        let _addr = match ChannelAddress::parse(address_str) {
            Ok(a) => a,
            Err(e) => return Ok(format!("Failed to parse address: {}", e))
        };

        // TODO: route to specific platform instance
        Ok(format!("Sent to {} successfully (stub)", address_str))
    });

    server.register_handler("channel.update_draft", |params| async move {
        let address_str = params.get("address").and_then(|v| v.as_str()).unwrap_or_default();
        let _message_id = params.get("message_id").and_then(|v| v.as_str()).unwrap_or_default();
        let _content = params.get("content").and_then(|v| v.as_str()).unwrap_or_default();
        
        let _addr = match ChannelAddress::parse(address_str) {
            Ok(a) => a,
            Err(e) => return Ok(format!("Failed to parse address: {}", e))
        };

        // TODO: route to specific platform instance
        Ok(format!("Updated draft via {} successfully (stub)", address_str))
    });

    let serve_result = server.serve(&socket_path).await;

    if let Err(e) = serve_result {
        warn!(error = %e, "mcp.server.exit");
    }

    Ok(())
}

