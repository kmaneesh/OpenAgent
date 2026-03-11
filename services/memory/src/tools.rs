//! Tool definitions and MCP-lite handler registration for the memory service.

use crate::handlers::{handle_delete, handle_index, handle_prune, handle_search};
use crate::metrics::MemoryTelemetry;
use fastembed::TextEmbedding;
use lancedb::connection::Connection;
use sdk_rust::{McpLiteServer, ToolDefinition};
use serde_json::json;
use std::sync::{Arc, Mutex};

pub fn make_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "memory.index".to_string(),
            description: concat!(
                "Embed and store text (content + optional metadata) into ",
                "LTS (long-term summaries) or STS (short-term conversation chain). ",
                "Returns the generated document id."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Text to embed and store"
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional metadata (session_id, source, user_id, type, tags, etc.)"
                    },
                    "store": {
                        "type": "string",
                        "enum": ["ltm", "stm"],
                        "description": "ltm = long-term memory; stm = short-term memory"
                    }
                },
                "required": ["content", "store"]
            }),
        },
        ToolDefinition {
            name: "memory.search".to_string(),
            description: concat!(
                "Semantic vector search over LTS, STS, or both. ",
                "Returns up to 5 results ranked by similarity score (0–1, higher = more relevant). ",
                "Each result includes id, content, metadata, created_at, store, score, distance."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language query"
                    },
                    "store": {
                        "type": "string",
                        "enum": ["ltm", "stm", "all"],
                        "description": "Store to search (default: all — merges and re-ranks by score)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "memory.delete".to_string(),
            description: concat!(
                "Delete a specific document by id from a store, ",
                "or purge all documents from a store by omitting id."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "store": {
                        "type": "string",
                        "enum": ["ltm", "stm"],
                        "description": "Store to delete from"
                    },
                    "id": {
                        "type": "string",
                        "description": "Document id to delete. Omit to purge the entire store."
                    }
                },
                "required": ["store"]
            }),
        },
        ToolDefinition {
            name: "memory.prune".to_string(),
            description: concat!(
                "Remove stale documents from the STS (short-term) store. ",
                "Deletes every row older than max_age_secs (default 86400 = 24 h). ",
                "Returns the number of documents pruned."
            )
            .to_string(),
            params: json!({
                "type": "object",
                "properties": {
                    "max_age_secs": {
                        "type": "integer",
                        "description": "Age threshold in seconds (default: 86400 = 24 h). Documents older than this are deleted."
                    }
                },
                "required": []
            }),
        },
    ]
}

pub fn register_handlers(
    server: &mut McpLiteServer,
    db: Arc<Connection>,
    model: Arc<Mutex<TextEmbedding>>,
    tel: Arc<MemoryTelemetry>,
) {
    let (db1, m1, t1) = (Arc::clone(&db), Arc::clone(&model), Arc::clone(&tel));
    server.register_tool("memory.index", move |p| {
        handle_index(p, Arc::clone(&db1), Arc::clone(&m1), Arc::clone(&t1))
    });

    let (db2, m2, t2) = (Arc::clone(&db), Arc::clone(&model), Arc::clone(&tel));
    server.register_tool("memory.search", move |p| {
        handle_search(p, Arc::clone(&db2), Arc::clone(&m2), Arc::clone(&t2))
    });

    let (db3, t3) = (Arc::clone(&db), Arc::clone(&tel));
    server.register_tool("memory.delete", move |p| {
        handle_delete(p, Arc::clone(&db3), Arc::clone(&t3))
    });

    let (db4, t4) = (Arc::clone(&db), Arc::clone(&tel));
    server.register_tool("memory.prune", move |p| {
        handle_prune(p, Arc::clone(&db4), Arc::clone(&t4))
    });
}
