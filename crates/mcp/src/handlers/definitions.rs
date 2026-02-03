#![forbid(unsafe_code)]

use serde_json::{Value, json};

use super::{branchmind, tasks};

pub(crate) fn handler_definitions() -> Vec<Value> {
    let mut handlers = Vec::new();
    handlers.push(json!({
        "name": "storage",
        "description": "Get storage paths and namespaces.",
        "inputSchema": { "type": "object", "properties": {}, "required": [] }
    }));
    handlers.extend(branchmind::branchmind_tool_definitions());
    handlers.extend(tasks::task_tool_definitions());
    augment_target_schemas(&mut handlers);
    handlers.sort_by_key(|tool| {
        tool.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    });
    handlers
}

fn target_ref_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "kind": { "type": "string", "enum": ["plan", "task"] }
        },
        "required": ["id"]
    })
}

fn target_any_schema() -> Value {
    json!({
        "anyOf": [
            { "type": "string" },
            target_ref_schema()
        ]
    })
}

fn augment_target_schemas(tools: &mut [Value]) {
    for tool in tools.iter_mut() {
        let name = tool
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(schema_obj) = tool.get_mut("inputSchema").and_then(|v| v.as_object_mut()) else {
            continue;
        };
        let Some(props) = schema_obj
            .get_mut("properties")
            .and_then(|v| v.as_object_mut())
        else {
            continue;
        };

        if props.contains_key("target") {
            props.insert("target".to_string(), target_any_schema());
            continue;
        }

        if name.starts_with("tasks_") && (props.contains_key("task") || props.contains_key("plan"))
        {
            props.insert("target".to_string(), target_any_schema());
        }
    }
}
