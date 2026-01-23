#![forbid(unsafe_code)]

use serde_json::{Value, json};

use super::{branchmind, tasks};

pub(crate) fn tool_definitions(toolset: crate::Toolset) -> Vec<Value> {
    let mut tools = Vec::new();
    tools.push(json!({
        "name": "storage",
        "description": "Get storage paths and namespaces.",
        "inputSchema": { "type": "object", "properties": {}, "required": [] }
    }));
    tools.extend(branchmind::branchmind_tool_definitions());
    tools.extend(tasks::task_tool_definitions());
    augment_target_schemas(&mut tools);
    tools = match toolset {
        crate::Toolset::Core => tools.into_iter().filter(is_core_tool).collect(),
        crate::Toolset::Daily => tools.into_iter().filter(is_daily_tool).collect(),
        crate::Toolset::Full => tools,
    };
    // For better “first screen” UX, keep core/daily-driver tools first even in full mode.
    tools.sort_by_key(|tool| {
        let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
        (tool_tier(name), name.to_string())
    });
    tools
}

fn tool_tier(name: &str) -> u8 {
    if is_core_tool_name(name) {
        0
    } else if is_daily_tool_name(name) {
        1
    } else {
        2
    }
}

fn is_daily_tool(tool: &Value) -> bool {
    let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
    is_daily_tool_name(name)
}

fn is_daily_tool_name(name: &str) -> bool {
    if is_core_tool_name(name) {
        return true;
    }

    // Daily-driver rule: keep the advertised surface extremely small.
    matches!(
        name,
        "tasks_macro_delegate"
            | "tasks_macro_close_step"
            | "tasks_lint"
            | "tasks_jobs_radar"
            | "open"
            | "skill"
            | "workspace_use"
            | "think_card"
            | "think_playbook"
    )
}

fn is_core_tool(tool: &Value) -> bool {
    let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("");
    is_core_tool_name(name)
}

fn is_core_tool_name(name: &str) -> bool {
    matches!(
        name,
        // Core: ultra-minimal “portal” set (golden path).
        "status" | "tasks_macro_start" | "tasks_snapshot"
    )
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
