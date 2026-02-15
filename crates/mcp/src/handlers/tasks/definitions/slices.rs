#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn slices_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_slices_propose_next",
            "description": "Propose exactly one next slice plan spec (read-only; does not write to store).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "plan": { "type": "string" },
                    "objective": { "type": "string" },
                    "constraints": { "type": "array", "items": { "type": "string" } },
                    "policy": { "type": "string", "enum": ["fail_closed"] }
                },
                "required": ["workspace", "plan"]
            }
        }),
        json!({
            "name": "tasks_slices_apply",
            "description": "Apply one slice plan spec: creates slice_id (SLC-...), slice task container, and step tree (SliceTasks -> Steps).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "slice_plan_spec": { "type": "object" },
                    "policy": { "type": "string", "enum": ["fail_closed"] }
                },
                "required": ["workspace", "plan", "slice_plan_spec"]
            }
        }),
        json!({
            "name": "tasks_slice_open",
            "description": "Open a slice by slice_id: returns binding, slice task, parsed slice plan spec, step tree and ready-to-run jobs actions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "slice_id": { "type": "string" }
                },
                "required": ["workspace", "slice_id"]
            }
        }),
        json!({
            "name": "tasks_slice_validate",
            "description": "Validate slice plan structure + step tree + budgets (fail-closed).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "slice_id": { "type": "string" },
                    "policy": { "type": "string", "enum": ["fail_closed"] }
                },
                "required": ["workspace", "slice_id"]
            }
        }),
    ]
}
