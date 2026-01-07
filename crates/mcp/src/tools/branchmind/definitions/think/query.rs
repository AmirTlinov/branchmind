#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "think_context",
            "description": "Return a bounded low-noise thinking context slice (cards from the graph).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "limit_cards": { "type": "integer" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "think_query",
            "description": "Query thinking cards via graph filters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "ids": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "types": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "status": { "type": "string" },
                    "tags_any": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "tags_all": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "text": { "type": "string" },
                    "limit": { "type": "integer" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "think_pack",
            "description": "Return a compact think_context plus frontier summary.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "limit_candidates": { "type": "integer" },
                    "limit_hypotheses": { "type": "integer" },
                    "limit_questions": { "type": "integer" },
                    "limit_subgoals": { "type": "integer" },
                    "limit_tests": { "type": "integer" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "think_frontier",
            "description": "Return prioritized frontier cards by type (recency + status).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "limit_hypotheses": { "type": "integer" },
                    "limit_questions": { "type": "integer" },
                    "limit_subgoals": { "type": "integer" },
                    "limit_tests": { "type": "integer" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "think_next",
            "description": "Return the next best card candidate.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "ref": { "type": "string" },
                    "graph_doc": { "type": "string" },
                    "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                    "step": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "all_lanes": { "type": "boolean" },
                    "context_budget": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
