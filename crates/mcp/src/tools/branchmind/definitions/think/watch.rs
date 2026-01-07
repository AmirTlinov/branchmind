#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![json!({
        "name": "think_watch",
        "description": "Return a bounded watch view (frontier + recent trace steps).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": { "type": "string" },
                "target": { "type": "string" },
                "ref": { "type": "string" },
                "graph_doc": { "type": "string" },
                "trace_doc": { "type": "string" },
                "view": { "type": "string", "description": "Relevance view: smart | explore | audit" },
                "step": { "type": "string" },
                "agent_id": { "type": "string" },
                "all_lanes": { "type": "boolean" },
                "limit_candidates": { "type": "integer" },
                "limit_hypotheses": { "type": "integer" },
                "limit_questions": { "type": "integer" },
                "limit_subgoals": { "type": "integer" },
                "limit_tests": { "type": "integer" },
                "trace_limit_steps": { "type": "integer" },
                "trace_statement_max_bytes": { "type": "integer" },
                "engine_signals_limit": { "type": "integer" },
                "engine_actions_limit": { "type": "integer" },
                "context_budget": { "type": "integer" },
                "max_chars": { "type": "integer" }
            },
            "required": ["workspace"]
        }
    })]
}
