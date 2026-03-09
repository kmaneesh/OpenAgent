//! Discord MCP-lite service.
//!
//! Connects to Discord via Serenity (no cache, rustls) and exposes four tools
//! plus two event streams over a Unix Domain Socket using the MCP-lite protocol.
//!
//! # Tools
//! - `discord.status`       — service health snapshot
//! - `discord.link_state`   — connection/auth state
//! - `discord.send_message` — send a message to a channel
//! - `discord.edit_message` — edit an existing message
//!
//! # Events (pushed to Python on change)
//! - `discord.connection.status`  — emitted on Ready / disconnect / error
//! - `discord.message.received`   — emitted for every inbound message
//!
//! # Abort
//! Fatal on invalid socket path (OS-level bind failure).
//! Serenity reconnects automatically on gateway disconnect.

mod handler;
mod state;
mod tools;

use anyhow::Context as _;
use handler::Handler;
use mimalloc::MiMalloc;
use sdk_rust::McpLiteServer;
use serenity::prelude::*;
use state::DiscordState;
use std::sync::Arc;
use tracing::{error, info, warn};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn token_from_env() -> Option<String> {
    ["DISCORD_BOT_TOKEN", "OPENAGENT_DISCORD_BOT_TOKEN"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = token_from_env()
        .ok_or_else(|| anyhow::anyhow!("missing DISCORD_BOT_TOKEN or OPENAGENT_DISCORD_BOT_TOKEN"))?;

    let socket_path = std::env::var("OPENAGENT_SOCKET_PATH")
        .unwrap_or_else(|_| "data/sockets/discord.sock".to_string());

    let logs_dir = std::env::var("OPENAGENT_LOGS_DIR").unwrap_or_else(|_| "logs".to_string());

    if let Err(e) = sdk_rust::setup_otel("discord", &logs_dir) {
        eprintln!("{{\"level\":\"WARN\",\"message\":\"otel init failed\",\"error\":\"{e}\"}}");
    }

    // Build MCP-lite server and grab the event sender before serve() consumes it.
    let mut server = McpLiteServer::new(tools::make_tools(), "ready");
    let event_tx = server.event_sender();

    let state = Arc::new(DiscordState::new(event_tx));
    tools::register_handlers(&mut server, Arc::clone(&state));

    // Build Serenity client.
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler { state: Arc::clone(&state) })
        .await
        .context("failed to create Discord client")?;

    info!(socket = %socket_path, "discord.start");

    // Capture the shard manager before moving `client` into the spawn task so
    // we can request a clean gateway shutdown when the MCP-lite server exits.
    let shard_manager = Arc::clone(&client.shard_manager);

    // Run Serenity in the background; serve MCP-lite in the foreground.
    let discord_handle = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            error!(error = %e, "discord.client.error");
        }
    });

    let serve_result = server.serve(&socket_path).await;

    // Graceful shutdown: tell Serenity to close the gateway WebSocket before
    // aborting the task, so Discord doesn't see an unclean disconnect.
    shard_manager.shutdown_all().await;
    discord_handle.abort();

    if let Err(e) = serve_result {
        warn!(error = %e, "mcp.server.exit");
    }

    Ok(())
}
