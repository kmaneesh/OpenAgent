pub mod codec;
pub mod otel;
pub mod server;
pub mod types;

pub use otel::{setup_otel, OTELGuard};
pub use server::McpLiteServer;
pub use types::{Frame, ToolDefinition};
