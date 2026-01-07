#![forbid(unsafe_code)]

use serde_json::{Value, json};

fn think_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "agent_id": { "type": "string" },
            "frame": { "anyOf": [{ "type": "string" }, { "type": "object" }] },
            "hypothesis": { "anyOf": [{ "type": "string" }, { "type": "object" }] },
            "test": { "anyOf": [{ "type": "string" }, { "type": "object" }] },
            "evidence": { "anyOf": [{ "type": "string" }, { "type": "object" }] },
            "decision": { "anyOf": [{ "type": "string" }, { "type": "object" }] },
            "status": { "type": "object" },
            "note_decision": { "type": "boolean" },
            "note_title": { "type": "string" },
            "note_format": { "type": "string" }
        }
    })
}

pub(crate) fn bootstrap_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_bootstrap",
            "description": "One-call task bootstrap: create task + steps + checkpoints.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "plan": { "type": "string" },
                    "parent": { "type": "string" },
                    "plan_title": { "type": "string" },
                    "plan_template": { "type": "string" },
                    "task_title": { "type": "string" },
                    "description": { "type": "string" },
                    "template": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "success_criteria": { "type": "array", "items": { "type": "string" } },
                                "tests": { "type": "array", "items": { "type": "string" } },
                                "blockers": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["title", "success_criteria", "tests"]
                        }
                    },
                    "think": think_schema()
                },
                "required": ["workspace", "task_title"]
            }
        }),
        json!({
            "name": "tasks_macro_start",
            "description": "One-call start: bootstrap task + return super-resume.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "plan": { "type": "string" },
                    "parent": { "type": "string" },
                    "plan_title": { "type": "string" },
                    "plan_template": { "type": "string" },
                    "task_title": { "type": "string" },
                    "description": { "type": "string" },
                    "template": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "success_criteria": { "type": "array", "items": { "type": "string" } },
                                "tests": { "type": "array", "items": { "type": "string" } },
                                "blockers": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["title", "success_criteria", "tests"]
                        }
                    },
                    "think": think_schema(),
                    "view": { "type": "string" },
                    "resume_max_chars": { "type": "integer" }
                },
                "required": ["task_title"]
            }
        }),
        json!({
            "name": "tasks_macro_close_step",
            "description": "One-call close: confirm checkpoints + close step + return super-resume.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "task": { "type": "string" },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "checkpoints": {
                        "anyOf": [
                            { "type": "string", "enum": ["all", "gate"] },
                            { "type": "object" }
                        ]
                    },
                    "note": { "type": "string" },
                    "proof": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } },
                            { "type": "object" }
                        ]
                    },
                    "view": { "type": "string" },
                    "resume_max_chars": { "type": "integer" }
                },
                "required": []
            }
        }),
        json!({
            "name": "tasks_macro_finish",
            "description": "One-call finish: tasks_complete + handoff.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "status": { "type": "string" },
                    "final_note": { "type": "string" },
                    "handoff_max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_macro_create_done",
            "description": "One-call create + verify + done (single-step task).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "plan": { "type": "string" },
                    "parent": { "type": "string" },
                    "plan_title": { "type": "string" },
                    "task_title": { "type": "string" },
                    "description": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "success_criteria": { "type": "array", "items": { "type": "string" } },
                                "tests": { "type": "array", "items": { "type": "string" } },
                                "blockers": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["title", "success_criteria", "tests"]
                        }
                    }
                },
                "required": ["workspace", "task_title", "steps"]
            }
        }),
    ]
}
