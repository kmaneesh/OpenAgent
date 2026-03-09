//! Serenity event dispatcher — wires gateway events into the MCP-lite event stream.

use crate::state::DiscordState;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};
use std::sync::{atomic::Ordering, Arc};
use tracing::info;

#[derive(Debug)]
pub struct Handler {
    pub state: Arc<DiscordState>,
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
        // Don't forward the bot's own messages — prevents agent echo loops.
        if msg.author.bot {
            return;
        }
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
            .send(sdk_rust::OutboundEvent::new("discord.message.received", data));
    }
}
