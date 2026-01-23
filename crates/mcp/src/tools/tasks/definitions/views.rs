#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(crate) fn views_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "tasks_plan",
            "description": "Update plan checklist (`doc`, `steps`, `current`) and/or `advance=true`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "steps": { "type": "array", "items": { "type": "string" } },
                    "current": { "type": "integer" },
                    "doc": { "type": "string" },
                    "advance": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_contract",
            "description": "Set or clear a plan contract.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "current": { "type": "string" },
                    "contract_data": { "type": "object" },
                    "clear": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_complete",
            "description": "Set status for a plan/task (TODO/ACTIVE/DONE/PARKED/CANCELED).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "expected_revision": { "type": "integer" },
                    "status": { "type": "string", "enum": ["TODO", "ACTIVE", "DONE", "PARKED", "CANCELED"] },
                    "parked_for_s": { "type": "integer" },
                    "parked_until_ts_ms": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_context",
            "description": "List plans and tasks in a workspace (v0 skeleton).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "max_chars": { "type": "integer" },
                    "include_all": { "type": "boolean" },
                    "plan": { "type": "string" },
                    "task": { "type": "string" },
                    "plans_limit": { "type": "integer" },
                    "plans_cursor": { "type": "integer" },
                    "plans_status": { "type": "string" },
                    "tasks_limit": { "type": "integer" },
                    "tasks_cursor": { "type": "integer" },
                    "tasks_parent": { "type": "string" },
                    "tasks_status": { "type": "string" },
                    "domain": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_delta",
            "description": "List events since an event id (v0 skeleton).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "since": { "type": "string" },
                    "limit": { "type": "integer" },
                    "max_chars": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_focus_get",
            "description": "Get current focus (workspace-scoped).",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": { "type": "string" } },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_focus_set",
            "description": "Set current focus (workspace-scoped).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_focus_clear",
            "description": "Clear current focus (workspace-scoped).",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": { "type": "string" } },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_radar",
            "description": "Radar View: compact snapshot (Now/Why/Verify/Next/Blockers).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_resume",
            "description": "Load a plan/task with optional timeline events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "events_limit": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_resume_pack",
            "description": "Unified resume: task radar + timeline + decisions/evidence/blockers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "events_limit": { "type": "integer" },
                    "decisions_limit": { "type": "integer" },
                    "evidence_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_resume_super",
            "description": "Unified super-resume: radar + memory + signals with explicit degradation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "view": { "type": "string" },
                    "context_budget": { "type": "integer" },
                    "agent_id": { "type": "string" },
                    "events_limit": { "type": "integer" },
                    "decisions_limit": { "type": "integer" },
                    "evidence_limit": { "type": "integer" },
                    "blockers_limit": { "type": "integer" },
                    "notes_limit": { "type": "integer" },
                    "trace_limit": { "type": "integer" },
                    "cards_limit": { "type": "integer" },
                    "notes_cursor": { "type": "integer" },
                    "trace_cursor": { "type": "integer" },
                    "cards_cursor": { "type": "integer" },
                    "graph_diff": { "type": "boolean" },
                    "graph_diff_cursor": { "type": "integer" },
                    "graph_diff_limit": { "type": "integer" },
                    "engine_signals_limit": { "type": "integer" },
                    "engine_actions_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_snapshot",
            "description": "Portal snapshot (BM-L1): focus + next action (backed by resume_super + graph diff).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "view": { "type": "string" },
                    "context_budget": { "type": "integer" },
                    "agent_id": { "type": "string" },
                    "delta": { "type": "boolean" },
                    "refs": { "type": "boolean" },
                    "delta_limit": { "type": "integer" },
                    "events_limit": { "type": "integer" },
                    "decisions_limit": { "type": "integer" },
                    "evidence_limit": { "type": "integer" },
                    "blockers_limit": { "type": "integer" },
                    "notes_limit": { "type": "integer" },
                    "trace_limit": { "type": "integer" },
                    "cards_limit": { "type": "integer" },
                    "notes_cursor": { "type": "integer" },
                    "trace_cursor": { "type": "integer" },
                    "cards_cursor": { "type": "integer" },
                    "graph_diff_cursor": { "type": "integer" },
                    "graph_diff_limit": { "type": "integer" },
                    "engine_signals_limit": { "type": "integer" },
                    "engine_actions_limit": { "type": "integer" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": []
            }
        }),
        json!({
            "name": "tasks_context_pack",
            "description": "Bounded summary: radar + delta slice.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "max_chars": { "type": "integer" },
                    "delta_limit": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_mindpack",
            "description": "Workspace mindpack: bounded semantic compaction for resume-by-meaning.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "update": { "type": "boolean" },
                    "reason": { "type": "string" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_mirror",
            "description": "Export a compact plan/task slice for external consumers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "path": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_handoff",
            "description": "Shift report: done/remaining/risks + radar core.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" },
                    "max_chars": { "type": "integer" },
                    "read_only": { "type": "boolean" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_lint",
            "description": "Read-only integrity checks for a plan/task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "task": { "type": "string" },
                    "plan": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_templates_list",
            "description": "List built-in templates for scaffolding.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
        json!({
            "name": "tasks_scaffold",
            "description": "Create a plan/task from a template.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "template": { "type": "string" },
                    "kind": { "type": "string", "enum": ["plan", "task"] },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "parent": { "type": "string" },
                    "plan_title": { "type": "string" }
                },
                "required": ["workspace", "template", "title"]
            }
        }),
        json!({
            "name": "tasks_storage",
            "description": "Return storage paths and namespaces.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" }
                },
                "required": ["workspace"]
            }
        }),
    ]
}
