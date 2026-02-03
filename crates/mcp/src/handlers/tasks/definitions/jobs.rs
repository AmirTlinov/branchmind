#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn jobs_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_jobs_create",
            "description": "Create a delegation job (does not execute anything).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "title": { "type": "string" },
                    "prompt": { "type": "string" },
                    "kind": { "type": "string" },
                    "priority": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "executor": { "type": "string", "enum": ["codex", "claude_code", "auto"] },
                    "executor_profile": { "type": "string", "enum": ["fast", "deep", "audit"] },
                    "policy": {
                        "type": "object",
                        "properties": {
                            "prefer": { "type": "array", "items": { "type": "string" } },
                            "forbid": { "type": "array", "items": { "type": "string" } },
                            "min_profile": { "type": "string" }
                        }
                    },
                    "expected_artifacts": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "title", "prompt"]
            }
        }),
        json!({
            "name": "tasks_jobs_list",
            "description": "List delegation jobs (bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "status": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_jobs_radar",
            "description": "Radar: list active jobs with a low-noise attention hint (optional reply shortcut).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "status": { "type": "string" },
                    "task": { "type": "string" },
                    "anchor": { "type": "string" },
                    "limit": { "type": "integer" },
                    "runners_limit": { "type": "integer" },
                    "runners_status": { "type": "string", "description": "optional filter: idle|live" },
                    "stale_after_s": { "type": "integer" },
                    "reply_job": { "type": "string" },
                    "reply_message": { "type": "string" },
                    "reply_refs": { "type": "array", "items": { "type": "string" } },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_jobs_open",
            "description": "Open a job (status + spec + recent events).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "include_prompt": { "type": "boolean" },
                    "include_events": { "type": "boolean" },
                    "include_meta": { "type": "boolean" },
                    "max_events": { "type": "integer" },
                    "before_seq": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_proof_attach",
            "description": "Attach proof receipts from a job to a task/step (evidence capture).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "task": { "type": "string" },
                    "step_id": { "type": "string" },
                    "path": { "type": "string" },
                    "checkpoint": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "artifact_ref": { "type": "string" },
                    "max_refs": { "type": "integer" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_tail",
            "description": "Tail job events incrementally (seq > after_seq).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "after_seq": { "type": "integer" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_jobs_claim",
            "description": "Claim a job (QUEUED -> RUNNING). Can optionally reclaim stale RUNNING jobs.",
            "inputSchema": {
                "type": "object",
                "oneOf": [
                    { "required": ["workspace", "job", "runner_id"] },
                    { "required": ["workspace", "job", "runner"] }
                ],
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "runner_id": { "type": "string", "description": "stable runner identity (recommended)" },
                    "runner": { "type": "string", "description": "deprecated alias for runner_id" },
                    "allow_stale": { "type": "boolean" },
                    "lease_ttl_ms": { "type": "integer", "description": "claim lease TTL; renewed by tasks_jobs_report heartbeats" }
                }
            }
        }),
        json!({
            "name": "tasks_jobs_message",
            "description": "Send a manager message to a job (QUEUED/RUNNING).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "message": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "fmt": { "type": "string" }
                },
                "required": ["workspace", "job", "message"]
            }
        }),
        json!({
            "name": "tasks_jobs_report",
            "description": "Append a progress event to a running job (bounded).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "runner_id": { "type": "string" },
                    "claim_revision": { "type": "integer" },
                    "lease_ttl_ms": { "type": "integer" },
                    "kind": { "type": "string" },
                    "message": { "type": "string" },
                    "percent": { "type": "integer" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "job", "runner_id", "claim_revision", "message"]
            }
        }),
        json!({
            "name": "tasks_jobs_complete",
            "description": "Complete a job (DONE/FAILED/CANCELED) and attach stable refs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "runner_id": { "type": "string" },
                    "claim_revision": { "type": "integer" },
                    "status": { "type": "string" },
                    "summary": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "job", "runner_id", "claim_revision", "status"]
            }
        }),
        json!({
            "name": "tasks_jobs_requeue",
            "description": "Requeue a terminal job back to QUEUED (bounded, audit event).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "job": { "type": "string" },
                    "reason": { "type": "string" },
                    "refs": { "type": "array", "items": { "type": "string" } },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "job"]
            }
        }),
        json!({
            "name": "tasks_runner_heartbeat",
            "description": "Runner liveness lease update (explicit live/idle/offline status; used by external runners).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "runner_id": { "type": "string" },
                    "status": { "type": "string", "description": "idle|live" },
                    "active_job_id": { "type": "string" },
                    "lease_ttl_ms": { "type": "integer" },
                    "executors": { "type": "array", "items": { "type": "string" } },
                    "profiles": { "type": "array", "items": { "type": "string" } },
                    "supports_artifacts": { "type": "array", "items": { "type": "string" } },
                    "max_parallel": { "type": "integer" },
                    "sandbox_policy": { "type": "string" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "runner_id", "status"]
            }
        }),
    ]
}
