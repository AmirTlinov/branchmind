#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

fn assert_step_scoped_think_card_actions_are_copy_paste_valid(blocked_text: &str) {
    let mut saw = false;
    for line in blocked_text.lines() {
        if !line.starts_with("think args=") || !line.contains("cmd=think.card") {
            continue;
        }
        saw = true;

        let prefix = "think args=";
        let start = line.find(prefix).expect("think args prefix") + prefix.len();
        let rest = &line[start..];
        let end = rest
            .find(" budget_profile=")
            .or_else(|| rest.find(" cmd="))
            .unwrap_or(rest.len());
        let args_json = &rest[..end];

        let parsed: serde_json::Value =
            serde_json::from_str(args_json).expect("think args must be valid JSON");
        let obj = parsed
            .as_object()
            .expect("think args must be a JSON object");

        // Step-scoped card creation is tied to `target`/focus. Branch/doc overrides are forbidden
        // (and would force agents to manually patch recovery actions).
        assert!(
            obj.contains_key("step"),
            "expected step-scoped think.card action: {line}"
        );
        assert!(
            obj.contains_key("target"),
            "expected target-scoped think.card action: {line}"
        );
        for forbidden in [
            "branch",
            "trace_doc",
            "graph_doc",
            "notes_doc",
            "ref",
            "doc",
        ] {
            assert!(
                !obj.contains_key(forbidden),
                "unexpected {forbidden} override in step-scoped recovery action: {line}"
            );
        }
    }
    assert!(saw, "expected at least one think.card recovery action line");
}

#[test]
fn deep_reasoning_mode_requires_resolved_synthesis_decision() {
    let mut server = Server::start_initialized("tasks_deep_reasoning_gate");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_deep_gate",
                "plan_title": "Plan Deep Gate",
                "task_title": "Task Deep Gate",
                "reasoning_mode": "deep",
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
    let step_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    // 1) No hypotheses/decisions yet → blocked.
    let blocked_1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_deep_gate", "task": task_id.clone() } } }
    }));
    let blocked_1_text = extract_tool_text_str(&blocked_1);
    assert!(
        blocked_1_text.starts_with("ERROR:"),
        "expected deep gate error"
    );
    assert!(blocked_1_text.contains("REASONING_REQUIRED"));
    assert_step_scoped_think_card_actions_are_copy_paste_valid(&blocked_1_text);

    // Add a hypothesis + test (step-scoped).
    let _h1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_deep_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H1", "type": "hypothesis", "title": "H1", "text": "Main hypothesis" }
            } } }
    }));
    let _t1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_deep_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T1", "type": "test", "title": "T1", "text": "Minimal test stub" },
                "supports": ["H1"]
            } } }
    }));

    // 2) Strict-discipline superset still applies in deep mode (should require counter-edges).
    let blocked_2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_deep_gate", "task": task_id.clone() } } }
    }));
    let blocked_2_text = extract_tool_text_str(&blocked_2);
    assert!(
        blocked_2_text.starts_with("ERROR:"),
        "expected deep gate error"
    );
    assert!(blocked_2_text.contains("BM10_NO_COUNTER_EDGES"));
    assert_step_scoped_think_card_actions_are_copy_paste_valid(&blocked_2_text);

    // Add a counter-hypothesis + test (step-scoped).
    let _h2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_deep_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H2", "type": "hypothesis", "title": "H2", "text": "Counter", "tags": ["counter"] },
                "blocks": ["H1"]
            } } }
    }));
    let _t2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_deep_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T2", "type": "test", "title": "T2", "text": "Counter test stub" },
                "supports": ["H2"]
            } } }
    }));

    // 3) Deep mode adds synthesis requirement: must record a resolved decision before closing.
    let blocked_3 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_deep_gate", "task": task_id.clone() } } }
    }));
    let blocked_3_text = extract_tool_text_str(&blocked_3);
    assert!(
        blocked_3_text.starts_with("ERROR:"),
        "expected deep gate error"
    );
    assert!(blocked_3_text.contains("REASONING_REQUIRED"));
    assert!(
        blocked_3_text.contains("DEEP_NEEDS_RESOLVED_DECISION"),
        "expected deep gate to require a resolved decision"
    );
    assert_step_scoped_think_card_actions_are_copy_paste_valid(&blocked_3_text);

    // Add a resolved decision (step-scoped).
    let _d1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_deep_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "D1", "type": "decision", "title": "D1", "text": "Synthesis decision", "status": "resolved" }
            } } }
    }));

    let closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_deep_gate", "task": task_id } } }
    }));
    let closed_text = extract_tool_text_str(&closed);
    assert!(
        !closed_text.starts_with("ERROR:"),
        "expected deep gate to allow closing after strict discipline + resolved decision"
    );
}
