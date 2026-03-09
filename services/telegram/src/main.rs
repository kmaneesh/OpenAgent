//! Telegram MCP-lite service.
//!
//! Connects to Telegram via Teloxide (Bot API, long polling) and exposes tools
//! plus event streams over a Unix Domain Socket using the MCP-lite protocol.
//!
//! # Tools
//! - `telegram.status`       — service health snapshot
//! - `telegram.link_state`   — connection/auth state
//! - `telegram.send_message` — send a message to a user (user_id = chat_id for DMs)
//!
//! # Events (pushed to Python on change)
//! - `telegram.connection.status`  — emitted on connect / disconnect / error
//! - `telegram.message.received`   — emitted for every inbound private DM
//!
//! # Environment variables
//! - `TELEGRAM_BOT_TOKEN` / `OPENAGENT_TELEGRAM_BOT_TOKEN` / `TELOXIDE_TOKEN`
//! - `OPENAGENT_SOCKET_PATH` (default: `data/sockets/telegram.sock`)
//! - `OPENAGENT_LOGS_DIR`    (default: `logs`)

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod dispatch;
mod handlers;
mod metrics;
mod state;
mod tools;

use anyhow::Context as _;
use metrics::TelegramTelemetry;
use sdk_rust::{setup_otel, McpLiteServer};
use state::TelegramState;
use std::sync::{atomic::Ordering, Arc};
use teloxide::prelude::*;
use tracing::{info, warn};

fn token_from_env() -> Option<String> {
    ["TELEGRAM_BOT_TOKEN", "OPENAGENT_TELEGRAM_BOT_TOKEN", "TELOXIDE_TOKEN"]
        .into_iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = token_from_env().ok_or_else(|| {
        anyhow::anyhow!("missing TELEGRAM_BOT_TOKEN, OPENAGENT_TELEGRAM_BOT_TOKEN, or TELOXIDE_TOKEN")
    })?;

    let socket_path = std::env::var("OPENAGENT_SOCKET_PATH")
        .unwrap_or_else(|_| "data/sockets/telegram.sock".to_string());

    let logs_dir = std::env::var("OPENAGENT_LOGS_DIR").unwrap_or_else(|_| "logs".to_string());

    if let Err(e) = setup_otel("telegram", &logs_dir) {
        eprintln!("{{\"level\":\"WARN\",\"message\":\"otel init failed\",\"error\":\"{e}\"}}");
    }

    let tel = Arc::new(
        TelegramTelemetry::new(&logs_dir).context("failed to init telegram telemetry")?,
    );

    let mut server = McpLiteServer::new(tools::make_tools(), "ready");
    let event_tx = server.event_sender();

    let state = TelegramState::new(event_tx.clone());
    tools::register_handlers(&mut server, Arc::clone(&state), tel);

    let bot = Bot::new(&token);

    // Verify bot token and populate shared state
    match bot.get_me().await {
        Ok(me) => {
            *state.bot.lock().expect("bot poisoned") = Some(bot.clone());
            state.connected.store(true, Ordering::Release);
            state.authorized.store(true, Ordering::Release);
            state.set_error("");
            info!(username = %me.username.as_deref().unwrap_or(""), "telegram.auth_ok");
        }
        Err(e) => {
            state.set_error(&e.to_string());
            state.emit_connection_status();
            return Err(e).context("telegram get_me failed");
        }
    }
    state.emit_connection_status();

    info!(socket = %socket_path, "telegram.start");

    // Run the Teloxide dispatcher in the background
    let event_tx_bg = event_tx.clone();
    let telegram_handle = tokio::spawn(async move {
        use teloxide::dispatching::UpdateFilterExt;
        use teloxide::types::Update;

        let schema = Update::filter_message().endpoint(move |bot: Bot, msg: Message| {
            let tx = event_tx_bg.clone();
            async move { dispatch::on_message(bot, msg, tx).await }
        });

        Dispatcher::builder(bot, schema).build().dispatch().await;
    });

    let serve_result = server.serve(&socket_path).await;

    telegram_handle.abort();

    if let Err(e) = serve_result {
        warn!(error = %e, "mcp.server.exit");
    }

    Ok(())
}
