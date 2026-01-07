#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

use std::thread::sleep;
use std::time::Duration;

#[test]
fn reasoning_engine_think_watch_bm4_blind_spot_emits_action() {
    let mut server = Server::start_initialized("reasoning_engine_think_watch_bm4");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_engine" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_add_hypothesis", "arguments": { "workspace": "ws_re_engine", "card": { "title": "Hypo", "text": "No tests yet" } } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_engine", "engine_signals_limit": 10, "engine_actions_limit": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);

    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals
            .iter()
            .any(|s| { s.get("code").and_then(|v| v.as_str()) == Some("BM4_HYPOTHESIS_NO_TEST") }),
        "expected BM4_HYPOTHESIS_NO_TEST signal"
    );

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    assert!(
        actions
            .iter()
            .any(|a| { a.get("kind").and_then(|v| v.as_str()) == Some("add_test_stub") }),
        "expected add_test_stub action"
    );
}

#[test]
fn reasoning_engine_lane_decision_suggests_publish_to_shared() {
    let mut server = Server::start_initialized("reasoning_engine_lane_decision_publish");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_lane" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_lane",
                "agent_id": "alice",
                "card": {
                    "id": "D1",
                    "type": "decision",
                    "title": "Decision in lane",
                    "text": "We should publish this."
                }
            }
        }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_watch",
            "arguments": {
                "workspace": "ws_re_lane",
                "agent_id": "alice",
                "engine_signals_limit": 20,
                "engine_actions_limit": 20
            }
        }
    }));
    let watch_text = extract_tool_text(&watch);
    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals.iter().any(|s| {
            s.get("code").and_then(|v| v.as_str()) == Some("BM_LANE_DECISION_NOT_PUBLISHED")
        }),
        "expected lane decision unpublished signal"
    );

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    let publish_action = actions
        .iter()
        .find(|a| a.get("kind").and_then(|v| v.as_str()) == Some("publish_decision"));
    let publish_action = publish_action.expect("publish_decision action");
    let calls = publish_action
        .get("calls")
        .and_then(|v| v.as_array())
        .expect("calls");
    assert!(
        calls
            .iter()
            .any(|c| c.get("target").and_then(|v| v.as_str()) == Some("think_publish")),
        "publish_decision must suggest think_publish"
    );
}

#[test]
fn reasoning_engine_tasks_resume_super_bm5_runnable_test_suggests_capture() {
    let mut server = Server::start_initialized("reasoning_engine_tasks_resume_super_bm5");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_re_tasks",
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

    let _test_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_tasks",
                "target": task_id.clone(),
                "card": {
                    "type": "test",
                    "title": "Runnable test",
                    "text": "Simple check",
                    "meta": { "run": { "cmd": "echo hi" } }
                }
            }
        }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": { "workspace": "ws_re_tasks", "task": task_id, "read_only": true, "max_chars": 8000, "engine_actions_limit": 10, "engine_signals_limit": 10 }
        }
    }));
    let resume_text = extract_tool_text(&resume);
    let engine = resume_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    assert!(
        actions
            .iter()
            .any(|a| a.get("kind").and_then(|v| v.as_str()) == Some("run_test")),
        "expected run_test action"
    );
}

#[test]
fn reasoning_engine_think_watch_bm1_contradiction_emits_signal_and_action() {
    let mut server = Server::start_initialized("reasoning_engine_think_watch_bm1");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm1" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm1",
                "card": { "id": "H1", "type": "hypothesis", "title": "Hypo", "text": "Conflicting evidence" }
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm1",
                "card": { "id": "E1", "type": "evidence", "title": "E1", "text": "Supports H1" },
                "supports": ["H1"]
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm1",
                "card": { "id": "E2", "type": "evidence", "title": "E2", "text": "Blocks H1" },
                "blocks": ["H1"]
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_re_bm1", "targets": ["E1", "E2"], "pinned": true } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm1", "limit_candidates": 10, "engine_signals_limit": 20, "engine_actions_limit": 20 } }
    }));
    let watch_text = extract_tool_text(&watch);

    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals.iter().any(|s| {
            s.get("code").and_then(|v| v.as_str()) == Some("BM1_CONTRADICTION_SUPPORTS_BLOCKS")
        }),
        "expected BM1 contradiction signal"
    );

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    let resolve = actions
        .iter()
        .find(|a| a.get("kind").and_then(|v| v.as_str()) == Some("resolve_contradiction"));
    let resolve = resolve.expect("expected resolve_contradiction action");

    let calls = resolve
        .get("calls")
        .and_then(|v| v.as_array())
        .expect("resolve_contradiction.calls");
    assert!(
        calls.iter().any(|c| {
            c.get("target").and_then(|v| v.as_str()) == Some("think_playbook")
                && c.get("params")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    == Some("contradiction")
        }),
        "resolve_contradiction should suggest the contradiction playbook"
    );
}

