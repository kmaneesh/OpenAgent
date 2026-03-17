//! Tool handler implementations for the research service.
//!
//! Each handler wires all four OTEL pillars via ResearchTelemetry:
//!   Traces  — tracing::info_span! with per-operation attributes
//!   Metrics — ResearchTelemetry::record() on success and error
//!   Logs    — structured tracing::{info!, warn!, error!} events
//!   Baggage — attach_context() propagates remote parent + tool tags
//!
//! Handlers are sync (as required by McpLiteServer::register_tool).
//! Async work (snapshot writes) is fire-and-forget via tokio::spawn.

use crate::db::ResearchStore;
use crate::metrics::{ts_ms, ResearchTelemetry};
use crate::snapshot::write_snapshot;
use opentelemetry::KeyValue;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

// ── Shared helper ──────────────────────────────────────────────────────────────

fn err_result(msg: &str) -> anyhow::Result<String> {
    Ok(json!({"error": msg}).to_string())
}

/// Fire a snapshot write in the background (best-effort, never blocks the handler).
fn spawn_snapshot(
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    research: crate::db::Research,
) {
    tokio::spawn(async move {
        if let Err(e) = write_snapshot(&store, &research_dir, &research).await {
            warn!(error = %e, research_id = %research.id, "snapshot write failed");
        }
    });
}

// ── research.start ─────────────────────────────────────────────────────────────

pub fn handle_research_start(
    params: Value,
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.start")],
    );
    let span = tracing::info_span!("research.start");
    let _enter = span.enter();

    let user_key = match params.get("user_key").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("user_key is required"),
    };
    let goal = match params.get("goal").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("goal is required"),
    };
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| goal.chars().take(60).collect());

    let t_start = Instant::now();
    info!(user_key = %user_key, "research.start");

    let research = match store.create_research(&user_key, &title, &goal) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "create_research failed");
            tel.record(&json!({
                "ts": ts_ms(), "op": "research.start", "status": "error", "error": e.to_string()
            }));
            return err_result(&e.to_string());
        }
    };

    // Add 3 skeleton tasks decomposed from the goal (each depends on the prior)
    let skeleton = [
        format!("Research: {}", goal),
        "Analyse findings".to_string(),
        "Draft summary".to_string(),
    ];
    let mut last_id: Option<String> = None;
    for desc in &skeleton {
        let deps: Vec<String> = last_id.iter().cloned().collect();
        match store.add_task(&research.id, desc, None, None, &deps) {
            Ok(task) => {
                last_id = Some(task.id);
            }
            Err(e) => {
                warn!(error = %e, "skeleton task creation failed (non-fatal)");
            }
        }
    }

    let elapsed = t_start.elapsed().as_millis() as u64;
    info!(elapsed_ms = elapsed, research_id = %research.id, "research.start ok");
    tel.record(&json!({
        "ts": ts_ms(), "op": "research.start", "status": "ok",
        "research_id": research.id, "elapsed_ms": elapsed
    }));

    spawn_snapshot(Arc::clone(&store), Arc::clone(&research_dir), research.clone());

    Ok(serde_json::to_string(&research)
        .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()))
}

// ── research.list ──────────────────────────────────────────────────────────────

pub fn handle_research_list(
    params: Value,
    store: Arc<ResearchStore>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.list")],
    );
    let span = tracing::info_span!("research.list");
    let _enter = span.enter();

    let user_key = match params.get("user_key").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("user_key is required"),
    };

    let t_start = Instant::now();
    match store.list_researches(&user_key) {
        Ok(list) => {
            let elapsed = t_start.elapsed().as_millis() as u64;
            tel.record(&json!({
                "ts": ts_ms(), "op": "research.list", "status": "ok",
                "count": list.len(), "elapsed_ms": elapsed
            }));
            Ok(serde_json::to_string(&list)
                .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()))
        }
        Err(e) => {
            error!(error = %e, "list_researches failed");
            tel.record(&json!({
                "ts": ts_ms(), "op": "research.list", "status": "error", "error": e.to_string()
            }));
            err_result(&e.to_string())
        }
    }
}

// ── research.switch ────────────────────────────────────────────────────────────

