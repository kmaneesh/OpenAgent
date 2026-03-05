use crate::codec::{Decoder, Encoder};
use crate::types::{Frame, ToolDefinition};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

type ToolHandler = Box<dyn Fn(serde_json::Value) -> Result<String> + Send + Sync>;

pub struct McpLiteServer {
    tools: Vec<ToolDefinition>,
    handlers: HashMap<String, ToolHandler>,
    status: String,
}

impl McpLiteServer {
    pub fn new(tools: Vec<ToolDefinition>, status: &str) -> Self {
        Self {
            tools,
            handlers: HashMap::new(),
            status: status.to_string(),
        }
    }

    pub fn register_tool<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(serde_json::Value) -> Result<String> + Send + Sync + 'static,
    {
        self.handlers.insert(name.to_string(), Box::new(handler));
    }

    pub async fn handle_request(&self, frame: Frame) -> Result<Frame> {
        match frame {
            Frame::PingRequest { id } => Ok(Frame::PingResponse {
                id,
                status: self.status.clone(),
            }),
            Frame::ToolListRequest { id } => Ok(Frame::ToolListResponse {
                id,
                tools: self.tools.clone(),
            }),
            Frame::ToolCallRequest { id, tool, params } => {
                if let Some(handler) = self.handlers.get(&tool) {
                    match handler(params) {
                        Ok(res) => Ok(Frame::ToolCallResponse {
                            id,
                            result: Some(res),
                            error: None,
                        }),
                        Err(err) => Ok(Frame::ToolCallResponse {
                            id,
                            result: None,
                            error: Some(err.to_string()),
                        }),
                    }
                } else {
                    Ok(Frame::ErrorResponse {
                        id,
                        code: "TOOL_NOT_FOUND".to_string(),
                        message: format!("Tool {} not found", tool),
                    })
                }
            }
            _ => anyhow::bail!("Unsupported frame type"),
        }
    }

    pub async fn serve(self, socket_path: &str) -> Result<()> {
        let path = Path::new(socket_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create socket directory")?;
        }
        
        if path.exists() {
            fs::remove_file(path).context("Failed to remove stale socket")?;
        }

        let listener = UnixListener::bind(socket_path).context("Failed to bind to unix socket")?;
        info!("Service listening on {}", socket_path);

        let server = Arc::new(self);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let server_clone = server.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, server_clone).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

async fn handle_connection(stream: tokio::net::UnixStream, server: Arc<McpLiteServer>) -> Result<()> {
    let (read_half, write_half) = stream.into_split();
    let mut decoder = Decoder::new(read_half);
    let encoder = Arc::new(Mutex::new(Encoder::new(write_half)));

    while let Ok(Some(frame)) = decoder.next_frame().await {
        let server = server.clone();
        let encoder = encoder.clone();

        tokio::spawn(async move {
            let id = frame.id().to_string();
            match server.handle_request(frame).await {
                Ok(response) => {
                    let mut enc = encoder.lock().await;
                    if let Err(e) = enc.write_frame(&response).await {
                        error!("Failed to send response for {}: {}", id, e);
                    }
                }
                Err(e) => {
                    warn!("Failed to handle request {}: {}", id, e);
                    let err_response = Frame::ErrorResponse {
                        id,
                        code: "INTERNAL_ERROR".to_string(),
                        message: e.to_string(),
                    };
                    let mut enc = encoder.lock().await;
                    let _ = enc.write_frame(&err_response).await;
                }
            }
        });
    }

    Ok(())
}
