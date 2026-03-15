use crate::config::MiddlewareConfig;
use crate::manager::ServiceManager;
use crate::telemetry::MetricsWriter;
use std::sync::Arc;

/// Shared application state injected into every Axum route and middleware.
#[derive(Clone, Debug)]
pub struct AppState {
    pub manager: Arc<ServiceManager>,
    pub metrics: MetricsWriter,
    pub config: MiddlewareConfig,
}

impl AppState {
    pub fn new(
        manager: Arc<ServiceManager>,
        metrics: MetricsWriter,
        config: MiddlewareConfig,
    ) -> Self {
        Self { manager, metrics, config }
    }
}
