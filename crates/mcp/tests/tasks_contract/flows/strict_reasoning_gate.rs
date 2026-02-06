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
fn strict_reasoning_mode_blocks_step_close_until_disciplined() {
    let mut server = Server::start_initialized("tasks_strict_reasoning_gate");

    // Regression: strict gate must not be bypassable by status drift on hypotheses/decisions.
    // (e.g., an agent marking a hypothesis as "accepted" without linking a test.)
    let bootstrap_status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_strict_gate_status",
                "plan_title": "Plan Strict Gate Status",
                "task_title": "Task Strict Gate Status",
                "reasoning_mode": "strict",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            } } }
    }));
    let bootstrap_status_text = extract_tool_text(&bootstrap_status);
    let task_status_id = bootstrap_status_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();
    let step_status_id = bootstrap_status_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_strict_gate_status",
                "target": task_status_id.clone(),
                "step": step_status_id.clone(),
                "card": { "id": "H_ACCEPTED", "type": "hypothesis", "title": "H accepted", "text": "status drift", "status": "accepted" }
            } } }
    }));
    let blocked_status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_strict_gate_status", "task": task_status_id } } }
    }));
    let blocked_status_text = extract_tool_text_str(&blocked_status);
    assert!(
        blocked_status_text.starts_with("ERROR:"),
        "expected strict gate error"
    );
    assert!(blocked_status_text.contains("REASONING_REQUIRED"));
    assert!(
        blocked_status_text.contains("BM4_HYPOTHESIS_NO_TEST"),
        "expected strict gate to treat status-drift hypotheses as active and require tests"
    );
    assert_step_scoped_think_card_actions_are_copy_paste_valid(&blocked_status_text);

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_strict_gate",
                "plan_title": "Plan Strict Gate",
                "task_title": "Task Strict Gate",
                "reasoning_mode": "strict",
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

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws_strict_gate", "task": task_id.clone(), "max_chars": 2000 } } }
    }));
    let radar_text = extract_tool_text(&radar);
    assert_eq!(
        radar_text
            .get("result")
            .and_then(|v| v.get("target"))
            .and_then(|v| v.get("reasoning_mode"))
            .and_then(|v| v.as_str()),
        Some("strict"),
        "expected radar to surface task.reasoning_mode"
    );

    let blocked_1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_strict_gate", "task": task_id.clone() } } }
    }));
    let blocked_1_text = extract_tool_text_str(&blocked_1);
    assert!(
        blocked_1_text.starts_with("ERROR:"),
        "expected strict gate error"
    );
    assert!(
        blocked_1_text.contains("REASONING_REQUIRED"),
        "expected typed REASONING_REQUIRED"
    );

    let blocked_done = server.request(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.done", "args": {
                "workspace": "ws_strict_gate",
                "task": task_id.clone(),
                "step_id": step_id.clone()
            } } }
    }));
    let blocked_done_text = extract_tool_text(&blocked_done);
    let blocked_done_code = blocked_done_text
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str());
    assert_eq!(
        blocked_done_code,
        Some("REASONING_REQUIRED"),
        "tasks_done should enforce strict reasoning gate"
    );

    let blocked_close_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.step.close", "args": {
                "workspace": "ws_strict_gate",
                "task": task_id.clone(),
                "step_id": step_id.clone(),
                "checkpoints": "gate"
            } } }
    }));
    let blocked_close_step_text = extract_tool_text(&blocked_close_step);
    let blocked_close_step_code = blocked_close_step_text
        .get("error")
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str());
    assert_eq!(
        blocked_close_step_code,
        Some("REASONING_REQUIRED"),
        "tasks_close_step should enforce strict reasoning gate"
    );

    // Add a hypothesis (step-scoped).
    let _h1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_strict_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H1", "type": "hypothesis", "title": "H1", "text": "Main hypothesis" }
            } } }
    }));

    let blocked_2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_strict_gate", "task": task_id.clone() } } }
    }));
    let blocked_2_text = extract_tool_text_str(&blocked_2);
    assert!(
        blocked_2_text.starts_with("ERROR:"),
        "expected strict gate error"
    );
    assert!(blocked_2_text.contains("REASONING_REQUIRED"));

    // Add a test that supports the hypothesis (step-scoped).
    let _t1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_strict_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T1", "type": "test", "title": "T1", "text": "Minimal test stub" },
                "supports": ["H1"]
            } } }
    }));

    let blocked_3 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_strict_gate", "task": task_id.clone() } } }
    }));
    let blocked_3_text = extract_tool_text_str(&blocked_3);
    assert!(
        blocked_3_text.starts_with("ERROR:"),
        "expected strict gate error"
    );
    assert!(blocked_3_text.contains("REASONING_REQUIRED"));
    let msg = blocked_3_text.as_str();
    assert!(
        msg.contains("BM10_NO_COUNTER_EDGES"),
        "expected strict gate to require a counter-position after supporting evidence exists"
    );

    // Add a counter-hypothesis (explicitly tagged counter, step-scoped) and its test stub.
    let _h2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_strict_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H2", "type": "hypothesis", "title": "H2", "text": "Counter", "tags": ["counter"] },
                "blocks": ["H1"]
            } } }
    }));
    let _t2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_strict_gate",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T2", "type": "test", "title": "T2", "text": "Counter test stub" },
                "supports": ["H2"]
            } } }
    }));

    let closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_strict_gate", "task": task_id } } }
    }));
    let closed_text = extract_tool_text_str(&closed);
    assert!(
        !closed_text.starts_with("ERROR:"),
        "expected strict gate to allow closing after hypothesis+test+counter are present"
    );
}

#[test]
fn strict_reasoning_override_allows_closing_with_reason_and_risk() {
    let mut server = Server::start_initialized("tasks_strict_reasoning_override");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_strict_override",
                "plan_title": "Plan Strict Override",
                "task_title": "Task Strict Override",
                "reasoning_mode": "strict",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
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

    let closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "workspace": "ws_strict_override",
                "task": task_id,
                "override": { "reason": "progress over purity", "risk": "may hide missing test/counter" }
            } } }
    }));
    let closed_text = extract_tool_text_str(&closed);
    assert!(
        !closed_text.starts_with("ERROR:"),
        "expected strict override to allow closing"
    );
    assert!(
        closed_text.contains("WARNING: REASONING_OVERRIDE_APPLIED"),
        "expected macro to surface explicit strict override warning"
    );
}
