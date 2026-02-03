#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn graph_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "graph_apply",
            "description": "Apply a batch of typed graph ops to a target graph or an explicit (branch, doc).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "ops": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": { "type": "string", "enum": ["node_upsert", "node_delete", "edge_upsert", "edge_delete"] },
                                "id": { "type": "string" },
                                "type": { "type": "string" },
                                "title": { "type": "string" },
                                "text": { "type": "string" },
                                "status": { "type": "string" },
                                "tags": { "type": "array", "items": { "type": "string" } },
                                "meta": { "type": "object" },
                                "from": { "type": "string" },
                                "rel": { "type": "string" },
                                "to": { "type": "string" }
                            },
                            "required": ["op"]
                        }
                    }
                },
                "required": ["workspace", "ops"]
            }
        }),
        json!({
            "name": "graph_query",
            "description": "Query a bounded slice of the effective graph view for a target or an explicit (branch, doc).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "ids": { "type": "array", "items": { "type": "string" } },
                    "types": { "type": "array", "items": { "type": "string" } },
                    "status": { "type": "string" },
                    "tags_any": { "type": "array", "items": { "type": "string" } },
                    "tags_all": { "type": "array", "items": { "type": "string" } },
                    "text": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "include_edges": { "type": "boolean" },
                    "edges_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "graph_validate",
            "description": "Validate invariants of the effective graph view for a target or an explicit (branch, doc).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "target": { "type": "string" },
                    "branch": { "type": "string" },
                    "doc": { "type": "string" },
                    "max_errors": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "graph_diff",
            "description": "Directional diff between two branches for a single graph document (patch-style).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "from", "to"]
            }
        }),
        json!({
            "name": "graph_merge",
            "description": "Merge graph changes from a derived branch back into its base branch (3-way, conflict-producing).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "from": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "dry_run": { "type": "boolean" },
                    "merge_to_base": { "type": "boolean" }
                },
                "required": ["workspace", "from"]
            }
        }),
        json!({
            "name": "graph_conflicts",
            "description": "List graph merge conflicts for a destination branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "status": { "type": "string" },
                    "cursor": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "into"]
            }
        }),
        json!({
            "name": "graph_conflict_show",
            "description": "Show a single conflict with base/theirs/ours snapshots.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "conflict_id": { "type": "string" }
                },
                "required": ["workspace", "conflict_id"]
            }
        }),
        json!({
            "name": "graph_conflict_resolve",
            "description": "Resolve a conflict and optionally apply the chosen snapshot into the destination branch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "conflict_id": { "type": "string" },
                    "resolution": { "type": "string", "enum": ["use_from", "use_into"] }
                },
                "required": ["workspace", "conflict_id", "resolution"]
            }
        }),
    ]
}
