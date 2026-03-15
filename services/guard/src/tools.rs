use sdk_rust::ToolDefinition;
use serde_json::json;

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "guard.check".to_string(),
            description: "Check whether a sender is allowed to interact with the agent. Returns allowed=true if the sender is whitelisted or on the web platform (which bypasses the whitelist). Use this before routing any inbound message to Cortex.".to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "platform": {
                        "type": "string",
                        "description": "Platform identifier (e.g. telegram, discord, slack, whatsapp)."
                    },
                    "channel_id": {
                        "type": "string",
                        "description": "Platform-specific sender/channel identifier."
                    }
                },
                "required": ["platform", "channel_id"]
            }),
        },
        ToolDefinition {
            name: "guard.add".to_string(),
            description: "Add a sender to the whitelist, granting them access to the agent. Idempotent — calling again updates the note.".to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "platform": {
                        "type": "string",
                        "description": "Platform identifier."
                    },
                    "channel_id": {
                        "type": "string",
                        "description": "Platform-specific sender/channel identifier."
                    },
                    "note": {
                        "type": "string",
                        "description": "Optional human-readable label for this entry (e.g. a name or reason)."
                    }
                },
                "required": ["platform", "channel_id"]
            }),
        },
        ToolDefinition {
            name: "guard.remove".to_string(),
            description: "Remove a sender from the whitelist, revoking their access. Returns ok=false if the entry did not exist.".to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "platform": {
                        "type": "string",
                        "description": "Platform identifier."
                    },
                    "channel_id": {
                        "type": "string",
                        "description": "Platform-specific sender/channel identifier."
                    }
                },
                "required": ["platform", "channel_id"]
            }),
        },
        ToolDefinition {
            name: "guard.list".to_string(),
            description: "List all whitelisted senders, ordered newest first.".to_string(),
            params: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}
