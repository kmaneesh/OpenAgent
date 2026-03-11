use crate::repair::{LlmJsonBackend, RepairEngine, RepairMode};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct RepairParams {
    pub text: String,
    #[serde(default)]
    pub mode: RepairMode,
}

pub fn handle_repair_json(params: Value) -> Result<String> {
    let args: RepairParams = serde_json::from_value(params)
        .context("invalid params for validator.repair_json")?;

    if args.text.trim().is_empty() {
        bail!("text must not be empty");
    }

    let engine = RepairEngine::new(LlmJsonBackend);
    match engine.repair_json(&args.text, args.mode) {
        Ok(outcome) => Ok(json!({
            "ok": true,
            "json": outcome.canonical_json,
            "was_repaired": outcome.was_repaired,
            "changed": outcome.changed
        }).to_string()),
        Err(err) => Ok(json!({
            "ok": false,
            "error": "unable_to_repair",
            "message": err.to_string(),
            "was_repaired": false,
            "changed": false
        }).to_string()),
    }
}
