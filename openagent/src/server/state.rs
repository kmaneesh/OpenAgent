use crate::agent::handlers::AgentContext;
use crate::channels::ChannelHandle;
use crate::config::{MiddlewareConfig, OpenAgentConfig};
use crate::guard::GuardDb;
use crate::service::manifest::ServiceManifest;
use crate::service::ServiceManager;
use crate::observability::telemetry::MetricsWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Shared application state injected into every Axum route and middleware.
#[derive(Clone, Debug)]
pub struct AppState {
    pub manager:      Arc<ServiceManager>,
    pub metrics:      MetricsWriter,
    pub config:       MiddlewareConfig,
    /// Inline guard whitelist — direct SQLite, no network hop.
    pub guard_db:     GuardDb,
    /// Process start time — used to compute uptime in /health.
    pub started_at:   Arc<Instant>,
    /// In-process agent context — AgentLayer and dispatch loop call handle_step directly.
    pub agent_ctx:    Arc<AgentContext>,
    /// In-process channel handle — webhook routes inject inbound events here.
    pub channel_handle: ChannelHandle,
    /// Project root — used by the doctor module to locate config, data, services, skills.
    pub project_root: Arc<PathBuf>,
    /// Full loaded config — passed to doctor::diagnose.
    pub full_config:  Arc<OpenAgentConfig>,
    /// Discovered service manifests — passed to doctor::diagnose.
    pub manifests:    Arc<Vec<ServiceManifest>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manager:        Arc<ServiceManager>,
        metrics:        MetricsWriter,
        config:         MiddlewareConfig,
        guard_db:       GuardDb,
        agent_ctx:      Arc<AgentContext>,
        channel_handle: ChannelHandle,
        project_root:   PathBuf,
        full_config:    OpenAgentConfig,
        manifests:      Vec<ServiceManifest>,
    ) -> Self {
        Self {
            manager,
            metrics,
            config,
            guard_db,
            started_at: Arc::new(Instant::now()),
            agent_ctx,
            channel_handle,
            project_root: Arc::new(project_root),
            full_config: Arc::new(full_config),
            manifests: Arc::new(manifests),
        }
    }
}
