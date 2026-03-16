use sdk_rust::ToolDefinition;
use serde_json::json;

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "guard.check".to_string(),
            description: "Check whether a sender is allowed to interact with the agent. Returns allowed=true/false, reason ('allowed'|'blocked'|'unknown'|'platform_bypass'), and the contact's name. Use this before routing any inbound message to Cortex.".to_string(),
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
            name: "guard.allow".to_string(),
            description: "Allow a contact to interact with the agent (sets status='allowed'). Idempotent — calling again updates name/note. Also accepts 'label' for backward compatibility.".to_string(),
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
                    "name": {
                        "type": "string",
                        "description": "Optional human-readable name for this contact."
                    },
                    "note": {
                        "type": "string",
                        "description": "Optional admin note (reason for allowing, etc.)."
                    }
                },
                "required": ["platform", "channel_id"]
            }),
        },
        ToolDefinition {
            name: "guard.block".to_string(),
            description: "Block a contact (sets status='blocked'). Future messages from this contact will be rejected with reason='blocked'.".to_string(),
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
                        "description": "Optional reason for blocking."
                    }
                },
                "required": ["platform", "channel_id"]
            }),
        },
        ToolDefinition {
            name: "guard.name".to_string(),
            description: "Set or update the human-readable name for an existing contact. Returns ok=false if the contact is not yet in the guard table.".to_string(),
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
                    "name": {
                        "type": "string",
                        "description": "Human-readable display name for this contact."
                    }
                },
                "required": ["platform", "channel_id", "name"]
            }),
        },
        ToolDefinition {
            name: "guard.remove".to_string(),
            description: "Remove a contact from the guard table entirely. Returns ok=false if the entry did not exist.".to_string(),
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
            description: "List all contacts in the guard table (allowed, blocked, and unknown), ordered by most recently seen.".to_string(),
            params: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}
