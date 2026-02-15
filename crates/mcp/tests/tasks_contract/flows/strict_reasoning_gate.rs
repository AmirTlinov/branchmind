#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

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
    assert!(
        msg.contains("tag `counter`"),
        "expected strict gate recovery hint to mention tagging counter-hypotheses with `counter` to avoid counter→counter regress"
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
        "expected strict gate to allow closing after hypothesis+test+counter are present; got: {closed_text}"
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
        closed_text.contains("WARNING: STRICT_OVERRIDE_APPLIED"),
        "expected macro to surface explicit strict override warning"
    );
}

#[test]
fn think_card_auto_tags_counter_for_counter_hypothesis_title_prefix() {
    let mut server = Server::start_initialized("think_auto_counter_tag");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_auto_counter_tag",
                "plan_title": "Plan Auto Counter Tag",
                "task_title": "Task Auto Counter Tag",
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

    // H1 + its supporting test.
    let _h1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_auto_counter_tag",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H1", "type": "hypothesis", "title": "H1", "text": "Main hypothesis" }
            } } }
    }));
    let _t1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_auto_counter_tag",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T1", "type": "test", "title": "T1", "text": "Minimal test stub" },
                "supports": ["H1"]
            } } }
    }));

    // Counter-hypothesis with conventional title prefix but without explicit tags.
    let h2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_auto_counter_tag",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H2", "type": "hypothesis", "title": "Counter-hypothesis: H1", "text": "Counter" },
                "blocks": ["H1"]
            } } }
    }));
    let h2_text = extract_tool_text(&h2);
    let warnings = h2_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings.iter().any(|w| {
            w.get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "COUNTER_TAG_AUTO_ADDED")
        }),
        "expected think.card to auto-add counter tag for Counter-hypothesis title prefix"
    );

    let _t2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_auto_counter_tag",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T2", "type": "test", "title": "T2", "text": "Counter test stub" },
                "supports": ["H2"]
            } } }
    }));

    let closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_auto_counter_tag", "task": task_id } } }
    }));
    let closed_text = extract_tool_text_str(&closed);
    assert!(
        !closed_text.starts_with("ERROR:"),
        "expected strict gate to allow closing after auto-tagged counter-hypothesis + test are present; got: {closed_text}"
    );
}

#[test]
fn think_macro_counter_hypothesis_stub_creates_counter_and_test() {
    let mut server = Server::start_initialized("think_macro_counter_stub");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_counter_stub",
                "plan_title": "Plan Counter Stub",
                "task_title": "Task Counter Stub",
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

    // H1 + its supporting test.
    let _h1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_counter_stub",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "H1", "type": "hypothesis", "title": "H1", "text": "Main hypothesis" }
            } } }
    }));
    let _t1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
                "workspace": "ws_counter_stub",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "card": { "id": "T1", "type": "test", "title": "T1", "text": "Minimal test stub" },
                "supports": ["H1"]
            } } }
    }));

    let counter_stub = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.macro.counter.hypothesis.stub", "args": {
                "workspace": "ws_counter_stub",
                "target": task_id.clone(),
                "step": step_id.clone(),
                "against": "H1",
                "label": "H1",
                "verbosity": "compact"
            } } }
    }));
    let counter_stub_text = extract_tool_text(&counter_stub);
    let counter_id = counter_stub_text
        .get("result")
        .and_then(|v| v.get("counter"))
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("counter card id");
    let test_id = counter_stub_text
        .get("result")
        .and_then(|v| v.get("test"))
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("test card id");
    assert!(
        !counter_id.trim().is_empty() && !test_id.trim().is_empty(),
        "expected macro to return both card ids"
    );

    let closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": { "workspace": "ws_counter_stub", "task": task_id } } }
    }));
    let closed_text = extract_tool_text_str(&closed);
    assert!(
        !closed_text.starts_with("ERROR:"),
        "expected strict gate to allow closing after counter stub macro was applied; got: {closed_text}"
    );
}

