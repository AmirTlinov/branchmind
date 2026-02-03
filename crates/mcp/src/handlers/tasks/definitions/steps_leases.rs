#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn steps_leases_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_step_lease_get",
            "description": "Inspect current step lease state (optional multi-agent room lock).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "target": { "type": ["string", "object"] },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_step_lease_claim",
            "description": "Claim a step lease for the caller (agent_id). Fails if held unless force=true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "target": { "type": ["string", "object"] },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "ttl_seq": { "type": "integer" },
                    "force": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_step_lease_renew",
            "description": "Renew an existing step lease held by agent_id (extends expires_seq).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "target": { "type": ["string", "object"] },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "ttl_seq": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_step_lease_release",
            "description": "Release an existing step lease held by agent_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "target": { "type": ["string", "object"] },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" },
                    "agent_id": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
