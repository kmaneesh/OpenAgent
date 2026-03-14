//! Prompt rendering via MiniJinja.
//!
//! Templates are embedded at compile time with `include_str!` — no file I/O
//! at runtime, no missing-file failures on a Pi.  The `Environment` is built
//! once and cached in a `OnceLock`.
//!
//! # Template files
//!
//! | File                      | Purpose                                              |
//! |---------------------------|------------------------------------------------------|
//! | `prompts/step_system.j2`  | Base system prompt + JSON output format section      |
//! | `prompts/tool_context.j2` | Appends available-tools block to the system prompt   |
//! | `prompts/correction.j2`   | Injected as a user turn when model returns non-JSON  |
//! | `prompts/diary_entry.j2`  | Deterministic markdown template for diary entries    |
//!
//! # Render functions
//!
//! All functions return `anyhow::Result<String>`.  Errors are only possible if
//! the embedded template contains a syntax error, which is a programming mistake
//! caught at the first call rather than at compile time.
//!
//! `render_diary_entry` additionally accepts structured turn data so the diary
//! markdown is deterministic and machine-readable without regex.

use anyhow::{anyhow, Result};
use minijinja::{context, Environment};
use std::sync::OnceLock;

// Templates are embedded at compile time; path is relative to this source file.
const STEP_SYSTEM_SRC: &str = include_str!("../prompts/step_system.j2");
const TOOL_CONTEXT_SRC: &str = include_str!("../prompts/tool_context.j2");
const CORRECTION_SRC: &str = include_str!("../prompts/correction.j2");
const DIARY_ENTRY_SRC: &str = include_str!("../prompts/diary_entry.j2");

static ENV: OnceLock<Environment<'static>> = OnceLock::new();

fn env() -> &'static Environment<'static> {
    ENV.get_or_init(|| {
        let mut e = Environment::new();
        // All templates are embedded constants — parse errors are programming
        // mistakes that must surface immediately, not be silently swallowed.
        e.add_template("step_system", STEP_SYSTEM_SRC)
            .expect("step_system.j2 must be a valid MiniJinja template");
        e.add_template("tool_context", TOOL_CONTEXT_SRC)
            .expect("tool_context.j2 must be a valid MiniJinja template");
        e.add_template("correction", CORRECTION_SRC)
            .expect("correction.j2 must be a valid MiniJinja template");
        e.add_template("diary_entry", DIARY_ENTRY_SRC)
            .expect("diary_entry.j2 must be a valid MiniJinja template");
        e
    })
}

/// Render the structured system prompt.
///
/// Appends the JSON output-format section to `system_prompt`.
/// Called once per step in `handle_step` before agent construction.
pub fn render_step_system(system_prompt: &str) -> Result<String> {
    env()
        .get_template("step_system")
        .and_then(|t| t.render(context! { system_prompt => system_prompt.trim() }))
        .map_err(|e| anyhow!("step_system render failed: {e}"))
}

/// Render the system prompt with an available-tools block appended.
///
/// Returns the bare `system_prompt` unchanged when `action_context` is empty
/// (tool_call turns do not inject the tool list).
pub fn render_tool_context(system_prompt: &str, action_context: &str) -> Result<String> {
    let action_context = action_context.trim();
    if action_context.is_empty() {
        return Ok(system_prompt.to_string());
    }
    env()
        .get_template("tool_context")
        .and_then(|t| t.render(context! {
            system_prompt    => system_prompt,
            action_context   => action_context,
        }))
        .map_err(|e| anyhow!("tool_context render failed: {e}"))
}

/// Render the correction message injected when the model returns non-JSON.
///
/// The template carries no variables today; the function signature accepts
/// none so callers stay decoupled from that detail if variables are added later.
pub fn render_correction() -> Result<String> {
    env()
        .get_template("correction")
        .and_then(|t| t.render(context! {}))
        .map_err(|e| anyhow!("correction render failed: {e}"))
}

/// Input for `render_diary_entry`.
///
/// All string fields are rendered verbatim — callers must pass already-trimmed
/// content.  `tool_calls` may be empty (the template renders "_none_").
pub struct DiaryEntryContext<'a> {
    pub session_id:    &'a str,
    pub timestamp:     u64,
    pub user_input:    &'a str,
    pub response_text: &'a str,
    pub tool_calls:    &'a [String],
}

