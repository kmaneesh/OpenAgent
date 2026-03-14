//! Diary write — fire-and-forget session summary written after each final answer.
//!
//! Called from `CortexAgent::execute()` via `tokio::spawn` so it never blocks
//! the step response.  Failures are logged as warnings and silently swallowed —
//! the caller has already returned its answer.
//!
//! # What gets written
//!
//! 1. A markdown file at `<diary_dir>/<unix_timestamp>.md` rendered from
//!    `prompts/diary_entry.j2` via `crate::prompt::render_diary_entry`.
//! 2. A stub row in the memory service's `diary` LanceDB table (zero vector +
//!    enriched metadata).  The memory compaction job will back-fill real
//!    embeddings, summaries, and keywords later.

use crate::prompt::{render_diary_entry, DiaryEntryContext};
use crate::tool_router::ToolRouter;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// Write a diary entry for a completed ReAct turn.
///
/// Creates `<diary_dir>/<ts>.md` from the `diary_entry.j2` template, then calls
/// `memory.diary_write` over the `ToolRouter` to insert a stub LanceDB row.
/// Both steps are best-effort — if either fails the error is logged and the
/// function returns without propagating.
pub async fn write_diary_entry(
    session_id: String,
    diary_dir: PathBuf,
    user_input: String,
    response_text: String,
    tool_calls_made: Vec<String>,
    router: Arc<ToolRouter>,
) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // ── 1. Render markdown via MiniJinja template ──────────────────────────────
    let ctx = DiaryEntryContext {
        session_id:    &session_id,
        timestamp:     ts,
        user_input:    &user_input,
        response_text: &response_text,
        tool_calls:    &tool_calls_made,
    };
    let md = match render_diary_entry(&ctx) {
        Ok(s) => s,
        Err(e) => {
            warn!(session_id = %session_id, error = %e, "diary: template render failed");
            return;
        }
    };

    // ── 2. Write markdown to disk ──────────────────────────────────────────────
    if let Err(e) = tokio::fs::create_dir_all(&diary_dir).await {
        warn!(session_id = %session_id, error = %e, "diary: failed to create directory");
        return;
    }

    let file_path = diary_dir.join(format!("{ts}.md"));
    if let Err(e) = tokio::fs::write(&file_path, &md).await {
        warn!(session_id = %session_id, error = %e, "diary: failed to write markdown");
        return;
    }

    info!(
        session_id = %session_id,
        file = %file_path.display(),
        "diary: markdown written"
    );

    // ── 3. Stub LanceDB row (zero vector) via memory service ───────────────────
    // `summary` is a 200-char truncation of the response — compact enough for
    // the diary index row without duplicating the full text already on disk.
    let summary: String = response_text.chars().take(200).collect();
    let params = json!({
        "session_id":       session_id,
        "content":          summary,
        "file_path":        file_path.display().to_string(),
        "keywords":         [],
        "validator_status": "pending",
        "flags":            {},
    });

    match router.call("memory.diary_write", &params).await {
        Ok(_) => info!(session_id = %session_id, "diary: memory row written"),
        Err(e) => warn!(
            session_id = %session_id,
            error = %e,
            "diary: memory write skipped (memory service may be down)"
        ),
    }
}