#[test]
fn reasoning_engine_think_watch_bm8_stale_evidence_prompts_rerun() {
    let mut server = Server::start_initialized("reasoning_engine_think_watch_bm8");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm8" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm8",
                "card": {
                    "id": "T1",
                    "type": "test",
                    "title": "Runnable test",
                    "text": "CMD: echo hi",
                    "meta": { "run": { "cmd": "echo hi", "stale_after_ms": 0 } }
                }
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm8",
                "card": { "id": "EV1", "type": "evidence", "title": "Evidence", "text": "Old output" },
                "supports": ["T1"]
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_re_bm8", "targets": ["EV1"], "pinned": true } }
    }));

    sleep(Duration::from_millis(10));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "trace_step", "arguments": { "workspace": "ws_re_bm8", "step": "bump clock" } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm8", "limit_candidates": 10, "engine_signals_limit": 20, "engine_actions_limit": 20 } }
    }));
    let watch_text = extract_tool_text(&watch);

    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals
            .iter()
            .any(|s| { s.get("code").and_then(|v| v.as_str()) == Some("BM8_EVIDENCE_STALE") }),
        "expected BM8_EVIDENCE_STALE signal"
    );

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    assert!(
        actions
            .iter()
            .any(|a| a.get("kind").and_then(|v| v.as_str()) == Some("run_test")),
        "expected run_test action"
    );
}

#[test]
fn reasoning_engine_bm2_weak_evidence_emits_warning() {
    let mut server = Server::start_initialized("reasoning_engine_bm2_weak_evidence");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm2" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm2",
                "card": { "id": "EVW", "type": "evidence", "title": "Weak evidence", "text": "just a claim (no receipts)" }
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_re_bm2", "targets": ["EVW"], "pinned": true } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm2", "engine_signals_limit": 20, "engine_actions_limit": 20, "limit_candidates": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);
    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals
            .iter()
            .any(|s| s.get("code").and_then(|v| v.as_str()) == Some("BM2_EVIDENCE_WEAK")),
        "expected BM2_EVIDENCE_WEAK signal"
    );
}

#[test]
fn reasoning_engine_bm3_low_confidence_pinned_decision_emits_warning() {
    let mut server = Server::start_initialized("reasoning_engine_bm3_low_confidence");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm3" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm3",
                "card": { "id": "DLOW", "type": "decision", "title": "Pinned decision", "text": "no evidence yet" }
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_re_bm3", "targets": ["DLOW"], "pinned": true } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm3", "engine_signals_limit": 20, "engine_actions_limit": 20, "limit_candidates": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);
    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals.iter().any(|s| {
            s.get("code").and_then(|v| v.as_str()) == Some("BM3_DECISION_LOW_CONFIDENCE")
        }),
        "expected BM3_DECISION_LOW_CONFIDENCE signal"
    );

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    assert!(
        actions.iter().any(|a| {
            a.get("kind").and_then(|v| v.as_str()) == Some("use_playbook")
                && a.get("calls")
                    .and_then(|v| v.as_array())
                    .is_some_and(|calls| {
                        calls.iter().any(|c| {
                            c.get("target").and_then(|v| v.as_str()) == Some("think_playbook")
                                && c.get("params")
                                    .and_then(|v| v.get("name"))
                                    .and_then(|v| v.as_str())
                                    == Some("experiment")
                        })
                    })
        }),
        "low-confidence decision should suggest an experiment playbook"
    );
}

