use crate::config::MiddlewareConfig;
use crate::manager::ServiceManager;
use crate::session::SessionStore;
use crate::telemetry::MetricsWriter;
use std::sync::Arc;
use std::time::Instant;

/// Shared application state injected into every Axum route and middleware.
#[derive(Clone, Debug)]
pub struct AppState {
    pub manager: Arc<ServiceManager>,
    pub metrics: MetricsWriter,
    pub config: MiddlewareConfig,
    /// Process start time — used to compute uptime in /health.
    pub started_at: Arc<Instant>,
    /// Session store — writes turns to openagent.db.
    pub sessions: SessionStore,
}

impl AppState {
    pub fn new(
        manager: Arc<ServiceManager>,
        metrics: MetricsWriter,
        config: MiddlewareConfig,
        sessions: SessionStore,
    ) -> Self {
        Self { manager, metrics, config, started_at: Arc::new(Instant::now()), sessions }
    }
}
