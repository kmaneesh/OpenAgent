use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP-lite protocol frame types (match Go SDK and Python protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Frame {
    #[serde(rename = "tools.list")]
    ToolListRequest { id: String },

    #[serde(rename = "tools.list.ok")]
    ToolListResponse { id: String, tools: Vec<ToolDefinition> },

    #[serde(rename = "tool.call")]
    ToolCallRequest { id: String, tool: String, params: Value },

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
