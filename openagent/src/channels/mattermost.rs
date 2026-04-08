//! Mattermost channel — Mattermost API v4 via zeroclaw.
//!
//! Config block in `config/channels.toml`:
//! ```toml
//! [mattermost]
//! enabled  = true
//! url      = "https://your.mattermost.server"
//! token    = "${MATTERMOST_BOT_TOKEN}"
//! ```

use std::sync::Arc;

use serde::Deserialize;
use zeroclaw::channels::Channel;
use zeroclaw::channels::MattermostChannel as Inner;

use crate::observability::telemetry::MetricsWriter;

use super::adapter::ZeroClawChannel;

#[derive(Debug, Default, Deserialize)]
pub struct MattermostConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub token: String,
    /// Optional channel ID to restrict the bot.
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// Reply in the originating thread (default: true).
    #[serde(default = "default_thread_replies")]
    pub thread_replies: bool,
    /// Only respond when @mentioned.
    #[serde(default)]
    pub mention_only: bool,
    /// Cancel in-flight request when a newer message arrives.
    #[serde(default)]
    pub interrupt_on_new_message: bool,
    /// Per-channel proxy URL.
    #[serde(default)]
    pub proxy_url: String,
}

fn default_thread_replies() -> bool { true }

/// Build a Mattermost channel wrapped in the observability adapter.
pub fn build(cfg: &MattermostConfig, metrics: Arc<MetricsWriter>) -> Arc<dyn Channel> {
    let channel_id = if cfg.channel_id.is_empty() { None } else { Some(cfg.channel_id.clone()) };
    Arc::new(ZeroClawChannel::new(
        Inner::new(cfg.url.clone(), cfg.token.clone(), channel_id, cfg.allowed_users.clone(), cfg.thread_replies, cfg.mention_only),
        metrics,
    ))
}
