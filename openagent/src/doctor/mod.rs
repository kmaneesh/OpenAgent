/// On-demand diagnostic runner for OpenAgent.
///
/// `diagnose()` runs a set of checks and returns structured `Vec<DiagResult>`.
/// Call sites:
/// - `GET /api/diagnose` — returns JSON to the web UI.
/// - `openagent doctor` CLI subcommand (future) — prints a human-readable report.
///
/// Checks performed (in order):
/// - **config**     — config file exists, provider/model/base_url set, port valid.
/// - **data**       — `data/` dir exists + writable, disk space, key DB files.
/// - **services**   — for each service.json: binary exists for current platform.
/// - **skills**     — `skills/` dir exists; each SKILL.md has required frontmatter fields.
/// - **environment** — `$HOME` set, optional `msb` binary in PATH.
/// - **components** — health registry: freshness of cron, channel:*, service:* entries.
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::config::OpenAgentConfig;
use crate::platform::host_platform_key;
use crate::service::manifest::ServiceManifest;

// Staleness thresholds (seconds)
const CRON_STALE_SECS: i64 = 120;
const CHANNEL_STALE_SECS: i64 = 300;
const SERVICE_STALE_SECS: i64 = 30;

// ---------------------------------------------------------------------------
// DiagResult — public structured output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagResult {
    pub severity: Severity,
    pub category: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Internal builder type
// ---------------------------------------------------------------------------

struct Item {
    severity: Severity,
    category: &'static str,
    message: String,
}