#[test]
fn strict_gate_requires_sequential_trace_for_explicit_gate_checkpoints() {
    let mut server = Server::start_initialized("strict_gate_requires_sequential_trace");

    let cmds = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": { "op": "cmd.list", "args": { "prefix": "think.", "limit": 200 } }
        }
    }));
    let cmds_text = extract_tool_text(&cmds);
    let cmd_list = cmds_text
        .get("result")
        .and_then(|v| v.get("cmds"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        cmd_list
            .iter()
            .any(|v| v.as_str() == Some("think.trace.sequential.step")),
        "golden cmd list must include think.trace.sequential.step; got: {cmds_text}"
    );

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_strict_seq_gate",
                "plan_title": "Plan Strict Seq Gate",
                "task_title": "Task Strict Seq Gate",
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

    for (idx, (id, ty, title, text, supports, blocks, tags)) in [
        (
            "H1",
            "hypothesis",
            "H1",
            "Main hypothesis",
            Vec::<&str>::new(),
            Vec::<&str>::new(),
            Vec::<&str>::new(),
        ),
        (
            "T1",
            "test",
            "T1",
            "Test for H1",
            vec!["H1"],
            Vec::<&str>::new(),
            Vec::<&str>::new(),
        ),
        (
            "H2",
            "hypothesis",
            "Counter-hypothesis: H1",
            "Counter",
            Vec::<&str>::new(),
            vec!["H1"],
            vec!["counter"],
        ),
        (
            "T2",
            "test",
            "T2",
            "Test for H2",
            vec!["H2"],
            Vec::<&str>::new(),
            Vec::<&str>::new(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut card = json!({
            "id": id,
            "type": ty,
            "title": title,
            "text": text
        });
        if !tags.is_empty() {
            card["tags"] = json!(tags);
        }
        let mut args = json!({
            "workspace": "ws_strict_seq_gate",
            "target": task_id.clone(),
            "step": step_id.clone(),
            "card": card
        });
        if !supports.is_empty() {
            args["supports"] = json!(supports);
        }
        if !blocks.is_empty() {
            args["blocks"] = json!(blocks);
        }
        let _ = server.request(json!({
            "jsonrpc":"2.0",
            "id": 10 + idx,
            "method":"tools/call",
            "params":{"name":"think","arguments":{"op":"call","cmd":"think.card","args":args}}
        }));
    }

    let blocked = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "workspace": "ws_strict_seq_gate",
                "task": task_id.clone(),
                "checkpoints": "gate"
            } } }
    }));
    let blocked_text = extract_tool_text_str(&blocked);
    assert!(
        blocked_text.starts_with("ERROR:") && blocked_text.contains("REASONING_REQUIRED"),
        "explicit gate checkpoints must require sequential trace: {blocked_text}"
    );
    assert!(
        blocked_text.contains("sequential trace"),
        "error should mention sequential trace gate: {blocked_text}"
    );

    for (num, need_next) in [(1, true), (2, false)] {
        let _ = server.request(json!({
            "jsonrpc":"2.0",
            "id": 30 + num,
            "method":"tools/call",
            "params":{"name":"think","arguments":{"op":"call","cmd":"think.trace.sequential.step","args":{
                "workspace":"ws_strict_seq_gate",
                "target":task_id.clone(),
                "thought":"Checkpoint: hypothesis/test/counter status.",
                "thoughtNumber":num,
                "totalThoughts":2,
                "nextThoughtNeeded":need_next,
                "meta":{"step_id":step_id.clone(),"checkpoint":"gate"}
            }}}
        }));
    }

    let closed = server.request(json!({
        "jsonrpc": "2.0",
        "id": 40,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.close.step", "args": {
                "workspace": "ws_strict_seq_gate",
                "task": task_id,
                "checkpoints": "gate"
            } } }
    }));
    let closed_text = extract_tool_text_str(&closed);
    assert!(
        !closed_text.starts_with("ERROR:"),
        "close must pass after sequential checkpoints are recorded: {closed_text}"
    );
}