pub fn handle_research_switch(
    params: Value,
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.switch")],
    );
    let span = tracing::info_span!("research.switch");
    let _enter = span.enter();

    let user_key = match params.get("user_key").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("user_key is required"),
    };
    let research_id = match params.get("research_id").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("research_id is required"),
    };

    let t_start = Instant::now();
    if let Err(e) = store.set_current(&user_key, &research_id) {
        error!(error = %e, "set_current failed");
        tel.record(&json!({
            "ts": ts_ms(), "op": "research.switch", "status": "error", "error": e.to_string()
        }));
        return err_result(&e.to_string());
    }

    match store.get_current(&user_key) {
        Ok(Some(research)) => {
            let elapsed = t_start.elapsed().as_millis() as u64;
            tel.record(&json!({
                "ts": ts_ms(), "op": "research.switch", "status": "ok",
                "research_id": research.id, "elapsed_ms": elapsed
            }));
            spawn_snapshot(Arc::clone(&store), Arc::clone(&research_dir), research.clone());
            Ok(serde_json::to_string(&research)
                .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()))
        }
        Ok(None) => err_result("research not found after switch"),
        Err(e) => {
            error!(error = %e, "get_current after switch failed");
            err_result(&e.to_string())
        }
    }
}

// ── research.status ────────────────────────────────────────────────────────────

pub fn handle_research_status(
    params: Value,
    store: Arc<ResearchStore>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.status")],
    );
    let span = tracing::info_span!("research.status");
    let _enter = span.enter();

    let user_key = match params.get("user_key").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("user_key is required"),
    };

    let t_start = Instant::now();

    let research = match store.get_current(&user_key) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Ok(
                json!({"research": null, "tasks": [], "runnable_tasks": []}).to_string()
            )
        }
        Err(e) => {
            error!(error = %e, "get_current failed");
            tel.record(&json!({
                "ts": ts_ms(), "op": "research.status", "status": "error", "error": e.to_string()
            }));
            return err_result(&e.to_string());
        }
    };

    let tasks = match store.get_tasks(&research.id) {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "get_tasks failed");
            return err_result(&e.to_string());
        }
    };

    let deps = match store.get_task_deps(&research.id) {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, "get_task_deps failed");
            return err_result(&e.to_string());
        }
    };

    // Build done set for dependency resolution
    let done_ids: std::collections::HashSet<String> = tasks
        .iter()
        .filter(|t| t.status == "done")
        .map(|t| t.id.clone())
        .collect();

    // Build dep map: task_id → list of dep_ids
    let mut dep_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (task_id, dep_id) in &deps {
        dep_map.entry(task_id.clone()).or_default().push(dep_id.clone());
    }

    // Runnable = pending tasks where all deps are done
    let runnable_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| {
            if t.status != "pending" {
                return false;
            }
            match dep_map.get(&t.id) {
                None => true,
                Some(dep_ids) => dep_ids.iter().all(|d| done_ids.contains(d)),
            }
        })
        .collect();

    let elapsed = t_start.elapsed().as_millis() as u64;
    tel.record(&json!({
        "ts": ts_ms(), "op": "research.status", "status": "ok",
        "tasks": tasks.len(), "runnable": runnable_tasks.len(), "elapsed_ms": elapsed
    }));

    Ok(json!({
        "research": research,
        "tasks": tasks,
        "runnable_tasks": runnable_tasks,
    })
    .to_string())
}

// ── research.complete ──────────────────────────────────────────────────────────

pub fn handle_research_complete(
    params: Value,
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.complete")],
    );
    let span = tracing::info_span!("research.complete");
    let _enter = span.enter();

    let user_key = match params.get("user_key").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("user_key is required"),
    };

    let research = match store.get_current(&user_key) {
        Ok(Some(r)) => r,
        Ok(None) => return err_result("no current research for user"),
        Err(e) => return err_result(&e.to_string()),
    };

    let t_start = Instant::now();
    if let Err(e) = store.update_status(&research.id, "complete") {
        error!(error = %e, "update_status failed");
        tel.record(&json!({
            "ts": ts_ms(), "op": "research.complete", "status": "error", "error": e.to_string()
        }));
        return err_result(&e.to_string());
    }

    let mut completed = research.clone();
    completed.status = "complete".to_string();

    let elapsed = t_start.elapsed().as_millis() as u64;
    info!(elapsed_ms = elapsed, research_id = %research.id, "research.complete ok");
    tel.record(&json!({
        "ts": ts_ms(), "op": "research.complete", "status": "ok",
        "research_id": research.id, "elapsed_ms": elapsed
    }));

    spawn_snapshot(Arc::clone(&store), Arc::clone(&research_dir), completed.clone());

    Ok(serde_json::to_string(&completed)
        .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()))
}

// ── research.task_add ──────────────────────────────────────────────────────────

