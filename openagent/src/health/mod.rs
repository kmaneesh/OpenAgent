/// In-process component health registry.
///
/// A lightweight, globally-accessible store that tracks the live status of
/// named internal components (channels, cron scheduler, MCP-lite services, …).
/// Any module can call `mark_component_ok` / `mark_component_error` without
/// taking a dependency on anything heavier than this crate.
///
/// The registry is read by:
/// - `GET /health` — embeds a full snapshot in the liveness response.
/// - `GET /api/diagnose` — doctor module checks component staleness.
use chrono::Utc;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub status: String,
    pub updated_at: String,
    pub last_ok: Option<String>,
    pub last_error: Option<String>,
    pub restart_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthSnapshot {
    pub pid: u32,
    pub uptime_seconds: u64,
    pub updated_at: String,
    pub components: BTreeMap<String, ComponentHealth>,
}

// ---------------------------------------------------------------------------
// Global registry (initialised once, zero-allocation hot path)
// ---------------------------------------------------------------------------

struct HealthRegistry {
    started_at: Instant,
    components: Mutex<BTreeMap<String, ComponentHealth>>,
}

static REGISTRY: OnceLock<HealthRegistry> = OnceLock::new();

fn registry() -> &'static HealthRegistry {
    REGISTRY.get_or_init(|| HealthRegistry {
        started_at: Instant::now(),
        components: Mutex::new(BTreeMap::new()),
    })
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn upsert_component<F>(component: &str, update: F)
where
    F: FnOnce(&mut ComponentHealth),
{
    let mut map = registry().components.lock();
    let now = now_rfc3339();
    let entry = map.entry(component.to_string()).or_insert_with(|| ComponentHealth {
        status: "starting".into(),
        updated_at: now.clone(),
        last_ok: None,
        last_error: None,
        restart_count: 0,
    });
    update(entry);
    entry.updated_at = now;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn mark_component_ok(component: &str) {
    upsert_component(component, |entry| {
        entry.status = "ok".into();
        entry.last_ok = Some(now_rfc3339());
        entry.last_error = None;
    });
}

#[allow(clippy::needless_pass_by_value)]
pub fn mark_component_error(component: &str, error: impl ToString) {
    let err = error.to_string();
    upsert_component(component, move |entry| {
        entry.status = "error".into();
        entry.last_error = Some(err);
    });
}

pub fn bump_component_restart(component: &str) {
    upsert_component(component, |entry| {
        entry.restart_count = entry.restart_count.saturating_add(1);
    });
}

pub fn snapshot() -> HealthSnapshot {
    let components = registry().components.lock().clone();
    HealthSnapshot {
        pid: std::process::id(),
        uptime_seconds: registry().started_at.elapsed().as_secs(),
        updated_at: now_rfc3339(),
        components,
    }
}

pub fn snapshot_json() -> serde_json::Value {
    serde_json::to_value(snapshot()).unwrap_or_else(|_| {
        serde_json::json!({
            "status": "error",
            "message": "failed to serialize health snapshot"
        })
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn unique(prefix: &str) -> String {
        format!("{prefix}-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn mark_ok_initializes_component() {
        let name = unique("health-ok");
        mark_component_ok(&name);
        let snap = snapshot();
        let entry = snap.components.get(&name).expect("component present");
        assert_eq!(entry.status, "ok");
        assert!(entry.last_ok.is_some());
        assert!(entry.last_error.is_none());
    }

    #[test]
    fn mark_error_then_ok_clears_last_error() {
        let name = unique("health-err");
        mark_component_error(&name, "boom");
        let snap = snapshot();
        let e = snap.components.get(&name).unwrap();
        assert_eq!(e.status, "error");
        assert_eq!(e.last_error.as_deref(), Some("boom"));

        mark_component_ok(&name);
        let snap2 = snapshot();
        let e2 = snap2.components.get(&name).unwrap();
        assert_eq!(e2.status, "ok");
        assert!(e2.last_error.is_none());
    }

    #[test]
    fn bump_restart_increments_counter() {
        let name = unique("health-restart");
        bump_component_restart(&name);
        bump_component_restart(&name);
        let snap = snapshot();
        assert_eq!(snap.components.get(&name).unwrap().restart_count, 2);
    }

    #[test]
    fn snapshot_json_includes_uptime_and_component() {
        let name = unique("health-json");
        mark_component_ok(&name);
        let json = snapshot_json();
        assert!(json["uptime_seconds"].as_u64().is_some());
        assert_eq!(json["components"][&name]["status"], "ok");
    }
}