#[test]
fn reasoning_engine_bm9_tradeoff_suggests_criteria_matrix_playbook() {
    let mut server = Server::start_initialized("reasoning_engine_bm9_tradeoff_criteria_matrix");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm9" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm9",
                "card": { "id": "Q1", "type": "question", "title": "A vs B: pick approach", "text": "Tradeoff between correctness vs speed." }
            }
        }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm9", "engine_signals_limit": 20, "engine_actions_limit": 20, "limit_candidates": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);
    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    assert!(
        actions.iter().any(|a| {
            a.get("kind").and_then(|v| v.as_str()) == Some("use_playbook")
                && a.get("calls")
                    .and_then(|v| v.as_array())
                    .is_some_and(|calls| {
                        calls.iter().any(|c| {
                            c.get("target").and_then(|v| v.as_str()) == Some("think_playbook")
                                && c.get("params")
                                    .and_then(|v| v.get("name"))
                                    .and_then(|v| v.as_str())
                                    == Some("criteria_matrix")
                        })
                    })
        }),
        "tradeoff framing should suggest criteria_matrix playbook"
    );
}

#[test]
fn reasoning_engine_bm6_assumption_not_open_but_used_emits_action() {
    let mut server = Server::start_initialized("reasoning_engine_bm6_assumption_cascade");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm6" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm6",
                "card": { "id": "D1", "type": "decision", "title": "Decision depends on A1", "text": "anchor" }
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm6",
                "card": { "id": "A1", "type": "note", "title": "Assumption: network is blocked", "text": "assumption", "tags": ["assumption"] },
                "supports": ["D1"]
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_re_bm6", "targets": ["A1", "D1"], "pinned": true } }
    }));

    // Assumption is no longer open.
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think_set_status", "arguments": { "workspace": "ws_re_bm6", "targets": ["A1"], "status": "rejected" } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm6", "engine_signals_limit": 20, "engine_actions_limit": 20, "limit_candidates": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);
    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let signals = engine
        .get("signals")
        .and_then(|v| v.as_array())
        .expect("engine.signals");
    assert!(
        signals.iter().any(|s| {
            s.get("code").and_then(|v| v.as_str()) == Some("BM6_ASSUMPTION_NOT_OPEN_BUT_USED")
        }),
        "expected BM6 assumption cascade signal"
    );

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    assert!(
        actions
            .iter()
            .any(|a| a.get("kind").and_then(|v| v.as_str()) == Some("recheck_assumption")),
        "expected recheck_assumption action"
    );
}

#[test]
fn reasoning_engine_bm7_counter_hypothesis_is_suggested() {
    let mut server = Server::start_initialized("reasoning_engine_bm7_counter_hypothesis");

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_re_bm7" } }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm7",
                "card": { "id": "H1", "type": "hypothesis", "title": "Hypothesis", "text": "one-sided support" }
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_re_bm7",
                "card": { "id": "E1", "type": "evidence", "title": "Evidence", "text": "Supports H1", "meta": { "run": { "cmd": "echo hi", "url": "https://example.com" } } },
                "supports": ["H1"]
            }
        }
    }));
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_re_bm7", "targets": ["H1", "E1"], "pinned": true } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think_watch", "arguments": { "workspace": "ws_re_bm7", "engine_signals_limit": 20, "engine_actions_limit": 20, "limit_candidates": 10 } }
    }));
    let watch_text = extract_tool_text(&watch);
    let engine = watch_text
        .get("result")
        .and_then(|v| v.get("engine"))
        .expect("engine");

    let actions = engine
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("engine.actions");
    let counter = actions
        .iter()
        .find(|a| a.get("kind").and_then(|v| v.as_str()) == Some("add_counter_hypothesis"));
    let counter = counter.expect("expected add_counter_hypothesis action");

    let calls = counter
        .get("calls")
        .and_then(|v| v.as_array())
        .expect("calls");
    assert!(
        calls
            .iter()
            .any(|c| c.get("target").and_then(|v| v.as_str()) == Some("think_card")),
        "counter action must suggest think_card"
    );
}
