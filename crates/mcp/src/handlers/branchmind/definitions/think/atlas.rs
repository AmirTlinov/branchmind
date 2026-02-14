#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "atlas_suggest",
            "description": "Suggest a directory-based atlas: propose anchors bound to key repo paths (mass onboarding helper).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "repo_root": { "type": "string", "description": "Absolute repo root path (optional; defaults to workspace bound_path)." },
                    "granularity": { "type": "string", "enum": ["top", "depth2"] },
                    "limit": { "type": "integer" },
                    "include_containers": { "type": "array", "items": { "type": "string" } },
                    "ignore_dirs": { "type": "array", "items": { "type": "string" } },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "macro_atlas_apply",
            "description": "Apply an atlas proposal: upsert anchors and bind them to repo paths (bind_paths → path:<repo_rel> refs).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "anchors": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "anchor": { "type": "string" },
                                "title": { "type": "string" },
                                "kind": { "type": "string" },
                                "status": { "type": "string" },
                                "description": { "type": ["string", "null"] },
                                "refs": { "type": "array", "items": { "type": "string" } },
                                "bind_paths": { "type": "array", "items": { "type": "string" } },
                                "aliases": { "type": "array", "items": { "type": "string" } },
                                "parent_id": { "type": ["string", "null"] },
                                "depends_on": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["anchor", "title", "kind"]
                        }
                    },
                    "atomic": { "type": "boolean" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace", "anchors"]
            }
        }),
        json!({
            "name": "atlas_bindings_list",
            "description": "List path→anchor bindings (transparent navigation index).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "prefix": { "type": "string", "description": "Repo-relative prefix filter (e.g. \"crates\")." },
                    "anchor": { "type": "string", "description": "Anchor id filter (a:<slug>)." },
                    "limit": { "type": "integer" },
                    "offset": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
