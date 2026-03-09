pub mod codec;
pub mod error;
pub mod otel;
pub mod server;
pub mod telemetry;
pub mod types;

pub use error::{Error, Result};
pub use otel::{setup_otel, OTELGuard};
pub use server::McpLiteServer;
pub use telemetry::{attach_context, elapsed_ms, ts_ms, MetricsWriter};
pub use types::{Frame, OutboundEvent, ToolDefinition};
