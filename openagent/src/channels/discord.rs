//! Discord channel — Discord Bot Gateway via zeroclaw.
//!
//! Config block in `config/openagent.toml`:
//! ```toml
//! [channels.discord]
//! enabled                 = true
//! token                   = "${DISCORD_BOT_TOKEN}"
//! guild_id                = ""          # optional — restrict to one server
//! allowed_users           = []          # empty = all users
//! listen_to_bots          = false
//! mention_only            = false
//! interrupt_on_new_message = false
//! stream_mode             = "off"       # "off" | "partial" | "multi_message"
//! draft_update_interval_ms = 1000
//! multi_message_delay_ms  = 800
//! stall_timeout_secs      = 0
//! proxy_url               = ""          # optional per-channel proxy
//! ```

use std::sync::Arc;

use serde::Deserialize;
use zeroclaw::channels::Channel;
use zeroclaw::channels::DiscordChannel as Inner;

use crate::config::StreamMode;
use crate::observability::telemetry::MetricsWriter;

use super::adapter::ZeroClawChannel;

#[derive(Debug, Default, Deserialize)]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    /// Restrict to a specific guild (server). Empty string = all guilds.
    #[serde(default)]
    pub guild_id: String,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// Respond to messages from other bots (the bot still ignores itself).
    #[serde(default)]
    pub listen_to_bots: bool,
    /// Only respond when @mentioned.
    #[serde(default)]
    pub mention_only: bool,
    /// Cancel the in-flight request when a newer message arrives from the same sender.
    #[serde(default)]
    pub interrupt_on_new_message: bool,
    /// Progressive response delivery mode.
    #[serde(default)]
    pub stream_mode: StreamMode,
    /// Minimum interval (ms) between draft message edits (partial mode only).
    #[serde(default = "default_draft_update_interval_ms")]
    pub draft_update_interval_ms: u64,
    /// Delay (ms) between message chunks in multi_message mode.
    #[serde(default = "default_multi_message_delay_ms")]
    pub multi_message_delay_ms: u64,
    /// Stall-watchdog timeout in seconds. 0 = disabled.
    #[serde(default)]
    pub stall_timeout_secs: u64,
    /// Per-channel proxy URL (http/https/socks5). Overrides global proxy.
    #[serde(default)]
    pub proxy_url: String,
}

fn default_draft_update_interval_ms() -> u64 { 1000 }
fn default_multi_message_delay_ms() -> u64 { 800 }

/// Build a Discord channel wrapped in the observability adapter.
pub fn build(cfg: &DiscordConfig, metrics: Arc<MetricsWriter>) -> Arc<dyn Channel> {
    let guild_id = if cfg.guild_id.is_empty() { None } else { Some(cfg.guild_id.clone()) };
    Arc::new(ZeroClawChannel::new(
        Inner::new(cfg.token.clone(), guild_id, cfg.allowed_users.clone(), cfg.listen_to_bots, cfg.mention_only),
        metrics,
    ))
}
