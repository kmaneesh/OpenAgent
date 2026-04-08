//! In-process channels module — replaces the standalone `services/channels/` daemon.
//!
//! Initialised once at startup via [`init`].  Returns a [`ChannelHandle`] that
//! callers (dispatch, HTTP handlers) use to send messages and typing indicators
//! without any TCP hop.
//!
//! Platform listeners push `message.received` events onto the `ServiceManager`'s
//! broadcast bus so the dispatch loop receives them exactly as before.

pub mod adapter;
pub mod address;
pub mod config;
pub mod registry;

// Re-export zeroclaw channel types so ZeroClaw-merged code that uses
// `crate::channels::*` continues to resolve without modification.
pub use zeroclaw::channels::Channel;
pub use zeroclaw::channels::SendMessage;
pub use zeroclaw::channels::DiscordChannel;
pub use zeroclaw::channels::TelegramChannel;
pub use zeroclaw::channels::SlackChannel;
pub use zeroclaw::channels::MattermostChannel;
pub use zeroclaw::channels::IrcChannel;
pub use zeroclaw::channels::SignalChannel;
pub use zeroclaw::channels::IMessageChannel;
pub mod traits {
    pub use zeroclaw::channels::traits::*;
}

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::observability::telemetry::MetricsWriter;

use self::address::ChannelAddress;
use self::config::ChannelsConfig;
use self::registry::ChannelRegistry;

/// Cheap clone handle — all operations are async and go directly to the platform.
#[derive(Clone)]
pub struct ChannelHandle {
    registry: Arc<ChannelRegistry>,
}

impl ChannelHandle {
    fn new(registry: Arc<ChannelRegistry>) -> Self {
        Self { registry }
    }

    /// Returns a handle backed by an empty registry (all operations return errors).
    /// Used as a fallback when `init()` fails so startup is not blocked.
    pub fn disabled() -> Self {
        // Build an empty registry directly.
        Self {
            registry: Arc::new(ChannelRegistry::empty()),
        }
    }

    /// Send a text message to a `ChannelAddress` URI (e.g. `telegram://bot/chat_id`).
    pub async fn send(&self, address: &str, content: &str) -> Result<()> {
        let addr = ChannelAddress::parse(address)?;
        let ch = self
            .registry
            .get(addr.platform())
            .ok_or_else(|| anyhow::anyhow!("no channel for platform: {}", addr.platform()))?;
        let msg = SendMessage::new(content.to_string(), addr.chat_id()).in_thread(addr.thread_id());
        ch.send(&msg).await
    }

    /// Start a typing indicator on the given address.
    pub async fn typing_start(&self, address: &str) -> Result<()> {
        let addr = ChannelAddress::parse(address)?;
        let ch = self
            .registry
            .get(addr.platform())
            .ok_or_else(|| anyhow::anyhow!("no channel for platform: {}", addr.platform()))?;
        ch.start_typing(addr.chat_id()).await
    }

    /// Stop the typing indicator on the given address.
    pub async fn typing_stop(&self, address: &str) -> Result<()> {
        let addr = ChannelAddress::parse(address)?;
        let ch = self
            .registry
            .get(addr.platform())
            .ok_or_else(|| anyhow::anyhow!("no channel for platform: {}", addr.platform()))?;
        ch.stop_typing(addr.chat_id()).await
    }

    /// Return names of all enabled platforms.
    pub fn platform_names(&self) -> Vec<String> {
        self.registry.all().iter().map(|c| c.name().to_string()).collect()
    }
}

/// Initialise the channels module.
///
/// - Loads `config/channels.toml` (env-interpolated).
/// - Builds `ChannelRegistry` from enabled platforms.
/// - Spawns per-platform listener tasks that push `message.received` events onto
///   `event_tx` — the same broadcast bus used by the `ServiceManager`.
///
/// Returns a [`ChannelHandle`] for outbound operations.
pub fn init(
    project_root: &std::path::Path,
    metrics: MetricsWriter,
    event_tx: broadcast::Sender<Value>,
) -> Result<ChannelHandle> {
    // Install rustls crypto provider (ring) before any TLS connection is made.
    // Safe to call multiple times — subsequent calls are no-ops.
    rustls::crypto::ring::default_provider().install_default().ok();

    let config_path = std::env::var("OPENAGENT_CHANNELS_CONFIG")
        .unwrap_or_else(|_| {
            project_root
                .join("config/channels.toml")
                .to_string_lossy()
                .into_owned()
        });

    let cfg = match config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "channels.config.fallback: using defaults (all disabled)");
            ChannelsConfig::default()
        }
    };

    let metrics_arc = Arc::new(metrics);
    let registry = Arc::new(ChannelRegistry::build(&cfg, Arc::clone(&metrics_arc))?);

    info!(count = registry.len(), "channels.registry.built");

    registry.spawn_listeners(event_tx);

    Ok(ChannelHandle::new(registry))
}
