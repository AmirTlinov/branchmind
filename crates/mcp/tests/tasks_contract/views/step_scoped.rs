#![forbid(unsafe_code)]

use super::super::support::*;

use serde_json::json;

#[test]
fn tasks_resume_super_smart_includes_step_scoped_cards() {
    let mut server = Server::start_initialized("tasks_resume_super_step_scoped_cards");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_scoped",
                "plan_title": "Plan Step Scoped",
                "task_title": "Task Step Scoped",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
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

    let initial = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": {
                "workspace": "ws_step_scoped",
                "task": task_id.clone(),
                "view": "smart",
                "max_chars": 8000
            }
        }
    }));
    let initial_text = extract_tool_text(&initial);
    let step_id = initial_text
        .get("result")
        .and_then(|v| v.get("step_focus"))
        .and_then(|v| v.get("step"))
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step_focus.step.step_id")
        .to_string();

    let _step_note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_scoped",
                "target": task_id.clone(),
                "step": step_id,
                "card": {
                    "id": "CARD-STEP-NOTE",
                    "type": "note",
                    "title": "Step scoped note",
                    "text": "This note should surface in smart view via step-scoping"
                }
            }
        }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": {
                "workspace": "ws_step_scoped",
                "task": task_id,
                "view": "smart",
                "cards_limit": 1,
                "max_chars": 8000
            }
        }
    }));
    let resume_text = extract_tool_text(&resume);
    let result = resume_text.get("result").expect("result");

    let first_id = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        first_id, "CARD-STEP-NOTE",
        "smart view should prioritize step-scoped cards when a first open step exists"
    );
}

#[test]
fn think_card_step_focus_resolves_first_open_step() {
    let mut server = Server::start_initialized("think_card_step_focus_first_open");

    let bootstrap = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_focus",
                "plan_title": "Plan Step Focus",
                "task_title": "Task Step Focus",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            }
        }
    } ));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let _card = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_focus",
                "target": task_id.clone(),
                "step": "focus",
                "card": {
                    "id": "CARD-FOCUS-NOTE",
                    "type": "note",
                    "title": "Focus step note",
                    "text": "This note should be attached to the first open step via step=focus"
                }
            }
        }
    } ));

    let resume = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "tasks_resume_super",
            "arguments": {
                "workspace": "ws_step_focus",
                "task": task_id,
                "view": "smart",
                "cards_limit": 1,
                "max_chars": 8000
            }
        }
    } ));
    let resume_text = extract_tool_text(&resume);
    let result = resume_text.get("result").expect("result");

    let first_id = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(first_id, "CARD-FOCUS-NOTE");
}

#[test]
fn think_query_step_focus_filters_to_step_scoped_cards() {
    let mut server = Server::start_initialized("think_query_step_focus_filters");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_query",
                "plan_title": "Plan Step Query",
                "task_title": "Task Step Query",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            }
        }
    }));
    let task_id = extract_tool_text(&bootstrap)
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_query",
                "target": task_id.clone(),
                "step": "focus",
                "card": {
                    "id": "CARD-STEP-ONLY",
                    "type": "note",
                    "title": "Step only",
                    "text": "This card is step-scoped via step=focus"
                }
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_query",
                "target": task_id.clone(),
                "card": {
                    "id": "CARD-GLOBAL-ONLY",
                    "type": "note",
                    "title": "Global only",
                    "text": "This card is NOT step-scoped"
                }
            }
        }
    }));

    let query = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": {
            "name": "think_query",
            "arguments": {
                "workspace": "ws_step_query",
                "target": task_id,
                "types": "note",
                "step": "focus",
                "limit": 50,
                "max_chars": 8000
            }
        }
    }));
    let query_text = extract_tool_text(&query);
    let cards = query_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut ids = std::collections::BTreeSet::<String>::new();
    for card in cards {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            ids.insert(id.to_string());
        }
    }

    assert!(
        ids.contains("CARD-STEP-ONLY"),
        "step-scoped card must be visible under step=focus query"
    );
    assert!(
        !ids.contains("CARD-GLOBAL-ONLY"),
        "non-step-scoped card must be filtered out under step=focus query"
    );
}

#[test]
fn think_watch_step_focus_filters_frontier_and_candidates() {
    let mut server = Server::start_initialized("think_watch_step_focus_filters");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_watch",
                "plan_title": "Plan Step Watch",
                "task_title": "Task Step Watch",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            }
        }
    }));
    let task_id = extract_tool_text(&bootstrap)
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_watch",
                "target": task_id.clone(),
                "step": "focus",
                "card": {
                    "id": "CARD-STEP-H1",
                    "type": "hypothesis",
                    "title": "H1 step",
                    "text": "Step-scoped hypothesis"
                }
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_watch",
                "target": task_id.clone(),
                "card": {
                    "id": "CARD-GLOBAL-H1",
                    "type": "hypothesis",
                    "title": "H1 global",
                    "text": "Non-step-scoped hypothesis"
                }
            }
        }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 33,
        "method": "tools/call",
        "params": {
            "name": "think_watch",
            "arguments": {
                "workspace": "ws_step_watch",
                "target": task_id,
                "step": "focus",
                "limit_candidates": 20,
                "max_chars": 8000
            }
        }
    }));
    let watch_text = extract_tool_text(&watch);

    let candidates = watch_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let frontier_hypotheses = watch_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut ids = std::collections::BTreeSet::<String>::new();
    for card in candidates
        .into_iter()
        .chain(frontier_hypotheses.into_iter())
    {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            ids.insert(id.to_string());
        }
    }

    assert!(
        ids.contains("CARD-STEP-H1"),
        "step-scoped hypothesis must surface under step=focus watch"
    );
    assert!(
        !ids.contains("CARD-GLOBAL-H1"),
        "non-step-scoped hypothesis must be filtered out under step=focus watch"
    );

    // Trace is also step-scoped under `step="focus"`: note entries must carry `meta.step`.
    let trace_entries = watch_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        trace_entries.iter().all(|e| {
            e.get("kind").and_then(|v| v.as_str()) != Some("note")
                || e.get("meta")
                    .and_then(|v| v.get("step"))
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str())
                    == Some("s:0")
        }),
        "trace note entries must be step-stamped under step=focus watch"
    );
}

