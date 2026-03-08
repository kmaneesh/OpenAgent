use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An unprompted event frame pushed from a service to the Python control plane.
///
/// Serialised as `{"type":"event","event":"<name>","data":{...}}`.
#[derive(Debug, Clone, Serialize)]
pub struct OutboundEvent {
    #[serde(rename = "type")]
    frame_type: &'static str,
    pub event: String,
    pub data: Value,
}

impl OutboundEvent {
    /// Create a new event frame with the given event name and JSON data payload.
    pub fn new(event: impl Into<String>, data: Value) -> Self {
        Self { frame_type: "event", event: event.into(), data }
    }
}

/// MCP-lite protocol frame types (match Go SDK and Python protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Frame {
    #[serde(rename = "tools.list")]
    ToolListRequest { id: String },

    #[serde(rename = "tools.list.ok")]
    ToolListResponse { id: String, tools: Vec<ToolDefinition> },

    #[serde(rename = "tool.call")]
    ToolCallRequest {
        id: String,
        tool: String,
        params: Value,
        /// Trace ID hex (32 chars) propagated from Python AgentLoop — enables distributed tracing.
        #[serde(default)]
        trace_id: Option<String>,
        /// Parent span ID hex (16 chars) propagated from Python AgentLoop.
        #[serde(default)]
        span_id: Option<String>,
    },

    #[serde(rename = "tool.result")]
    ToolCallResponse { id: String, result: Option<String>, error: Option<String> },

    #[serde(rename = "ping")]
    PingRequest { id: String },

    #[serde(rename = "pong")]
    PingResponse { id: String, status: String },

    #[serde(rename = "error")]
    ErrorResponse { id: String, code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub params: Value,
}

impl Frame {
    pub fn id(&self) -> &str {
        match self {
            Frame::ToolListRequest { id } => id,
            Frame::ToolListResponse { id, .. } => id,
            Frame::ToolCallRequest { id, .. } => id,
            Frame::ToolCallResponse { id, .. } => id,
            Frame::PingRequest { id } => id,
            Frame::PingResponse { id, .. } => id,
            Frame::ErrorResponse { id, .. } => id,
        }
    }
}
