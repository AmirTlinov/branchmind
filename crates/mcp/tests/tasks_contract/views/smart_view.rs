#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_resume_super_smart_includes_step_focus_and_prefers_pins() {
    let mut server = Server::start_initialized("tasks_resume_super_smart_prefers_pins");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_smart_view",
                "plan_title": "Plan Smart",
                "task_title": "Task Smart",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": ["b1"] }
                ]
            } } }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let _card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_smart_view",
                "target": task_id.clone(),
                "card": {
                    "id": "CARD-PIN",
                    "type": "note",
                    "title": "Pinned note",
                    "text": "Keep this visible"
                }
            } } }
    }));

    let _pin = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.pin", "args": {
                "workspace": "ws_smart_view",
                "target": task_id.clone(),
                "targets": ["CARD-PIN"],
                "pinned": true
            } } }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": {
                "workspace": "ws_smart_view",
                "task": task_id,
                "view": "smart",
                "cards_limit": 1,
                "max_chars": 8000
            } } }
    }));
    let resume_text = extract_tool_text(&resume);
    let result = resume_text.get("result").expect("result");

    assert!(
        result.get("step_focus").is_some(),
        "smart view should include step_focus when a first open step exists"
    );

    let first_id = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        first_id, "CARD-PIN",
        "smart cards should prefer pinned items"
    );
}

#[test]
fn tasks_resume_super_context_budget_defaults_to_smart_view() {
    let mut server =
        Server::start_initialized("tasks_resume_super_context_budget_defaults_to_smart");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_context_budget",
                "plan_title": "Plan Budget",
                "task_title": "Task Budget",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            } } }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": { "workspace": "ws_context_budget", "task": task_id, "context_budget": 4000 } } }
    }));
    let resume_text = extract_tool_text(&resume);
    let result = resume_text.get("result").expect("result");

    assert!(
        result.get("step_focus").is_some(),
        "context_budget should default to smart view (step_focus present)"
    );
}

#[test]
fn tasks_resume_super_tight_budget_degrades_to_capsule_only_instead_of_minimal_signal() {
    let mut server =
        Server::start_initialized("tasks_resume_super_tight_budget_degrades_to_capsule_only");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_smart_capsule_budget",
                "plan_title": "Plan",
                "task_title": "Task",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": ["b1"] }
                ]
            } } }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let baseline = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": {
                "workspace": "ws_smart_capsule_budget",
                "task": task_id.clone(),
                "view": "smart",
                "max_chars": 20000
            } } }
    }));
    let baseline_text = extract_tool_text(&baseline);
    let capsule = baseline_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .cloned()
        .expect("capsule");
    let degradation = baseline_text
        .get("result")
        .and_then(|v| v.get("degradation"))
        .cloned()
        .expect("degradation");

    // Choose a budget that can fit the capsule-only fallback, but is too tight for the full envelope.
    let capsule_only = json!({
        "capsule": capsule,
        "degradation": degradation,
        "truncated": true
    });
    let capsule_only_len = serde_json::to_string(&capsule_only)
        .expect("serialize capsule-only")
        .len();

    let tight = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": {
                "workspace": "ws_smart_capsule_budget",
                "task": task_id,
                "view": "smart",
                "max_chars": capsule_only_len + 32
            } } }
    }));
    let tight_text = extract_tool_text(&tight);
    let result = tight_text.get("result").expect("result");

    let capsule_type = result
        .get("capsule")
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(capsule_type, "handoff_capsule");

    assert!(
        result.get("target").is_none(),
        "tight budgets should degrade to capsule-only (drop full envelope fields like target)"
    );
    assert!(
        result.get("memory").is_none(),
        "tight budgets should drop memory envelope when keeping capsule-only"
    );
    assert!(
        result.get("signals").is_none(),
        "tight budgets should drop signals envelope when keeping capsule-only"
    );
    assert!(
        result.get("timeline").is_none(),
        "tight budgets should drop timeline envelope when keeping capsule-only"
    );
}
