#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn open_step_deeplink_is_supported() {
    let mut server = Server::start_initialized("open_step_deeplink_is_supported");

    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": "ws_open_step",
            "kind": "plan",
            "title": "Open step plan"
        } } }
    }));
    let plan = extract_tool_text(&plan);
    assert_eq!(plan.get("success").and_then(|v| v.as_bool()), Some(true));
    let plan_id = plan
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": "ws_open_step",
            "kind": "task",
            "parent": plan_id,
            "title": "Open step task",
            // We intentionally create enough steps so the hex step counter includes A-F
            // (e.g. STEP-0000000A). This guards `open STEP-*` for real-world workspaces.
            "steps": (0..12).map(|i| json!({
                "title": format!("S{i}"),
                "success_criteria": [format!("c{i}")]
            })).collect::<Vec<_>>()
        } } }
    }));
    let created = extract_tool_text(&created);
    assert_eq!(created.get("success").and_then(|v| v.as_bool()), Some(true));

    let result = created.get("result").expect("result");
    let task_id = result
        .get("id")
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let step0 = result
        .get("steps")
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .expect("steps[0]");
    let step_id = step0
        .get("step_id")
        .and_then(|v| v.as_str())
        .expect("step_id")
        .to_string();
    let path = step0
        .get("path")
        .and_then(|v| v.as_str())
        .expect("path")
        .to_string();

    let opened_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_open_step", "id": step_id } }
    }));
    let opened_step = extract_tool_text(&opened_step);
    assert_eq!(
        opened_step.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let opened = opened_step.get("result").expect("result");
    assert_eq!(opened.get("kind").and_then(|v| v.as_str()), Some("step"));
    assert_eq!(
        opened.get("task_id").and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert_eq!(
        opened
            .get("step")
            .and_then(|v| v.get("step_id"))
            .and_then(|v| v.as_str()),
        Some(step_id.as_str())
    );
    assert_eq!(
        opened
            .get("step")
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        Some(path.as_str())
    );

    let steps = result
        .get("steps")
        .and_then(|v| v.as_array())
        .expect("steps array");
    let (hex_step_id, hex_path) = steps
        .iter()
        .find_map(|s| {
            let step_id = s.get("step_id")?.as_str()?;
            let rest = step_id.strip_prefix("STEP-").unwrap_or(step_id);
            let has_hex_letter = rest.chars().any(|c| matches!(c, 'A'..='F' | 'a'..='f'));
            if !has_hex_letter {
                return None;
            }
            let path = s.get("path")?.as_str()?;
            Some((step_id.to_string(), path.to_string()))
        })
        .expect("a STEP-* id with hex A-F in its suffix");

    let opened_hex = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_open_step", "id": hex_step_id } }
    }));
    let opened_hex = extract_tool_text(&opened_hex);
    assert_eq!(
        opened_hex.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let opened_hex = opened_hex.get("result").expect("result");
    assert_eq!(
        opened_hex.get("kind").and_then(|v| v.as_str()),
        Some("step")
    );
    assert_eq!(
        opened_hex
            .get("step")
            .and_then(|v| v.get("step_id"))
            .and_then(|v| v.as_str()),
        Some(hex_step_id.as_str())
    );
    assert_eq!(
        opened_hex
            .get("step")
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        Some(hex_path.as_str())
    );

    let deeplink = format!("{task_id}@{path}");
    let opened_path = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_open_step", "id": deeplink } }
    }));
    let opened_path = extract_tool_text(&opened_path);
    assert_eq!(
        opened_path.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let opened = opened_path.get("result").expect("result");
    assert_eq!(opened.get("kind").and_then(|v| v.as_str()), Some("step"));
    assert_eq!(
        opened.get("task_id").and_then(|v| v.as_str()),
        Some(task_id.as_str())
    );
    assert_eq!(
        opened.get("path").and_then(|v| v.as_str()),
        Some(path.as_str())
    );
}
