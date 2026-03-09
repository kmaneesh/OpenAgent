//! Shared runtime state for the Discord service.

use sdk_rust::OutboundEvent;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

/// State shared between Serenity event handlers and MCP-lite tool handlers.
pub struct DiscordState {
    pub connected: AtomicBool,
    pub authorized: AtomicBool,
    pub last_error: Mutex<String>,
    /// HTTP client set once the Ready event fires; None before first connect.
    pub http: Mutex<Option<Arc<serenity::http::Http>>>,
    pub event_tx: tokio::sync::broadcast::Sender<OutboundEvent>,
}

impl std::fmt::Debug for DiscordState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordState")
            .field("connected", &self.connected)
            .field("authorized", &self.authorized)
            .finish()
    }
}

impl DiscordState {
    pub fn new(event_tx: tokio::sync::broadcast::Sender<OutboundEvent>) -> Self {
        Self {
            connected: AtomicBool::new(false),
            authorized: AtomicBool::new(false),
            last_error: Mutex::new(String::new()),
            http: Mutex::new(None),
            event_tx,
        }
    }

    pub fn set_error(&self, msg: &str) {
        *self.last_error.lock().expect("last_error poisoned") = msg.to_string();
    }

    pub fn error_text(&self) -> String {
        self.last_error.lock().expect("last_error poisoned").clone()
    }

    pub fn emit_connection_status(&self) {
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

    pub fn status_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            // Always true: if we can handle this call, the service is running.
            "running":    true,
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

    pub fn link_state_json(&self) -> serde_json::Value {
        serde_json::json!({
            "authorized": self.authorized.load(Ordering::Acquire),
            "connected":  self.connected.load(Ordering::Acquire),
            "backend":    "serenity",
        })
    }
}
