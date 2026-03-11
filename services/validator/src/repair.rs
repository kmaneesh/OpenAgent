use anyhow::{anyhow, bail, Context, Result};
use llm_json::repair_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairMode {
    Auto,
    JsonObject,
    JsonArray,
}

impl Default for RepairMode {
    fn default() -> Self {
        Self::Auto
    }
}

pub trait RepairBackend: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    fn repair(&self, text: &str) -> Result<String>;
}

#[derive(Debug, Default)]
pub struct LlmJsonBackend;

impl RepairBackend for LlmJsonBackend {
    fn name(&self) -> &'static str {
        "llm_json"
    }

    fn repair(&self, text: &str) -> Result<String> {
        repair_json(text, &Default::default()).map_err(|err| anyhow!(err.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairOutcome {
    pub canonical_json: String,
    pub was_repaired: bool,
    pub changed: bool,
}

#[derive(Debug)]
pub struct RepairEngine<B: RepairBackend> {
    backend: B,
}

impl<B: RepairBackend> RepairEngine<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn repair_json(&self, text: &str, mode: RepairMode) -> Result<RepairOutcome> {
        let original = text.trim();
        if original.is_empty() {
            bail!("input text is empty");
        }

        match serde_json::from_str::<Value>(original) {
            Ok(value) => {
                validate_mode(&value, mode)?;
                let canonical = serde_json::to_string(&value)
                    .context("failed to serialise strict JSON as canonical output")?;
                Ok(RepairOutcome {
                    changed: canonical != original,
                    was_repaired: false,
                    canonical_json: canonical,
                })
            }
            Err(_) => {
                let repaired = self.backend.repair(original)
                    .with_context(|| format!("{} could not repair malformed JSON", self.backend.name()))?;
                let value = serde_json::from_str::<Value>(&repaired)
                    .context("repaired output is still not valid JSON")?;
                validate_mode(&value, mode)?;
                let canonical = serde_json::to_string(&value)
                    .context("failed to serialise repaired JSON as canonical output")?;
                Ok(RepairOutcome {
                    changed: canonical != original,
                    was_repaired: true,
                    canonical_json: canonical,
                })
            }
        }
    }
}

fn validate_mode(value: &Value, mode: RepairMode) -> Result<()> {
    match mode {
        RepairMode::Auto => Ok(()),
        RepairMode::JsonObject if value.is_object() => Ok(()),
        RepairMode::JsonArray if value.is_array() => Ok(()),
        RepairMode::JsonObject => bail!("repaired JSON is not an object"),
        RepairMode::JsonArray => bail!("repaired JSON is not an array"),
    }
}

#[cfg(test)]
mod tests {
    use super::{LlmJsonBackend, RepairEngine, RepairMode};

    #[test]
    fn repairs_common_llm_json() {
        let engine = RepairEngine::new(LlmJsonBackend);
        let out = engine
            .repair_json("{name: 'John', age: 30,}", RepairMode::JsonObject)
            .expect("repair should succeed");

        assert!(out.was_repaired);
        assert_eq!(out.canonical_json, r#"{"name":"John","age":30}"#);
    }

    #[test]
    fn rejects_wrong_top_level_mode() {
        let engine = RepairEngine::new(LlmJsonBackend);
        let err = engine
            .repair_json(r#"{"items":[1,2]}"#, RepairMode::JsonArray)
            .expect_err("shape mismatch should fail");

        assert!(err.to_string().contains("not an array"));
    }
}
