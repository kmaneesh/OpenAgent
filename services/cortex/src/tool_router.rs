//! Tool router — dispatches tool calls from the Cortex ReAct loop to the
//! correct service socket using an inline MCP-lite client.
//!
//! The same JSON frame protocol that Python uses to call Cortex is used here by
//! Cortex to call downstream services.  No extra abstraction needed — just a plain
//! async connect → write ToolCallRequest → read ToolCallResponse.
//!
//! Socket routing is looked up from the `tool_name → socket_path` map built at
//! startup from all `service.json` manifests (via ActionCatalog).  This means the
//! tool namespace and the socket name are fully decoupled — a service can expose
//! tools under any name prefix without any routing special-cases here.

use anyhow::{anyhow, Result};
use sdk_rust::codec::{Decoder, Encoder};
use sdk_rust::types::Frame;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UnixStream;
use tokio::time::timeout;
use tracing::{info, warn};

/// Timeout for a single tool call (connect + write + read).
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Routes tool calls to the correct service socket.
///
/// Created once at startup from the ActionCatalog's tool→socket map.
#[derive(Debug, Clone)]
pub struct ToolRouter {
    /// Direct tool-name → absolute socket path lookup.
    /// Populated from service.json `socket` fields via ActionCatalog.
    tool_sockets: HashMap<String, PathBuf>,
    /// Fallback socket directory for tools not in the map (e.g. cortex self-calls).
    socket_dir: PathBuf,
}

impl ToolRouter {
    /// Create a router.
    ///
    /// `tool_sockets` maps every tool name to its socket path as declared in
    /// `service.json`.  `socket_dir` is the fallback for tools not in the map
    /// (prefix convention: `memory.search` → `<socket_dir>/memory.sock`).
    pub fn new(tool_sockets: HashMap<String, PathBuf>, socket_dir: PathBuf) -> Self {
        Self { tool_sockets, socket_dir }
    }

    /// Dispatch `tool` with `arguments` to the owning service.
    ///
    /// Returns the raw result string from the service, or an error JSON string
    /// suitable for feeding back into the LLM context (caller decides policy).
    pub async fn call(&self, tool: &str, arguments: &Value) -> Result<String> {
        let socket_path = self.resolve_socket(tool)?;
        info!(
            tool = %tool,
            socket = %socket_path.display(),
            "cortex.tool_router.call"
        );
        call_service(&socket_path, tool, arguments).await
    }

    /// Map a tool name to its socket path.
    ///
    /// Primary: direct lookup in the catalog-populated tool_sockets map.
    /// Fallback: prefix convention (`memory.search` → `<socket_dir>/memory.sock`).
    fn resolve_socket(&self, tool: &str) -> Result<PathBuf> {
        // 1. Direct lookup from service.json declarations (authoritative).
        if let Some(path) = self.tool_sockets.get(tool) {
            return Ok(path.clone());
        }

        // 2. Prefix fallback for cortex self-calls and any tool not in the catalog.
        if !tool.contains('.') {
            return Err(anyhow!(
                "tool name must have a dot-separated owner prefix: {tool}"
            ));
        }
        let owner = tool
            .split('.')
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("tool name must have a dot-separated owner prefix: {tool}"))?;
        Ok(self.socket_dir.join(format!("{owner}.sock")))
    }

    /// Returns true if the named service socket file exists on disk.
    /// Used to check availability without attempting a full connection.
    pub fn socket_exists(&self, tool: &str) -> bool {
        self.resolve_socket(tool)
            .map(|p| p.exists())
            .unwrap_or(false)
    }
}

