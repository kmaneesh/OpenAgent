//! Slack channel — Slack Web API + Socket Mode via zeroclaw.
//!
//! Config block in `config/openagent.toml`:
//! ```toml
//! [channels.slack]
//! enabled                  = true
//! bot_token                = "${SLACK_BOT_TOKEN}"   # xoxb-…
//! app_token                = "${SLACK_APP_TOKEN}"   # xapp-… (Socket Mode)
//! channel_id               = ""                     # optional: single channel
//! channel_ids              = []                     # optional: multiple channels
//! allowed_users            = []
//! mention_only             = false
//! thread_replies           = true
//! interrupt_on_new_message = false
//! stream_drafts            = false
//! draft_update_interval_ms = 1200
//! use_markdown_blocks      = false
//! cancel_reaction          = ""
//! proxy_url                = ""
//! ```

use std::sync::Arc;

use serde::Deserialize;
use zeroclaw::channels::Channel;
use zeroclaw::channels::SlackChannel as Inner;

use crate::observability::telemetry::MetricsWriter;

use super::adapter::ZeroClawChannel;

#[derive(Debug, Default, Deserialize)]
pub struct SlackConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Slack bot OAuth token (xoxb-…).
    #[serde(default)]
    pub bot_token: String,
    /// App-level token for Socket Mode (xapp-…).
    #[serde(default)]
    pub app_token: String,
    /// Restrict to a single channel ID.
    #[serde(default)]
    pub channel_id: String,
    /// Restrict to multiple channel IDs (takes precedence over channel_id).
    #[serde(default)]
    pub channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// Only respond when @mentioned. Direct messages are always allowed.
    #[serde(default)]
    pub mention_only: bool,
    /// Reply in the originating thread (default: true).
    #[serde(default = "default_true")]
    pub thread_replies: bool,
    /// Cancel in-flight request when a newer message arrives from the same sender.
    #[serde(default)]
    pub interrupt_on_new_message: bool,
    /// Progressive draft streaming via `chat.update`.
    #[serde(default)]
    pub stream_drafts: bool,
    /// Minimum interval (ms) between draft edits.
    #[serde(default = "default_slack_draft_update_interval_ms")]
    pub draft_update_interval_ms: u64,
    /// Use Slack `markdown` block type (12K char limit, richer formatting).
    #[serde(default)]
    pub use_markdown_blocks: bool,
    /// Reaction name (without colons) that cancels an in-flight request.
    #[serde(default)]
    pub cancel_reaction: String,
    /// Per-channel proxy URL. Overrides global proxy.
    #[serde(default)]
    pub proxy_url: String,
}

fn default_true() -> bool { true }
fn default_slack_draft_update_interval_ms() -> u64 { 1200 }

/// Build a Slack channel wrapped in the observability adapter.
pub fn build(cfg: &SlackConfig, metrics: Arc<MetricsWriter>) -> Arc<dyn Channel> {
    let app_token = if cfg.app_token.is_empty() { None } else { Some(cfg.app_token.clone()) };
    let channel_id = if cfg.channel_id.is_empty() { None } else { Some(cfg.channel_id.clone()) };
    Arc::new(ZeroClawChannel::new(
        Inner::new(cfg.bot_token.clone(), app_token, channel_id, cfg.channel_ids.clone(), cfg.allowed_users.clone()),
        metrics,
    ))
}
