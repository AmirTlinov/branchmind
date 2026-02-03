#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_trace_smoke() {
    let mut server = Server::start_initialized("branchmind_trace_smoke");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_trace" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_trace", "kind": "plan", "title": "Trace Plan" } } }
    }));
    let created_plan_text = extract_tool_text(&created_plan);
    let plan_id = created_plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let created_task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_trace", "kind": "task", "parent": plan_id, "title": "Trace Task" } } }
    }));
    let created_task_text = extract_tool_text(&created_task);
    let task_id = created_task_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws_trace", "task": task_id.clone() } } }
    }));
    let radar_text = extract_tool_text(&radar);
    let target_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let target_trace_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.trace_doc")
        .to_string();

    let target_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.step", "args": { "workspace": "ws_trace", "target": task_id, "step": "Target step" } } }
    }));
    let target_step_text = extract_tool_text(&target_step);
    assert_eq!(
        target_step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        target_step_text
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.as_str()),
        Some(target_branch.as_str())
    );
    assert_eq!(
        target_step_text
            .get("result")
            .and_then(|v| v.get("doc"))
            .and_then(|v| v.as_str()),
        Some(target_trace_doc.as_str())
    );

    let step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.step", "args": { "workspace": "ws_trace", "step": "Step 1", "message": "m1" } } }
    }));
    let step_text = extract_tool_text(&step);
    let seq1 = step_text
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("trace step seq");

    let seq_step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.sequential.step", "args": {
                "workspace": "ws_trace",
                "thought": "Thought 1",
                "thoughtNumber": 1,
                "totalThoughts": 2,
                "nextThoughtNeeded": true
            } } }
    }));
    let seq_text = extract_tool_text(&seq_step);
    let seq2 = seq_text
        .get("result")
        .and_then(|v| v.get("entry"))
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("trace sequential seq");
    assert!(seq2 > seq1, "sequential step must advance seq");

    let seq_step_2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 41,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.sequential.step", "args": {
                "workspace": "ws_trace",
                "thought": "Thought 2 (branch)",
                "thoughtNumber": 2,
                "totalThoughts": 2,
                "nextThoughtNeeded": false,
                "branchFromThought": 1,
                "branchId": "alt-1"
            } } }
    }));
    let seq2_text = extract_tool_text(&seq_step_2);
    assert_eq!(
        seq2_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let hydrate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.hydrate", "args": { "workspace": "ws_trace", "limit_steps": 10 } } }
    }));
    let hydrate_text = extract_tool_text(&hydrate);
    let entries = hydrate_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("trace entries");
    assert!(entries.len() >= 2, "trace hydrate must return entries");

    let sequential = hydrate_text
        .get("result")
        .and_then(|v| v.get("sequential"))
        .expect("result.sequential");
    let edges = sequential
        .get("edges")
        .and_then(|v| v.as_array())
        .expect("sequential.edges");
    assert!(
        edges.iter().any(|e| {
            e.get("rel").and_then(|v| v.as_str()) == Some("branch")
                && e.get("from").and_then(|v| v.as_i64()) == Some(1)
                && e.get("to").and_then(|v| v.as_i64()) == Some(2)
        }),
        "sequential graph must include branch edge (1 -> 2)"
    );

    let validate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.validate", "args": { "workspace": "ws_trace" } } }
    }));
    let validate_text = extract_tool_text(&validate);
    let ok = validate_text
        .get("result")
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .expect("trace validate ok");
    assert!(ok, "trace validate must be ok");
}

#[test]
fn branchmind_trace_step_meta_can_build_sequential_graph() {
    let mut server = Server::start_initialized("branchmind_trace_step_meta_graph");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_trace_meta" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let t1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.step", "args": {
                "workspace": "ws_trace_meta",
                "step": "Thought 1 (via trace_step)",
                "meta": { "thoughtNumber": 1 }
            } } }
    }));
    let t1_text = extract_tool_text(&t1);
    assert_eq!(t1_text.get("success").and_then(|v| v.as_bool()), Some(true));

    let t2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.step", "args": {
                "workspace": "ws_trace_meta",
                "step": "Thought 2 (branch)",
                "meta": { "thoughtNumber": 2, "branchFromThought": 1, "branchId": "alt-1" }
            } } }
    }));
    let t2_text = extract_tool_text(&t2);
    assert_eq!(t2_text.get("success").and_then(|v| v.as_bool()), Some(true));

    let hydrate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.hydrate", "args": { "workspace": "ws_trace_meta", "limit_steps": 10 } } }
    }));
    let hydrate_text = extract_tool_text(&hydrate);
    let sequential = hydrate_text
        .get("result")
        .and_then(|v| v.get("sequential"))
        .expect("result.sequential");
    let edges = sequential
        .get("edges")
        .and_then(|v| v.as_array())
        .expect("sequential.edges");
    assert!(
        edges.iter().any(|e| {
            e.get("rel").and_then(|v| v.as_str()) == Some("branch")
                && e.get("from").and_then(|v| v.as_i64()) == Some(1)
                && e.get("to").and_then(|v| v.as_i64()) == Some(2)
        }),
        "sequential graph must include branch edge from trace_step meta (1 -> 2)"
    );
}

#[test]
fn trace_step_warns_when_sequential_meta_is_incomplete() {
    let mut server = Server::start_initialized("trace_step_sequential_meta_warnings");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_trace_warn" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.step", "args": {
                "workspace": "ws_trace_warn",
                "step": "Branch without thought number",
                "meta": { "branchFromThought": 1, "branchId": "alt-1" }
            } } }
    }));
    let step_text = extract_tool_text(&step);
    assert_eq!(
        step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let warnings = step_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings.iter().any(|w| {
            w.get("code").and_then(|v| v.as_str()) == Some("TRACE_SEQ_META_MISSING_THOUGHT_NUMBER")
        }),
        "trace_step should warn when sequential meta is present but missing thoughtNumber"
    );
}

#[test]
fn trace_step_sequential_meta_warnings_are_capped_to_stay_low_noise() {
    let mut server = Server::start_initialized("trace_step_sequential_meta_warnings_capped");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_trace_warn_cap" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    // Intentionally messy sequential-like meta to trigger multiple possible warnings.
    // The tool must keep it low-noise by capping warnings.
    let step = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.trace.step", "args": {
                "workspace": "ws_trace_warn_cap",
                "step": "Bad sequential meta",
                "meta": {
                    "branchFromThought": 0,
                    "branchId": "",
                    "isRevision": true,
                    "revisesThought": 0,
                    "totalThoughts": 0,
                    "nextThoughtNeeded": "nope"
                }
            } } }
    }));
    let step_text = extract_tool_text(&step);
    assert_eq!(
        step_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let warnings = step_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !warnings.is_empty(),
        "trace_step should emit at least one warning for malformed sequential-like meta"
    );
    assert!(
        warnings.len() <= 2,
        "trace_step sequential-meta lint must stay low-noise (<=2 warnings), got {}",
        warnings.len()
    );
}
