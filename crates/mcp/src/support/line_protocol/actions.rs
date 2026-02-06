#![forbid(unsafe_code)]

use serde_json::Value;

use super::util::render_kv_args;

pub(super) fn append_actions_as_commands_limited(
    lines: &mut Vec<String>,
    response: &Value,
    limit: usize,
) {
    if limit == 0 {
        return;
    }
    let Some(actions) = response.get("actions").and_then(|v| v.as_array()) else {
        return;
    };

    // Two-pass selection:
    // 1) Prefer portal-first recovery actions (skip schema/example auto-actions).
    // 2) Fallback to example.call and schema.get when nothing else exists.
    let mut added = 0usize;

    added += append_actions_matching(lines, actions, limit - added, |action_id, tool, args| {
        if action_id.starts_with("recover.schema.get::") {
            return false;
        }
        if action_id.starts_with("recover.example.call::") {
            return false;
        }
        // In line protocol, prefer domain actions over system ops.
        if tool == "system"
            && args
                .get("op")
                .and_then(|v| v.as_str())
                .is_some_and(|op| op == "schema.get")
        {
            return false;
        }
        true
    });
    if added > 0 || added >= limit {
        return;
    }

    // Second pass: show example minimal call.
    added += append_actions_matching(lines, actions, limit - added, |action_id, _tool, _args| {
        action_id.starts_with("recover.example.call::")
    });
    if added > 0 || added >= limit {
        return;
    }

    // Third pass: schema.get (lowest preference in BM-L1).
    let _ = append_actions_matching(lines, actions, limit - added, |action_id, _tool, _args| {
        action_id.starts_with("recover.schema.get::")
    });
}

fn append_actions_matching(
    lines: &mut Vec<String>,
    actions: &[Value],
    limit: usize,
    predicate: impl Fn(&str, &str, &Value) -> bool,
) -> usize {
    if limit == 0 {
        return 0;
    }
    let mut added = 0usize;
    for action in actions {
        if added >= limit {
            break;
        }
        let action_id = action
            .get("action_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tool = action.get("tool").and_then(|v| v.as_str()).unwrap_or("");
        let args = action.get("args").unwrap_or(&Value::Null);
        if !predicate(action_id, tool, args) {
            continue;
        }
        if let Some(cmd) = render_action_command(tool, args) {
            lines.push(cmd);
            added += 1;
        }
    }
    added
}

fn render_action_command(tool: &str, args: &Value) -> Option<String> {
    // v1: actions are copy/paste-ready portal calls. Render them directly without translating
    // back into legacy tool names (strict surface = 10).
    render_simple_tool_call(tool, args)
}

fn render_simple_tool_call(tool: &str, args: &Value) -> Option<String> {
    let args_str = render_kv_args(args).unwrap_or_default();
    if args_str.is_empty() {
        Some(tool.to_string())
    } else {
        Some(format!("{tool} {args_str}"))
    }
}
