#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn steps_lifecycle_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_decompose",
            "description": "Add steps to a task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "parent": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "success_criteria": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["title", "success_criteria"]
                        }
                    }
                },
                "required": ["workspace", "steps"]
            }
        }),
        json!({
            "name": "tasks_define",
            "description": "Update step fields (title/success_criteria/tests/blockers).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "title": { "type": "string" },
                    "success_criteria": { "type": "array", "items": { "type": "string" } },
                    "tests": { "type": "array", "items": { "type": "string" } },
                    "blockers": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_note",
            "description": "Add a progress note to a step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["workspace", "note"]
            }
        }),
        json!({
            "name": "tasks_verify",
            "description": "Confirm checkpoints for a step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "checkpoints": {
                        "anyOf": [
                            { "type": "string", "enum": ["all", "gate"] },
                            {
                                "type": "object",
                                "properties": {
                                    "criteria": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "tests": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "security": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "perf": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "docs": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] }
                                }
                            }
                        ]
                    }
                },
                "required": ["workspace", "checkpoints"]
            }
        }),
        json!({
            "name": "tasks_done",
            "description": "Mark a step completed (requires confirmed checkpoints).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_close_step",
            "description": "Atomically confirm checkpoints and close a step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "path": { "type": "string" },
                    "step_id": { "type": "string" },
                    "checkpoints": {
                        "anyOf": [
                            { "type": "string", "enum": ["all", "gate"] },
                            {
                                "type": "object",
                                "properties": {
                                    "criteria": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "tests": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "security": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "perf": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] },
                                    "docs": { "anyOf": [{ "type": "boolean" }, { "type": "object", "properties": { "confirmed": { "type": "boolean" } } }] }
                                }
                            }
                        ]
                    }
                },
                "required": ["workspace", "checkpoints"]
            }
        }),
    ]
}
