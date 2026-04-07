use crate::config::MiddlewareConfig;
use crate::guard::GuardDb;
use crate::manager::ServiceManager;
use crate::telemetry::MetricsWriter;
use std::sync::Arc;
use std::time::Instant;

/// Shared application state injected into every Axum route and middleware.
#[derive(Clone, Debug)]
pub struct AppState {
    pub manager:    Arc<ServiceManager>,
    pub metrics:    MetricsWriter,
    pub config:     MiddlewareConfig,
    /// Inline guard whitelist — direct SQLite, no network hop.
    pub guard_db:   GuardDb,
    /// Process start time — used to compute uptime in /health.
    pub started_at: Arc<Instant>,
}

impl AppState {
    pub fn new(
        manager:  Arc<ServiceManager>,
        metrics:  MetricsWriter,
        config:   MiddlewareConfig,
        guard_db: GuardDb,
    ) -> Self {
        Self {
            manager,
            metrics,
            config,
            guard_db,
            started_at: Arc::new(Instant::now()),
        }
    }
}
