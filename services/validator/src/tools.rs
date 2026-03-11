use sdk_rust::ToolDefinition;
use serde_json::json;

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "validator.repair_json".to_string(),
        description: "Repair malformed JSON and return compact canonical JSON. Use mode json_object or json_array to enforce the expected top-level shape, or auto to accept any valid JSON value.".to_string(),
        params: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Raw malformed JSON or LLM output containing JSON."
                },
                "mode": {
                    "type": "string",
                    "enum": ["auto", "json_object", "json_array"],
                    "description": "Expected repaired JSON top-level shape. Defaults to auto."
                }
            },
            "required": ["text"]
        }),
    }]
}
