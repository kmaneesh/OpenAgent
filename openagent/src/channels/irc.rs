//! IRC channel — IRC over TLS via zeroclaw.
//!
//! Config block in `config/channels.toml`:
//! ```toml
//! [irc]
//! enabled   = true
//! server    = "irc.libera.chat"
//! port      = 6697
//! nickname  = "openagent"
//! channel   = "#your-channel"
//! password  = ""   # server password or NickServ password
//! ```

use std::sync::Arc;

use serde::Deserialize;
use zeroclaw::channels::irc::IrcChannelConfig;
use zeroclaw::channels::Channel;
use zeroclaw::channels::IrcChannel as Inner;

use crate::observability::telemetry::MetricsWriter;

use super::adapter::ZeroClawChannel;

#[derive(Debug, Deserialize)]
pub struct IrcConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub server: String,
    #[serde(default = "default_irc_port")]
    pub port: u16,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub username: Option<String>,
    /// IRC channels to join (e.g. ["#general", "#bots"]).
    #[serde(default)]
    pub channels: Vec<String>,
    /// Allowed nicknames. Empty = all. Use "*" for all.
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// Server password (for bouncers like ZNC).
    #[serde(default)]
    pub server_password: Option<String>,
    /// NickServ IDENTIFY password.
    #[serde(default)]
    pub nickserv_password: Option<String>,
    /// SASL PLAIN password (IRCv3).
    #[serde(default)]
    pub sasl_password: Option<String>,
    /// Verify TLS certificate (default: true).
    #[serde(default = "default_true")]
    pub verify_tls: bool,
}

impl Default for IrcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server: String::new(),
            port: default_irc_port(),
            nickname: String::new(),
            username: None,
            channels: vec![],
            allowed_users: vec![],
            server_password: None,
            nickserv_password: None,
            sasl_password: None,
            verify_tls: true,
        }
    }
}

fn default_irc_port() -> u16 { 6697 }
fn default_true() -> bool { true }

/// Build an IRC channel wrapped in the observability adapter.
pub fn build(cfg: &IrcConfig, metrics: Arc<MetricsWriter>) -> Arc<dyn Channel> {
    Arc::new(ZeroClawChannel::new(
        Inner::new(IrcChannelConfig {
            server: cfg.server.clone(),
            port: cfg.port,
            nickname: cfg.nickname.clone(),
            username: cfg.username.clone(),
            channels: cfg.channels.clone(),
            allowed_users: cfg.allowed_users.clone(),
            server_password: cfg.server_password.clone(),
            nickserv_password: cfg.nickserv_password.clone(),
            sasl_password: cfg.sasl_password.clone(),
            verify_tls: Some(cfg.verify_tls),
        }),
        metrics,
    ))
}
