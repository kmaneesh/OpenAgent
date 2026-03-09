//! Shared runtime state for the Telegram service.

use sdk_rust::OutboundEvent;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use teloxide::prelude::Bot;

const BACKEND: &str = "teloxide";

/// All mutable state shared between the event handler, MCP-lite tool handlers,
/// and the main task.  Access is always through `Arc<TelegramState>`.
#[derive(Debug)]
pub struct TelegramState {
    pub connected: AtomicBool,
    pub authorized: AtomicBool,
    pub last_error: Mutex<String>,
    pub bot: Mutex<Option<Bot>>,
    pub event_tx: tokio::sync::broadcast::Sender<OutboundEvent>,
}

impl TelegramState {
    pub fn new(event_tx: tokio::sync::broadcast::Sender<OutboundEvent>) -> Arc<Self> {
        Arc::new(Self {
            connected: AtomicBool::new(false),
            authorized: AtomicBool::new(false),
            last_error: Mutex::new(String::new()),
            bot: Mutex::new(None),
            event_tx,
        })
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
            "backend":    BACKEND,
        });
        let err = self.error_text();
        if !err.is_empty() {
            data["last_error"] = serde_json::Value::String(err);
        }
        let _ = self.event_tx.send(OutboundEvent::new("telegram.connection.status", data));
    }

    pub fn status_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            // Always true: if this handler is executing, the service is running.
            "running":    true,
            "connected":  self.connected.load(Ordering::Acquire),
            "authorized": self.authorized.load(Ordering::Acquire),
            "backend":    BACKEND,
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
            "backend":    BACKEND,
        })
    }
}