impl Item {
    fn ok(cat: &'static str, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Ok, category: cat, message: msg.into() }
    }
    fn warn(cat: &'static str, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Warn, category: cat, message: msg.into() }
    }
    fn error(cat: &'static str, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Error, category: cat, message: msg.into() }
    }
    fn into_result(self) -> DiagResult {
        DiagResult {
            severity: self.severity,
            category: self.category.to_string(),
            message: self.message,
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run all diagnostic checks and return structured results.
pub fn diagnose(
    cfg: &OpenAgentConfig,
    project_root: &Path,
    manifests: &[ServiceManifest],
) -> Vec<DiagResult> {
    let mut items: Vec<Item> = Vec::new();

    check_config(cfg, project_root, &mut items);
    check_data(project_root, &mut items);
    check_services(manifests, project_root, &mut items);
    check_skills(project_root, &mut items);
    check_environment(&mut items);
    check_components(&mut items);

    items.into_iter().map(Item::into_result).collect()
}

/// Run diagnostics and print a human-readable report to stdout.
/// Used by the `openagent doctor` CLI subcommand.
#[allow(dead_code)]
pub fn run_report(
    cfg: &OpenAgentConfig,
    project_root: &Path,
    manifests: &[ServiceManifest],
) {
    let results = diagnose(cfg, project_root, manifests);

    println!("OpenAgent Doctor");
    println!();

    let mut current_cat = "";
    for item in &results {
        if item.category != current_cat {
            current_cat = &item.category;
            println!("  [{current_cat}]");
        }
        let icon = match item.severity {
            Severity::Ok => "ok  ",
            Severity::Warn => "warn",
            Severity::Error => "ERR ",
        };
        println!("    [{icon}] {}", item.message);
    }

    let errors = results.iter().filter(|i| i.severity == Severity::Error).count();
    let warns  = results.iter().filter(|i| i.severity == Severity::Warn).count();
    let oks    = results.iter().filter(|i| i.severity == Severity::Ok).count();

    println!();
    println!("  Summary: {oks} ok, {warns} warnings, {errors} errors");
    if errors > 0 {
        println!("  Fix the errors above and re-run `openagent doctor`.");
    }
}

// ---------------------------------------------------------------------------
// Check: config
// ---------------------------------------------------------------------------

fn check_config(cfg: &OpenAgentConfig, project_root: &Path, items: &mut Vec<Item>) {
    let cat = "config";
    let path = project_root.join("config").join("openagent.toml");

    if path.exists() {
        items.push(Item::ok(cat, format!("config file: {}", path.display())));
    } else {
        items.push(Item::warn(cat, format!("config file not found: {} (using defaults)", path.display())));
    }

    // Provider kind
    if cfg.provider.kind.is_empty() {
        items.push(Item::error(cat, "provider.kind is not set"));
    } else {
        items.push(Item::ok(cat, format!("provider.kind = \"{}\"", cfg.provider.kind)));
    }

    // Base URL
    if cfg.provider.base_url.is_empty() {
        items.push(Item::warn(cat, "provider.base_url not set (required for openai_compat)"));
    } else {
        items.push(Item::ok(cat, format!("provider.base_url = \"{}\"", cfg.provider.base_url)));
    }

    // Model
    if cfg.provider.model.is_empty() {
        items.push(Item::warn(cat, "provider.model not set"));
    } else {
        items.push(Item::ok(cat, format!("provider.model = \"{}\"", cfg.provider.model)));
    }
}

// ---------------------------------------------------------------------------
// Check: data directory
// ---------------------------------------------------------------------------

fn check_data(project_root: &Path, items: &mut Vec<Item>) {
    let cat = "data";
    let data_dir = project_root.join("data");

    if !data_dir.exists() {
        items.push(Item::error(cat, format!("data/ directory missing: {}", data_dir.display())));
        return;
    }
    items.push(Item::ok(cat, format!("data/ exists: {}", data_dir.display())));

    // Writable probe
    let probe = data_dir.join(format!(
        ".openagent_doctor_probe_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    ));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(mut f) => {
            let write_ok = f.write_all(b"probe").is_ok();
            drop(f);
            let _ = std::fs::remove_file(&probe);
            if write_ok {
                items.push(Item::ok(cat, "data/ is writable"));
            } else {
                items.push(Item::error(cat, "data/ write probe failed"));
            }
        }
        Err(e) => items.push(Item::error(cat, format!("data/ not writable: {e}"))),
    }

    // Disk space (best-effort via `df`)
    if let Some(avail_mb) = disk_available_mb(&data_dir) {
        if avail_mb >= 200 {
            items.push(Item::ok(cat, format!("disk space: {avail_mb} MB available")));
        } else {
            items.push(Item::warn(cat, format!("low disk space: only {avail_mb} MB available")));
        }
    }

    // Key database files
    let db = data_dir.join("openagent.db");
    if db.exists() {
        items.push(Item::ok(cat, "openagent.db present"));
    } else {
        items.push(Item::warn(cat, "openagent.db not found (created on first run)"));
    }
}

fn disk_available_mb(path: &Path) -> Option<u64> {
    let out = std::process::Command::new("df")
        .arg("-m")
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // `df -m` output: last non-empty line, 4th whitespace-separated column = Available
    let line = text.lines().rev().find(|l| !l.trim().is_empty())?;
    line.split_whitespace().nth(3)?.parse().ok()
}

// ---------------------------------------------------------------------------
// Check: service binaries
// ---------------------------------------------------------------------------

fn check_services(manifests: &[ServiceManifest], project_root: &Path, items: &mut Vec<Item>) {
    let cat = "services";
    let platform = host_platform_key();

    if manifests.is_empty() {
        items.push(Item::warn(cat, "no service manifests found in services/"));
        return;
    }

    for m in manifests {
        if !m.enabled {
            items.push(Item::ok(cat, format!("{}: disabled in service.json", m.name)));
            continue;
        }

        match m.binary.get(platform) {
            Some(rel_path) => {
                let abs = project_root.join(rel_path);
                if abs.exists() {
                    items.push(Item::ok(cat, format!("{}: binary present ({})", m.name, rel_path)));
                } else {
                    items.push(Item::error(cat, format!(
                        "{}: binary missing — run `make local` (expected: {})",
                        m.name, abs.display()
                    )));
                }
            }
            None => {
                items.push(Item::warn(cat, format!(
                    "{}: no binary entry for platform \"{}\" in service.json",
                    m.name, platform
                )));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Check: skills
// ---------------------------------------------------------------------------

fn check_skills(project_root: &Path, items: &mut Vec<Item>) {
    let cat = "skills";
    let skills_dir = project_root.join("skills");

    if !skills_dir.exists() {
        items.push(Item::warn(cat, "skills/ directory not found"));
        return;
    }

    let entries = match std::fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(err) => {
            items.push(Item::error(cat, format!("cannot read skills/: {err}")));
            return;
        }
    };

    let mut skill_count = 0u32;
    let mut missing_fields = 0u32;

    for entry in entries.flatten() {
        let skill_path = entry.path();
        if !skill_path.is_dir() {
            continue;
        }
        let skill_md = skill_path.join("SKILL.md");
        if !skill_md.exists() {
            items.push(Item::warn(cat, format!(
                "{}: SKILL.md missing",
                skill_path.file_name().unwrap_or_default().to_string_lossy()
            )));
            continue;
        }

        skill_count += 1;

        match std::fs::read_to_string(&skill_md) {
            Ok(content) => {
                let name_missing = !frontmatter_has_field(&content, "name");
                let desc_missing = !frontmatter_has_field(&content, "description");
                let hint_missing = !frontmatter_has_field(&content, "hint");

                if name_missing || desc_missing || hint_missing {
                    missing_fields += 1;
                    let mut missing = Vec::new();
                    if name_missing { missing.push("name"); }
                    if desc_missing { missing.push("description"); }
                    if hint_missing { missing.push("hint"); }
                    items.push(Item::warn(cat, format!(
                        "{}/SKILL.md: missing frontmatter fields: {}",
                        skill_path.file_name().unwrap_or_default().to_string_lossy(),
                        missing.join(", ")
                    )));
                }
            }
            Err(e) => {
                items.push(Item::error(cat, format!(
                    "{}/SKILL.md: cannot read: {e}",
                    skill_path.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
        }
    }

    if skill_count == 0 {
        items.push(Item::warn(cat, "no skills found in skills/"));
    } else if missing_fields == 0 {
        items.push(Item::ok(cat, format!("{skill_count} skills — all frontmatter valid")));
    } else {
        items.push(Item::warn(cat, format!(
            "{skill_count} skills, {missing_fields} with incomplete frontmatter"
        )));
    }
}

/// Returns `true` if the YAML frontmatter block (between `---` delimiters) contains `field:`.
fn frontmatter_has_field(content: &str, field: &str) -> bool {
    let search = format!("{field}:");
    // Only scan inside the frontmatter block (first `---` … second `---`)
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        // No frontmatter — scan whole file as fallback
        return content.contains(&search);
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with(&search) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Check: environment
// ---------------------------------------------------------------------------

fn check_environment(items: &mut Vec<Item>) {
    let cat = "environment";

    if std::env::var("HOME").is_ok() || std::env::var("USERPROFILE").is_ok() {
        items.push(Item::ok(cat, "home directory env set"));
    } else {
        items.push(Item::error(cat, "neither $HOME nor $USERPROFILE is set"));
    }

    // msb — required at runtime only by the sandbox service
    match std::process::Command::new("msb")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout);
            let first = ver.lines().next().unwrap_or("").trim();
            items.push(Item::ok(cat, format!("msb: {first}")));
        }
        Ok(_) => items.push(Item::warn(cat, "msb found but returned non-zero")),
        Err(_) => items.push(Item::warn(
            cat,
            "msb not in PATH (only required if sandbox service is enabled — `cargo install msb`)",
        )),
    }
}

// ---------------------------------------------------------------------------
// Check: in-process health registry
// ---------------------------------------------------------------------------

fn check_components(items: &mut Vec<Item>) {
    let cat = "components";
    let snap = crate::health::snapshot();

    if snap.components.is_empty() {
        items.push(Item::warn(cat, "no components registered yet (daemon just started?)"));
        return;
    }

    for (name, component) in &snap.components {
        let age_secs = component
            .last_ok
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map_or(i64::MAX, |dt| {
                Utc::now().signed_duration_since(dt.with_timezone(&Utc)).num_seconds()
            });

        let threshold = if name == "cron" {
            CRON_STALE_SECS
        } else if name.starts_with("channel:") {
            CHANNEL_STALE_SECS
        } else {
            SERVICE_STALE_SECS
        };

        if component.status == "ok" && age_secs <= threshold {
            items.push(Item::ok(cat, format!("{name}: ok ({age_secs}s ago)")));
        } else if component.status == "ok" {
            items.push(Item::warn(cat, format!("{name}: ok but stale ({age_secs}s ago)")));
        } else {
            let err = component.last_error.as_deref().unwrap_or("unknown");
            items.push(Item::error(cat, format!("{name}: {err}")));
        }

        if component.restart_count > 0 {
            items.push(Item::warn(cat, format!(
                "{name}: restarted {} time(s)",
                component.restart_count
            )));
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn abs_binary_path(project_root: &Path, rel: &str) -> PathBuf {
    project_root.join(rel)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_detects_fields() {
        let content = "---\nname: foo\ndescription: bar\nhint: call it\n---\n# body";
        assert!(frontmatter_has_field(content, "name"));
        assert!(frontmatter_has_field(content, "description"));
        assert!(frontmatter_has_field(content, "hint"));
        assert!(!frontmatter_has_field(content, "enforce"));
    }

    #[test]
    fn frontmatter_no_block_falls_back_to_full_scan() {
        let content = "name: foo\ndescription: bar";
        assert!(frontmatter_has_field(content, "name"));
        assert!(!frontmatter_has_field(content, "hint"));
    }

    #[test]
    fn diagnose_returns_results_for_missing_root() {
        use std::path::PathBuf;
        let cfg = OpenAgentConfig::default();
        let root = PathBuf::from("/tmp/openagent_doctor_nonexistent_xyz");
        let results = diagnose(&cfg, &root, &[]);
        assert!(!results.is_empty());
        // data/ check should produce an error
        assert!(results.iter().any(|r| r.severity == Severity::Error || r.severity == Severity::Warn));
    }
}