pub fn handle_task_add(
    params: Value,
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.task_add")],
    );
    let span = tracing::info_span!("research.task_add");
    let _enter = span.enter();

    let user_key = match params.get("user_key").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("user_key is required"),
    };
    let description = match params.get("description").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("description is required"),
    };

    let parent_id = params
        .get("parent_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let assigned_agent = params
        .get("assigned_agent")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());

    let depends_on: Vec<String> = params
        .get("depends_on")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let research = match store.get_current(&user_key) {
        Ok(Some(r)) => r,
        Ok(None) => return err_result("no current research for user"),
        Err(e) => return err_result(&e.to_string()),
    };

    let t_start = Instant::now();
    let task = match store.add_task(
        &research.id,
        &description,
        parent_id,
        assigned_agent,
        &depends_on,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "add_task failed");
            tel.record(&json!({
                "ts": ts_ms(), "op": "research.task_add", "status": "error", "error": e.to_string()
            }));
            return err_result(&e.to_string());
        }
    };

    let elapsed = t_start.elapsed().as_millis() as u64;
    info!(elapsed_ms = elapsed, task_id = %task.id, "research.task_add ok");
    tel.record(&json!({
        "ts": ts_ms(), "op": "research.task_add", "status": "ok",
        "task_id": task.id, "elapsed_ms": elapsed
    }));

    spawn_snapshot(Arc::clone(&store), Arc::clone(&research_dir), research);

    Ok(serde_json::to_string(&task)
        .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()))
}

// ── research.task_done ─────────────────────────────────────────────────────────

pub fn handle_task_done(
    params: Value,
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.task_done")],
    );
    let span = tracing::info_span!("research.task_done");
    let _enter = span.enter();

    let task_id = match params.get("task_id").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("task_id is required"),
    };
    let result_str = match params.get("result").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => return err_result("result is required"),
    };

    let t_start = Instant::now();
    if let Err(e) = store.update_task(&task_id, "done", Some(&result_str)) {
        error!(error = %e, "update_task failed");
        tel.record(&json!({
            "ts": ts_ms(), "op": "research.task_done", "status": "error", "error": e.to_string()
        }));
        return err_result(&e.to_string());
    }

    let elapsed = t_start.elapsed().as_millis() as u64;
    info!(elapsed_ms = elapsed, task_id = %task_id, "research.task_done ok");
    tel.record(&json!({
        "ts": ts_ms(), "op": "research.task_done", "status": "ok",
        "task_id": task_id, "elapsed_ms": elapsed
    }));

    // Fire snapshot for the parent research (best-effort)
    if let Ok(Some(research)) = store.get_research_for_task(&task_id) {
        spawn_snapshot(Arc::clone(&store), Arc::clone(&research_dir), research);
    }

    Ok(json!({"ok": true, "task_id": task_id}).to_string())
}

// ── research.task_fail ─────────────────────────────────────────────────────────

pub fn handle_task_fail(
    params: Value,
    store: Arc<ResearchStore>,
    research_dir: Arc<PathBuf>,
    tel: Arc<ResearchTelemetry>,
) -> anyhow::Result<String> {
    let _cx_guard = ResearchTelemetry::attach_context(
        &params,
        vec![KeyValue::new("tool", "research.task_fail")],
    );
    let span = tracing::info_span!("research.task_fail");
    let _enter = span.enter();

    let task_id = match params.get("task_id").and_then(Value::as_str) {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => return err_result("task_id is required"),
    };
    let reason = match params.get("reason").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => return err_result("reason is required"),
    };

    let t_start = Instant::now();
    if let Err(e) = store.update_task(&task_id, "failed", Some(&reason)) {
        error!(error = %e, "update_task failed");
        tel.record(&json!({
            "ts": ts_ms(), "op": "research.task_fail", "status": "error", "error": e.to_string()
        }));
        return err_result(&e.to_string());
    }

    let elapsed = t_start.elapsed().as_millis() as u64;
    info!(elapsed_ms = elapsed, task_id = %task_id, "research.task_fail ok");
    tel.record(&json!({
        "ts": ts_ms(), "op": "research.task_fail", "status": "ok",
        "task_id": task_id, "elapsed_ms": elapsed
    }));

    // Fire snapshot for the parent research (best-effort)
    if let Ok(Some(research)) = store.get_research_for_task(&task_id) {
        spawn_snapshot(Arc::clone(&store), Arc::clone(&research_dir), research);
    }

    Ok(json!({"ok": true, "task_id": task_id}).to_string())
}
