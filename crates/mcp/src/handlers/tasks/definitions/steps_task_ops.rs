#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn steps_task_ops_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_edit",
            "description": "Edit plan/task meta fields (v0).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "context": { "type": "string" },
                    "priority": { "type": "string" },
                    "new_domain": { "type": "string" },
                    "reasoning_mode": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "depends_on": { "type": "array", "items": { "type": "string" } },
                    "contract": { "type": "string" },
                    "contract_data": { "type": "object" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_delete",
            "description": "Delete a plan/task or a step by selector.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_task_add",
            "description": "Add a task node inside a step plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "parent_step": { "type": "string" },
                    "title": { "type": "string" },
                    "status": { "type": "string" },
                    "status_manual": { "type": "boolean" },
                    "priority": { "type": "string" },
                    "blocked": { "type": "boolean" },
                    "description": { "type": "string" },
                    "context": { "type": "string" },
                    "blockers": { "type": "array", "items": { "type": "string" } },
                    "dependencies": { "type": "array", "items": { "type": "string" } },
                    "next_steps": { "type": "array", "items": { "type": "string" } },
                    "problems": { "type": "array", "items": { "type": "string" } },
                    "risks": { "type": "array", "items": { "type": "string" } },
                    "success_criteria": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace", "parent_step", "title"]
            }
        }),
        json!({
            "name": "tasks_task_define",
            "description": "Update a task node inside a step plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "title": { "type": "string" },
                    "status": { "type": "string" },
                    "status_manual": { "type": "boolean" },
                    "priority": { "type": "string" },
                    "blocked": { "type": "boolean" },
                    "description": { "type": "string" },
                    "context": { "type": "string" },
                    "blockers": { "type": "array", "items": { "type": "string" } },
                    "dependencies": { "type": "array", "items": { "type": "string" } },
                    "next_steps": { "type": "array", "items": { "type": "string" } },
                    "problems": { "type": "array", "items": { "type": "string" } },
                    "risks": { "type": "array", "items": { "type": "string" } },
                    "success_criteria": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace", "path"]
            }
        }),
        json!({
            "name": "tasks_task_delete",
            "description": "Delete a task node inside a step plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" }
                },
                "required": ["workspace", "path"]
            }
        }),
        json!({
            "name": "tasks_evidence_capture",
            "description": "Attach artifacts/checks to a step or task/plan root.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "items": { "type": "array", "items": { "type": "object" } },
                    "artifacts": { "type": "array", "items": { "type": "object" } },
                    "checks": { "type": "array", "items": { "type": "string" } },
                    "attachments": { "type": "array", "items": { "type": "string" } },
                    "checkpoint": {
                        "anyOf": [
                            { "type": "string", "enum": ["criteria", "tests", "security", "perf", "docs"] },
                            { "type": "array", "items": { "type": "string", "enum": ["criteria", "tests", "security", "perf", "docs"] } }
                        ]
                    }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
