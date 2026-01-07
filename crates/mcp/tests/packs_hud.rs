#![forbid(unsafe_code)]

mod support;

use serde_json::{Value, json};
use support::*;

#[test]
fn think_pack_includes_capsule_and_engine() {
    let mut server = Server::start_initialized("think_pack_includes_capsule_and_engine");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_pack_hud" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_pack_hud",
            "card": { "id": "H1", "type": "hypothesis", "title": "Hypothesis", "text": "needs a test" }
        } }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_pack_hud",
            "card": { "id": "T1", "type": "test", "title": "Runnable test", "text": "CMD: echo hi", "meta": { "run": { "cmd": "echo hi" } } },
            "supports": ["H1"]
        } }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_pack", "arguments": { "workspace": "ws_pack_hud", "limit_candidates": 15, "limit_tests": 10 } }
    }));
    let pack_text = extract_tool_text(&pack);

    let capsule_type = pack_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(capsule_type, "think_pack_capsule");

    let engine_version = pack_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(engine_version, "v0.5");

    let trace_doc = pack_text
        .get("result")
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !trace_doc.is_empty(),
        "think_pack should expose trace_doc for unambiguous suggested writes"
    );

    let capsule_trace_doc = pack_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("docs"))
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        capsule_trace_doc, trace_doc,
        "capsule.where.docs.trace must match result.trace_doc"
    );

    let primary = pack_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("next"))
        .and_then(|v| v.get("primary"))
        .cloned()
        .unwrap_or(Value::Null);
    assert!(!primary.is_null(), "capsule.next.primary must be present");
}

#[test]
fn context_pack_includes_capsule_engine_and_sequential_trace() {
    let mut server =
        Server::start_initialized("context_pack_includes_capsule_engine_and_sequential_trace");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_pack_ctx",
                "plan_title": "Plan",
                "task_title": "Task",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_pack_ctx",
                "target": task_id.clone(),
                "card": {
                    "id": "T1",
                    "type": "test",
                    "title": "Runnable test",
                    "text": "CMD: echo hi",
                    "meta": { "run": { "cmd": "echo hi" } }
                }
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "trace_sequential_step",
            "arguments": {
                "workspace": "ws_pack_ctx",
                "target": task_id.clone(),
                "thought": "Thought 1",
                "thoughtNumber": 1,
                "totalThoughts": 2,
                "nextThoughtNeeded": true
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "trace_sequential_step",
            "arguments": {
                "workspace": "ws_pack_ctx",
                "target": task_id.clone(),
                "thought": "Thought 2 (branch)",
                "thoughtNumber": 2,
                "totalThoughts": 2,
                "nextThoughtNeeded": false,
                "branchFromThought": 1,
                "branchId": "alt-1"
            }
        }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "context_pack", "arguments": { "workspace": "ws_pack_ctx", "target": task_id, "trace_limit": 20, "limit_cards": 20, "max_chars": 8000 } }
    }));
    let pack_text = extract_tool_text(&pack);

    let capsule_type = pack_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(capsule_type, "context_pack_capsule");

    let engine_version = pack_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(engine_version, "v0.5");

    let sequential = pack_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("sequential"))
        .expect("trace.sequential");
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
        "context_pack must include derived sequential branch edge (1 -> 2)"
    );
}

#[test]
fn context_pack_step_focus_filters_graph_cards_to_step_scope() {
    let mut server =
        Server::start_initialized("context_pack_step_focus_filters_graph_cards_to_step_scope");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_pack_step",
                "plan_title": "Plan",
                "task_title": "Task",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_pack_step",
            "target": task_id.clone(),
            "card": { "id": "GLOBAL", "type": "hypothesis", "title": "Global", "text": "not step scoped" }
        } }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_pack_step",
            "target": task_id.clone(),
            "step": "focus",
            "card": { "id": "STEP", "type": "hypothesis", "title": "Step", "text": "step scoped" }
        } }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "context_pack", "arguments": { "workspace": "ws_pack_step", "target": task_id, "step": "focus", "limit_cards": 50, "trace_limit": 0, "notes_limit": 0, "max_chars": 8000 } }
    }));
    let pack_text = extract_tool_text(&pack);

    let cards = pack_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("STEP")),
        "step-scoped card must be included"
    );
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("GLOBAL")),
        "non step-scoped cards must be excluded under step focus"
    );
}

#[test]
fn context_pack_step_focus_filters_notes_and_trace_to_step_scope() {
    let mut server =
        Server::start_initialized("context_pack_step_focus_filters_notes_and_trace_to_step_scope");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_pack_step_docs",
                "plan_title": "Plan",
                "task_title": "Task",
                "steps": [
                    { "title": "S0", "success_criteria": ["c0"], "tests": ["t0"] },
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    // Create a step-focused decision note (older).
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_pipeline",
            "arguments": {
                "workspace": "ws_pack_step_docs",
                "target": task_id.clone(),
                "step": "focus",
                "decision": "Focus decision"
            }
        }
    }));

    // Create a non-focus step decision note (newer, must be filtered out under step focus).
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_pipeline",
            "arguments": {
                "workspace": "ws_pack_step_docs",
                "target": task_id.clone(),
                "step": "s:1",
                "decision": "Other decision"
            }
        }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "context_pack",
            "arguments": {
                "workspace": "ws_pack_step_docs",
                "target": task_id.clone(),
                "step": "focus",
                "notes_limit": 50,
                "trace_limit": 50,
                "limit_cards": 0,
                "max_chars": 8000
            }
        }
    }));
    let pack_text = extract_tool_text(&pack);

    let notes = pack_text
        .get("result")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("notes.entries");
    assert!(!notes.is_empty(), "step-scoped notes must not be empty");
    assert!(
        notes.iter().all(|n| {
            let meta = n.get("meta").unwrap_or(&Value::Null);
            meta.get("step")
                .and_then(|v| v.get("task_id"))
                .and_then(|v| v.as_str())
                == Some(task_id.as_str())
                && meta
                    .get("step")
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str())
                    == Some("s:0")
        }),
        "step focus notes must be stamped to the focused step"
    );
    assert!(
        notes.iter().all(|n| {
            !n.get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("Other decision")
        }),
        "non-focus step decision note must be filtered out under step focus"
    );

    let trace = pack_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("trace.entries");
    assert!(
        trace.iter().all(|e| {
            match e.get("kind").and_then(|v| v.as_str()).unwrap_or("") {
                "event" => {
                    e.get("task_id").and_then(|v| v.as_str()) == Some(task_id.as_str())
                        && e.get("path").and_then(|v| v.as_str()) == Some("s:0")
                }
                "note" => {
                    let meta = e.get("meta").unwrap_or(&Value::Null);
                    meta.get("step")
                        .and_then(|v| v.get("task_id"))
                        .and_then(|v| v.as_str())
                        == Some(task_id.as_str())
                        && meta
                            .get("step")
                            .and_then(|v| v.get("path"))
                            .and_then(|v| v.as_str())
                            == Some("s:0")
                }
                _ => false,
            }
        }),
        "step focus trace must only include entries scoped to the focused step"
    );
    assert!(
        trace.iter().all(|e| {
            !e.get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("Other decision")
        }),
        "non-focus trace notes must be filtered out under step focus"
    );
}
