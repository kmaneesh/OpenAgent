//! Telegram channel — Telegram Bot API via zeroclaw.
//!
//! Config block in `config/openagent.toml`:
//! ```toml
//! [channels.telegram]
//! enabled                  = true
//! bot_token                = "${TELEGRAM_BOT_TOKEN}"
//! allowed_users            = []      # empty = pairing-code flow
//! mention_only             = false
//! interrupt_on_new_message = false
//! stream_mode              = "off"   # "off" | "partial" | "multi_message"
//! draft_update_interval_ms = 1000
//! ack_reactions            = false
//! proxy_url                = ""
//! ```

use std::sync::Arc;

use serde::Deserialize;
use zeroclaw::channels::Channel;
use zeroclaw::channels::TelegramChannel as Inner;

use crate::config::StreamMode;
use crate::observability::telemetry::MetricsWriter;

use super::adapter::ZeroClawChannel;

#[derive(Debug, Default, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Telegram Bot API token (from @BotFather).
    /// Field is `bot_token` to match zeroclaw 0.6.8 naming.
    #[serde(default, alias = "token")]
    pub bot_token: String,
    /// Allowed Telegram user IDs or usernames. Empty = pairing-code flow.
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// Only respond when the bot is @mentioned in groups.
    #[serde(default)]
    pub mention_only: bool,
    /// Cancel in-flight request when a newer message arrives from the same sender.
    #[serde(default)]
    pub interrupt_on_new_message: bool,
    /// Progressive response delivery mode.
    #[serde(default)]
    pub stream_mode: StreamMode,
    /// Minimum interval (ms) between draft message edits.
    #[serde(default = "default_draft_update_interval_ms")]
    pub draft_update_interval_ms: u64,
    /// Send an emoji reaction to acknowledge inbound messages.
    #[serde(default)]
    pub ack_reactions: bool,
    /// Per-channel proxy URL. Overrides global proxy.
    #[serde(default)]
    pub proxy_url: String,
}

fn default_draft_update_interval_ms() -> u64 { 1000 }

/// Build a Telegram channel wrapped in the observability adapter.
pub fn build(cfg: &TelegramConfig, metrics: Arc<MetricsWriter>) -> Arc<dyn Channel> {
    Arc::new(ZeroClawChannel::new(
        Inner::new(cfg.bot_token.clone(), cfg.allowed_users.clone(), cfg.mention_only),
        metrics,
    ))
}