/// Render a deterministic markdown diary entry.
///
/// The resulting string is written to `data/diary/{session_id}/{ts}.md` by
/// `diary::write_diary_entry`.  Using a template instead of a format string
/// guarantees a stable, machine-readable structure for offline compaction.
pub fn render_diary_entry(ctx: &DiaryEntryContext<'_>) -> Result<String> {
    env()
        .get_template("diary_entry")
        .and_then(|t| {
            t.render(context! {
                session_id    => ctx.session_id,
                timestamp     => ctx.timestamp,
                user_input    => ctx.user_input,
                response_text => ctx.response_text,
                tool_calls    => ctx.tool_calls,
            })
        })
        .map_err(|e| anyhow!("diary_entry render failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_system_contains_output_format_section() {
        let out = render_step_system("You are a helpful assistant.").unwrap();
        assert!(out.starts_with("You are a helpful assistant."));
        assert!(out.contains("## Output format"));
        assert!(out.contains("\"type\":\"final\""));
        assert!(out.contains("\"type\":\"tool_call\""));
    }

    #[test]
    fn step_system_trims_system_prompt() {
        let out = render_step_system("  trimmed  ").unwrap();
        assert!(out.starts_with("trimmed"));
    }

    #[test]
    fn tool_context_appends_tools_block() {
        let base = "Base system prompt.";
        let tools = "- browser.open [browser]";
        let out = render_tool_context(base, tools).unwrap();
        assert!(out.starts_with(base));
        assert!(out.contains("## Available tools"));
        assert!(out.contains("browser.open"));
        assert!(out.contains("\"type\":\"tool_call\""));
    }

    #[test]
    fn tool_context_returns_bare_prompt_when_no_tools() {
        let base = "Base system prompt.";
        let out = render_tool_context(base, "").unwrap();
        assert_eq!(out, base);
    }

    #[test]
    fn tool_context_returns_bare_prompt_for_whitespace_only_tools() {
        let base = "Base system prompt.";
        let out = render_tool_context(base, "   ").unwrap();
        assert_eq!(out, base);
    }

    #[test]
    fn correction_contains_json_shape_examples() {
        let out = render_correction().unwrap();
        assert!(out.contains("not valid JSON"));
        assert!(out.contains("\"type\":\"final\""));
        assert!(out.contains("\"type\":\"tool_call\""));
    }

    #[test]
    fn env_is_initialised_once() {
        // Two calls must return the same pointer — OnceLock guarantee.
        let a = env() as *const _;
        let b = env() as *const _;
        assert_eq!(a, b);
    }

    #[test]
    fn diary_entry_contains_required_sections() {
        let ctx = DiaryEntryContext {
            session_id:    "sess-abc",
            timestamp:     1_700_000_000,
            user_input:    "What is the capital of France?",
            response_text: "Paris.",
            tool_calls:    &[],
        };
        let out = render_diary_entry(&ctx).unwrap();
        assert!(out.contains("# Session: sess-abc"));
        assert!(out.contains("1700000000"));
        assert!(out.contains("What is the capital of France?"));
        assert!(out.contains("Paris."));
        assert!(out.contains("_none_"));
    }

    #[test]
    fn diary_entry_lists_tool_calls() {
        let tools = vec!["browser.open".to_string(), "sandbox.execute".to_string()];
        let ctx = DiaryEntryContext {
            session_id:    "sess-xyz",
            timestamp:     1_700_000_001,
            user_input:    "Run a script",
            response_text: "Done.",
            tool_calls:    &tools,
        };
        let out = render_diary_entry(&ctx).unwrap();
        assert!(out.contains("- browser.open"));
        assert!(out.contains("- sandbox.execute"));
        assert!(!out.contains("_none_"));
    }

    #[test]
    fn diary_entry_trims_whitespace_in_fields() {
        let ctx = DiaryEntryContext {
            session_id:    "sess-trim",
            timestamp:     0,
            user_input:    "  hello  ",
            response_text: "  world  ",
            tool_calls:    &[],
        };
        let out = render_diary_entry(&ctx).unwrap();
        // MiniJinja `| trim` strips leading/trailing whitespace from user_input and response_text.
        assert!(out.contains("\nhello\n"));
        assert!(out.contains("\nworld\n"));
    }
}
