#![forbid(unsafe_code)]

use serde_json::{Value, json};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "think_subgoal_open",
            "description": "Open a subgoal card linked to a parent question.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "question_id": { "type": "string" },
                    "ref": { "type": "string" },
                    "parent_graph_doc": { "type": "string" },
                    "parent_trace_doc": { "type": "string" },
                    "child_graph_doc": { "type": "string" },
                    "child_trace_doc": { "type": "string" },
                    "message": { "type": "string" },
                    "meta": { "type": "object" }
                },
                "required": ["workspace", "question_id"]
            }
        }),
        json!({
            "name": "think_subgoal_close",
            "description": "Close a subgoal card and optionally attach a return card.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": { "type": "string" },
                    "subgoal_id": { "type": "string" },
                    "return_card": {
                        "anyOf": [
                            { "type": "object" },
                            { "type": "string" }
                        ]
                    },
                    "ref": { "type": "string" },
                    "parent_graph_doc": { "type": "string" },
                    "parent_trace_doc": { "type": "string" }
                },
                "required": ["workspace", "subgoal_id"]
            }
        }),
    ]
}
