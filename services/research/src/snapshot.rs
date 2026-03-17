//! Markdown snapshot writer for research sessions.
//!
//! Writes `data/research/{research_id}/snapshot.md` with the current state of
//! a research including tasks grouped by status and recent tool calls.

use crate::db::{Research, ResearchStore};
use std::path::Path;

/// Format a millisecond Unix timestamp as a human-readable date string (UTC).
fn format_date_ms(ms: i64) -> String {
    if ms < 0 {
        return "unknown".to_string();
    }
    humanize_epoch((ms / 1000) as u64)
}

/// Very lightweight epoch → "YYYY-MM-DD HH:MM UTC" without chrono.
fn humanize_epoch(secs: u64) -> String {
    // Days since epoch
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;

    // Gregorian calendar computation
    let mut y: u64 = 1970;
    let mut remaining_days = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let days_in_year: u64 = if leap { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in_months: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for dim in &days_in_months {
        if remaining_days < *dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
    }
    let day = remaining_days + 1;
    format!("{y:04}-{month:02}-{day:02} {hh:02}:{mm:02} UTC")
}

/// Write a markdown snapshot of the research to disk.
///
/// The snapshot path is `{research_dir}/{research_id}/snapshot.md`.
/// Errors are logged as warnings — snapshot failures must never break the tool flow.
pub async fn write_snapshot(
    store: &ResearchStore,
    research_dir: &Path,
    research: &Research,
) -> anyhow::Result<()> {
    let tasks = store.get_tasks(&research.id)?;
    let deps = store.get_task_deps(&research.id)?;
    let recent_calls = store.get_recent_tool_calls(&research.id, 10)?;

    // Build dependency map: task_id → [dep_task_ids]
    let mut dep_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (task_id, dep_id) in &deps {
        dep_map
            .entry(task_id.clone())
            .or_default()
            .push(dep_id.clone());
    }

    let mut md = String::with_capacity(2048);

    // Header
    md.push_str(&format!("# {}\n\n", research.title));
    md.push_str(&format!("**Goal:** {}\n", research.goal));
    md.push_str(&format!(
        "**Status:** {} | Started: {}\n",
        research.status,
        format_date_ms(research.created_at)
    ));
    md.push_str(&format!("**Research ID:** {}\n\n", research.id));

    // Tasks grouped by status
    md.push_str("## Tasks\n\n");

    let done_tasks: Vec<_> = tasks.iter().filter(|t| t.status == "done").collect();
    let in_progress: Vec<_> = tasks.iter().filter(|t| t.status == "in_progress").collect();
    let pending: Vec<_> = tasks.iter().filter(|t| t.status == "pending").collect();
    let failed: Vec<_> = tasks.iter().filter(|t| t.status == "failed").collect();

    if !done_tasks.is_empty() {
        md.push_str("### Done\n");
        for t in &done_tasks {
            let short_id = &t.id[..t.id.len().min(8)];
            md.push_str(&format!("- [{}] {}\n", short_id, t.description));
            if let Some(ref result) = t.result {
                let truncated: String = result.chars().take(200).collect();
                let ellipsis = if result.len() > 200 { "…" } else { "" };
                md.push_str(&format!("  > {}{}\n", truncated, ellipsis));
            }
        }
        md.push('\n');
    }

    if !in_progress.is_empty() {
        md.push_str("### In Progress\n");
        for t in &in_progress {
            let short_id = &t.id[..t.id.len().min(8)];
            md.push_str(&format!("- [{}] {}\n", short_id, t.description));
            if let Some(ref agent) = t.assigned_agent {
                md.push_str(&format!("  *(assigned: {})*\n", agent));
            }
        }
        md.push('\n');
    }

    if !pending.is_empty() {
        md.push_str("### Pending\n");
        for t in &pending {
            let short_id = &t.id[..t.id.len().min(8)];
            md.push_str(&format!("- [{}] {}\n", short_id, t.description));
            if let Some(dep_ids) = dep_map.get(&t.id) {
                if !dep_ids.is_empty() {
                    let short_deps: Vec<String> =
                        dep_ids.iter().map(|d| d[..d.len().min(8)].to_string()).collect();
                    md.push_str(&format!("  *(depends on: {})*\n", short_deps.join(", ")));
                }
            }
        }
        md.push('\n');
    }

    if !failed.is_empty() {
        md.push_str("### Failed\n");
        for t in &failed {
            let short_id = &t.id[..t.id.len().min(8)];
            md.push_str(&format!("- [{}] {}\n", short_id, t.description));
            if let Some(ref reason) = t.result {
                let truncated: String = reason.chars().take(200).collect();
                md.push_str(&format!("  > Reason: {}\n", truncated));
            }
        }
        md.push('\n');
    }

    // Recent tool calls
    if !recent_calls.is_empty() {
        md.push_str("## Recent Tool Calls\n\n");
        for call in &recent_calls {
            let date = format_date_ms(call.called_at);
            let result_str = call
                .result
                .as_deref()
                .map(|r| {
                    let truncated: String = r.chars().take(100).collect();
                    let ellipsis = if r.len() > 100 { "…" } else { "" };
                    format!("{}{}", truncated, ellipsis)
                })
                .unwrap_or_else(|| "(no result)".to_string());
            md.push_str(&format!("- {} | {} → {}\n", date, call.tool_name, result_str));
        }
        md.push('\n');
    }

    // Write to disk
    let dir = research_dir.join(&research.id);
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join("snapshot.md");
    tokio::fs::write(&path, md.as_bytes()).await?;

    Ok(())
}