#[test]
fn think_frontier_step_focus_filters_to_step_scoped_cards() {
    let mut server = Server::start_initialized("think_frontier_step_focus_filters");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 40,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_frontier",
                "plan_title": "Plan Step Frontier",
                "task_title": "Task Step Frontier",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            }
        }
    }));
    let task_id = extract_tool_text(&bootstrap)
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 41,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_frontier",
                "target": task_id.clone(),
                "step": "focus",
                "card": {
                    "id": "CARD-STEP-H2",
                    "type": "hypothesis",
                    "title": "H2 step",
                    "text": "Step-scoped hypothesis"
                }
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_frontier",
                "target": task_id.clone(),
                "card": {
                    "id": "CARD-GLOBAL-H2",
                    "type": "hypothesis",
                    "title": "H2 global",
                    "text": "Non-step-scoped hypothesis"
                }
            }
        }
    }));

    let frontier = server.request(json!({
        "jsonrpc": "2.0",
        "id": 43,
        "method": "tools/call",
        "params": {
            "name": "think_frontier",
            "arguments": {
                "workspace": "ws_step_frontier",
                "target": task_id,
                "step": "focus",
                "max_chars": 8000
            }
        }
    }));
    let frontier_text = extract_tool_text(&frontier);
    let hypotheses = frontier_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut ids = std::collections::BTreeSet::<String>::new();
    for card in hypotheses {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            ids.insert(id.to_string());
        }
    }

    assert!(ids.contains("CARD-STEP-H2"));
    assert!(!ids.contains("CARD-GLOBAL-H2"));
}

#[test]
fn think_pack_step_focus_filters_candidates_to_step_scoped_cards() {
    let mut server = Server::start_initialized("think_pack_step_focus_filters");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 50,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_pack",
                "plan_title": "Plan Step Pack",
                "task_title": "Task Step Pack",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            }
        }
    }));
    let task_id = extract_tool_text(&bootstrap)
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 51,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_pack",
                "target": task_id.clone(),
                "step": "focus",
                "card": {
                    "id": "CARD-STEP-N1",
                    "type": "note",
                    "title": "N1 step",
                    "text": "Step-scoped note"
                }
            }
        }
    }));

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 52,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_pack",
                "target": task_id.clone(),
                "card": {
                    "id": "CARD-GLOBAL-N1",
                    "type": "note",
                    "title": "N1 global",
                    "text": "Non-step-scoped note"
                }
            }
        }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 53,
        "method": "tools/call",
        "params": {
            "name": "think_pack",
            "arguments": {
                "workspace": "ws_step_pack",
                "target": task_id,
                "step": "focus",
                "limit_candidates": 50,
                "max_chars": 8000
            }
        }
    }));
    let pack_text = extract_tool_text(&pack);
    let candidates = pack_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut ids = std::collections::BTreeSet::<String>::new();
    for card in candidates {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            ids.insert(id.to_string());
        }
    }

    assert!(ids.contains("CARD-STEP-N1"));
    assert!(!ids.contains("CARD-GLOBAL-N1"));
}

#[test]
fn think_next_step_focus_selects_step_scoped_candidate() {
    let mut server = Server::start_initialized("think_next_step_focus_filters");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 60,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_step_next",
                "plan_title": "Plan Step Next",
                "task_title": "Task Step Next",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": [] }
                ]
            }
        }
    }));
    let task_id = extract_tool_text(&bootstrap)
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 61,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_next",
                "target": task_id.clone(),
                "step": "focus",
                "card": {
                    "id": "CARD-STEP-Q1",
                    "type": "question",
                    "title": "Q1 step",
                    "text": "Step-scoped question"
                }
            }
        }
    }));

    // Create a newer global question that should win without step filtering.
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 62,
        "method": "tools/call",
        "params": {
            "name": "think_card",
            "arguments": {
                "workspace": "ws_step_next",
                "target": task_id.clone(),
                "card": {
                    "id": "CARD-GLOBAL-Q1",
                    "type": "question",
                    "title": "Q1 global",
                    "text": "Non-step-scoped question"
                }
            }
        }
    }));

    let next = server.request(json!({
        "jsonrpc": "2.0",
        "id": 63,
        "method": "tools/call",
        "params": {
            "name": "think_next",
            "arguments": {
                "workspace": "ws_step_next",
                "target": task_id,
                "step": "focus",
                "max_chars": 8000
            }
        }
    }));
    let next_text = extract_tool_text(&next);
    let candidate_id = next_text
        .get("result")
        .and_then(|v| v.get("candidate"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert_eq!(candidate_id, "CARD-STEP-Q1");
}