/// Inline MCP-lite client — connects to a Unix socket, writes one ToolCallRequest
/// frame, reads the ToolCallResponse, and returns the result string.
///
/// Uses the same `sdk_rust::codec` and `sdk_rust::types::Frame` that the MCP-lite
/// server uses, so the wire format is identical to what Python sends to Cortex.
async fn call_service(socket_path: &Path, tool: &str, params: &Value) -> Result<String> {
    let stream = timeout(TOOL_CALL_TIMEOUT, UnixStream::connect(socket_path))
        .await
        .map_err(|_| {
            anyhow!(
                "connect to {} timed out after {}s",
                socket_path.display(),
                TOOL_CALL_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| anyhow!("connect to {}: {e}", socket_path.display()))?;

    let (read_half, write_half) = stream.into_split();
    let mut decoder = Decoder::new(read_half);
    let mut encoder = Encoder::new(write_half);
    let id = request_id();

    encoder
        .write_frame(&Frame::ToolCallRequest {
            id: id.clone(),
            tool: tool.to_string(),
            params: params.clone(),
            trace_id: None,
            span_id: None,
        })
        .await
        .map_err(|e| anyhow!("write tool call frame to {}: {e}", socket_path.display()))?;

    let frame = timeout(TOOL_CALL_TIMEOUT, decoder.next_frame())
        .await
        .map_err(|_| {
            anyhow!(
                "tool result from {} timed out after {}s",
                socket_path.display(),
                TOOL_CALL_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| anyhow!("read tool result from {}: {e}", socket_path.display()))?;

    let Some(frame) = frame else {
        return Err(anyhow!(
            "service at {} closed connection without responding",
            socket_path.display()
        ));
    };

    match frame {
        Frame::ToolCallResponse { id: resp_id, result, error } if resp_id == id => {
            if let Some(err) = error {
                warn!(tool = %tool, error = %err, "cortex.tool_router.service_error");
                return Err(anyhow!("tool {tool} returned error: {err}"));
            }
            Ok(result.unwrap_or_default())
        }
        Frame::ErrorResponse { id: resp_id, code, message } if resp_id == id => {
            Err(anyhow!("tool {tool} protocol error {code}: {message}"))
        }
        other => Err(anyhow!(
            "unexpected frame from {tool}: {other:?}"
        )),
    }
}

fn request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("cortex-tool-{nanos}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router_with_map(map: HashMap<String, PathBuf>) -> ToolRouter {
        ToolRouter::new(map, PathBuf::from("data/sockets"))
    }

    fn empty_router() -> ToolRouter {
        router_with_map(HashMap::new())
    }

    #[test]
    fn web_tool_maps_to_browser_sock_via_catalog_map() {
        // The browser service declares socket "data/sockets/browser.sock" in service.json.
        // The catalog populates the tool_sockets map; ToolRouter does a direct lookup.
        let mut map = HashMap::new();
        map.insert("web.search".to_string(), PathBuf::from("data/sockets/browser.sock"));
        map.insert("web.fetch".to_string(), PathBuf::from("data/sockets/browser.sock"));
        let r = router_with_map(map);
        assert_eq!(r.resolve_socket("web.search").unwrap(), PathBuf::from("data/sockets/browser.sock"));
        assert_eq!(r.resolve_socket("web.fetch").unwrap(), PathBuf::from("data/sockets/browser.sock"));
    }

    #[test]
    fn sandbox_tool_maps_to_sandbox_sock_via_prefix_fallback() {
        // sandbox.execute not in map → prefix fallback: sandbox → sandbox.sock
        let r = empty_router();
        let path = r.resolve_socket("sandbox.execute").unwrap();
        assert_eq!(path, PathBuf::from("data/sockets/sandbox.sock"));
    }

    #[test]
    fn memory_tool_maps_to_memory_sock_via_prefix_fallback() {
        let r = empty_router();
        let path = r.resolve_socket("memory.search").unwrap();
        assert_eq!(path, PathBuf::from("data/sockets/memory.sock"));
    }

    #[test]
    fn tool_without_prefix_returns_error() {
        let r = empty_router();
        assert!(r.resolve_socket("notool").is_err());
    }

    #[test]
    fn empty_tool_name_returns_error() {
        let r = empty_router();
        assert!(r.resolve_socket("").is_err());
    }
}
