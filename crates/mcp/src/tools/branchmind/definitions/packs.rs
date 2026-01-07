#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn packs_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "context_pack",
            "description": "Bounded resumption pack that merges notes, trace, and graph cards into one response.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "notes_doc": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "notes_limit": { "type": "integer" },
                    "trace_limit": { "type": "integer" },
                    "limit_cards": { "type": "integer" },
                    "decisions_limit": { "type": "integer" },
                    "evidence_limit": { "type": "integer" },
                    "blockers_limit": { "type": "integer" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "context_pack_export",
            "description": "Write a bounded context_pack result to a file for external tooling (e.g., Context Finder memory overlay).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "out_file": { "type": "string", "description": "Filesystem path to write the context_pack result JSON into." },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "notes_doc": { "type": "string" },
                    "trace_doc": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "notes_limit": { "type": "integer" },
                    "trace_limit": { "type": "integer" },
                    "limit_cards": { "type": "integer" },
                    "decisions_limit": { "type": "integer" },
                    "evidence_limit": { "type": "integer" },
                    "blockers_limit": { "type": "integer" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace", "out_file"]
            }
        }),
        json!({
            "name": "export",
            "description": "Build a bounded snapshot for fast IDE/agent resumption (target + refs + tail notes/trace).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "notes_limit": { "type": "integer" },
                    "trace_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "target"]
            }
        }),
    ]
}
