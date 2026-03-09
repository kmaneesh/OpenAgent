//! Teloxide dispatcher — routes inbound Telegram messages to the event bus.
//!
//! Only private (DM) text messages are forwarded; group messages and non-text
//! updates are silently dropped.  The agent handles replies via `send_message`.

use sdk_rust::OutboundEvent;
use teloxide::prelude::*;
use tracing::info;

/// Forward a private text message to the MCP-lite event bus.
///
/// Bot API has no `access_hash`; we emit `0` for adapter compatibility
/// with the Python `TelegramChannelAdapter` which expects the field.
pub async fn on_message(
    _bot: Bot,
    msg: Message,
    event_tx: tokio::sync::broadcast::Sender<OutboundEvent>,
) -> Result<(), teloxide::RequestError> {
    // Only private (DM) messages
    if !matches!(msg.chat.kind, teloxide::types::ChatKind::Private(_)) {
        return Ok(());
    }

    let text = match msg.text() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return Ok(()),
    };

    let from_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
    let from_name = msg
        .from
        .as_ref()
        .map(|u| {
            let mut n = u.first_name.clone();
            if let Some(ref last) = u.last_name {
                if !last.is_empty() {
                    n.push(' ');
                    n.push_str(last);
                }
            }
            n
        })
        .unwrap_or_default();
    let username = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_deref())
        .unwrap_or("")
        .to_string();

    info!(from_id, from_name = %from_name, "telegram.message.received");

    let data = serde_json::json!({
        "message_id":  msg.id.0,
        "from_id":     from_id,
        "access_hash": 0i64,       // Bot API has no access_hash
        "from_name":   from_name,
        "username":    username,
        "text":        text,
    });
    let _ = event_tx.send(OutboundEvent::new("telegram.message.received", data));

    Ok(())
}
