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

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use anyhow::Context as _;
use sdk_rust::{McpLiteServer, OutboundEvent, ToolDefinition};
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId, id::MessageId},
    prelude::*,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::runtime::Handle;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Shared runtime state
// ---------------------------------------------------------------------------

/// State shared between Serenity event handlers and MCP-lite tool handlers.
struct DiscordState {
    started: AtomicBool,
    connected: AtomicBool,
    authorized: AtomicBool,
    last_error: Mutex<String>,
    /// HTTP client set once the Ready event fires; None before first connect.
    http: Mutex<Option<Arc<serenity::http::Http>>>,
    event_tx: tokio::sync::broadcast::Sender<OutboundEvent>,
}

impl std::fmt::Debug for DiscordState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordState")
            .field("started", &self.started)
            .field("connected", &self.connected)
            .field("authorized", &self.authorized)
            .finish()
    }
}

impl DiscordState {
    fn new(event_tx: tokio::sync::broadcast::Sender<OutboundEvent>) -> Self {
        Self {
            started: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            authorized: AtomicBool::new(false),
            last_error: Mutex::new(String::new()),
            http: Mutex::new(None),
            event_tx,
        }
    }

    fn set_error(&self, msg: &str) {
        *self.last_error.lock().expect("last_error poisoned") = msg.to_string();
    }

    fn error_text(&self) -> String {
        self.last_error.lock().expect("last_error poisoned").clone()
    }

    fn emit_connection_status(&self) {
        let mut data = serde_json::json!({
            "connected":  self.connected.load(Ordering::Acquire),
            "authorized": self.authorized.load(Ordering::Acquire),
            "backend":    "serenity",
        });
        let err = self.error_text();
        if !err.is_empty() {
            data["last_error"] = serde_json::Value::String(err);
        }
        // Err means no listeners — silently drop.
        let _ = self.event_tx.send(OutboundEvent::new("discord.connection.status", data));
    }

    fn status_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "running":    self.started.load(Ordering::Acquire),
            "connected":  self.connected.load(Ordering::Acquire),
            "authorized": self.authorized.load(Ordering::Acquire),
            "backend":    "serenity",
        });
        let err = self.error_text();
        if !err.is_empty() {
            v["last_error"] = serde_json::Value::String(err);
        }
        v
    }

    fn link_state_json(&self) -> serde_json::Value {
        serde_json::json!({
            "authorized": self.authorized.load(Ordering::Acquire),
            "connected":  self.connected.load(Ordering::Acquire),
            "backend":    "serenity",
        })
    }
}

// ---------------------------------------------------------------------------
// Serenity event handler
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Handler {
    state: Arc<DiscordState>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _data: Ready) {
        *self.state.http.lock().expect("http poisoned") = Some(Arc::clone(&ctx.http));
        self.state.connected.store(true, Ordering::Release);
        self.state.authorized.store(true, Ordering::Release);
        self.state.set_error("");
        info!("discord.ready");
        self.state.emit_connection_status();
    }

    async fn message(&self, _ctx: Context, msg: Message) {
        let data = serde_json::json!({
            "id":         msg.id.to_string(),
            "channel_id": msg.channel_id.to_string(),
            "guild_id":   msg.guild_id.map(|g| g.to_string()).unwrap_or_default(),
            "author_id":  msg.author.id.to_string(),
            "author":     msg.author.name,
            "content":    msg.content,
            "is_bot":     msg.author.bot,
        });
        // Err means no listeners — silently drop.
        let _ = self
            .state
            .event_tx
            .send(OutboundEvent::new("discord.message.received", data));
    }
}

// ---------------------------------------------------------------------------
// Tool handlers (sync — use block_in_place for async Serenity HTTP calls)
// ---------------------------------------------------------------------------

fn make_tools() -> Vec<ToolDefinition> {
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

fn register_handlers(server: &mut McpLiteServer, state: Arc<DiscordState>) {
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
        let channel_id = params["channel_id"].as_str().unwrap_or("").to_string();
        let text = params["text"].as_str().unwrap_or("").to_string();

        if channel_id.is_empty() {
            anyhow::bail!("channel_id is required");
        }
        if text.is_empty() {
            anyhow::bail!("text is required");
        }
        if !s.started.load(Ordering::Acquire) {
            anyhow::bail!("discord runtime is not started");
        }

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
        let channel_id = params["channel_id"].as_str().unwrap_or("").to_string();
        let message_id = params["message_id"].as_str().unwrap_or("").to_string();
        let text = params["text"].as_str().unwrap_or("").to_string();

        if channel_id.is_empty() || message_id.is_empty() {
            anyhow::bail!("channel_id and message_id are required");
        }
        if text.is_empty() {
            anyhow::bail!("text is required");
        }
        if !s.started.load(Ordering::Acquire) {
            anyhow::bail!("discord runtime is not started");
        }

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

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

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

    let logs_dir =
        std::env::var("OPENAGENT_LOGS_DIR").unwrap_or_else(|_| "logs".to_string());

    if let Err(e) = sdk_rust::setup_otel("discord", &logs_dir) {
        eprintln!("{{\"level\":\"WARN\",\"message\":\"otel init failed\",\"error\":\"{e}\"}}");
    }

    // Build MCP-lite server and grab the event sender before serve() consumes it.
    let mut server = McpLiteServer::new(make_tools(), "ready");
    let event_tx = server.event_sender();

    let state = Arc::new(DiscordState::new(event_tx));
    register_handlers(&mut server, Arc::clone(&state));

    // Build Serenity client.
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler { state: Arc::clone(&state) })
        .await
        .context("failed to create Discord client")?;

    state.started.store(true, Ordering::Release);
    info!(socket = %socket_path, "discord.start");

    // Run Serenity in the background; serve MCP-lite in the foreground.
    let discord_handle = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            error!(error = %e, "discord.client.error");
        }
    });

    let serve_result = server.serve(&socket_path).await;

    discord_handle.abort();

    if let Err(e) = serve_result {
        warn!(error = %e, "mcp.server.exit");
    }

    Ok(())
}
