#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_think_wrappers_subgoals_watch_lint_smoke() {
    let mut server =
        Server::start_initialized("branchmind_think_wrappers_subgoals_watch_lint_smoke");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_think_wrap" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let hypo1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.hypothesis", "args": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Same" } } } }
    }));
    let hypo1_text = extract_tool_text(&hypo1);
    let hypo1_id = hypo1_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo1 id")
        .to_string();

    let hypo2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.hypothesis", "args": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Same" } } } }
    }));
    let hypo2_text = extract_tool_text(&hypo2);
    let hypo2_id = hypo2_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo2 id")
        .to_string();

    let question = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.question", "args": { "workspace": "ws_think_wrap", "card": { "title": "Question", "text": "Why?" } } } }
    }));
    let question_text = extract_tool_text(&question);
    let question_id = question_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("question id")
        .to_string();

    let merge = server.request(json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.nominal.merge", "args": { "workspace": "ws_think_wrap", "candidate_ids": [hypo1_id.clone(), hypo2_id.clone()] } } }
    }));
    let merge_text = extract_tool_text(&merge);
    assert_eq!(
        merge_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let playbook = server.request(json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.playbook", "args": { "workspace": "ws_think_wrap", "name": "default" } } }
    }));
    let playbook_text = extract_tool_text(&playbook);
    assert_eq!(
        playbook_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let playbook_strict = server.request(json!({
        "jsonrpc": "2.0",
        "id": 161,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.playbook", "args": { "workspace": "ws_think_wrap", "name": "strict" } } }
    }));
    let playbook_strict_text = extract_tool_text(&playbook_strict);
    assert_eq!(
        playbook_strict_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let strict_steps = playbook_strict_text
        .get("result")
        .and_then(|v| v.get("template"))
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("strict playbook steps");
    assert!(
        strict_steps.iter().any(|s| {
            s.as_str()
                .is_some_and(|t| t.to_ascii_lowercase().contains("skeptic"))
        }),
        "strict playbook should include a skeptic step"
    );
    assert!(
        strict_steps.iter().any(|s| {
            s.as_str().is_some_and(|t| {
                let t = t.to_ascii_lowercase();
                t.contains("counter-hypothesis") && t.contains("stop criteria")
            })
        }),
        "strict playbook should include a counter-hypothesis + stop criteria loop"
    );
    assert!(
        strict_steps.iter().any(|s| {
            s.as_str().is_some_and(|t| {
                let t = t.to_ascii_lowercase();
                t.contains("breakthrough") || t.contains("10x") || t.contains("lever")
            })
        }),
        "strict playbook should include an optional breakthrough lever loop"
    );

    let playbook_breakthrough = server.request(json!({
        "jsonrpc": "2.0",
        "id": 162,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.playbook", "args": { "workspace": "ws_think_wrap", "name": "breakthrough" } } }
    }));
    let playbook_breakthrough_text = extract_tool_text(&playbook_breakthrough);
    assert_eq!(
        playbook_breakthrough_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let breakthrough_steps = playbook_breakthrough_text
        .get("result")
        .and_then(|v| v.get("template"))
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .expect("breakthrough playbook steps");
    assert!(
        breakthrough_steps.iter().any(|s| {
            s.as_str()
                .is_some_and(|t| t.to_ascii_lowercase().contains("inversion"))
        }),
        "breakthrough playbook should include inversion"
    );
    assert!(
        breakthrough_steps.iter().any(|s| {
            s.as_str().is_some_and(|t| {
                let t = t.to_ascii_lowercase();
                t.contains("10x") || t.contains("lever")
            })
        }),
        "breakthrough playbook should include a 10x/lever step"
    );
    assert!(
        breakthrough_steps.iter().any(|s| {
            s.as_str()
                .is_some_and(|t| t.to_ascii_lowercase().contains("stop criteria"))
        }),
        "breakthrough playbook should include stop criteria"
    );

    let subgoal_open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.subgoal.open", "args": { "workspace": "ws_think_wrap", "question_id": question_id.clone() } } }
    }));
    let subgoal_open_text = extract_tool_text(&subgoal_open);
    let subgoal_id = subgoal_open_text
        .get("result")
        .and_then(|v| v.get("subgoal_id"))
        .and_then(|v| v.as_str())
        .expect("subgoal id")
        .to_string();

    let subgoal_close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 18,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.subgoal.close", "args": { "workspace": "ws_think_wrap", "subgoal_id": subgoal_id, "return_card": "done" } } }
    }));
    let subgoal_close_text = extract_tool_text(&subgoal_close);
    assert_eq!(
        subgoal_close_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    server.request(json!({
        "jsonrpc": "2.0",
        "id": 19,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.add.note", "args": { "workspace": "ws_think_wrap", "card": "Note for watch" } } }
    }));

    let watch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.watch", "args": { "workspace": "ws_think_wrap", "limit_candidates": 10 } } }
    }));
    let watch_text = extract_tool_text(&watch);
    assert_eq!(
        watch_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let watch_budget = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.watch", "args": { "workspace": "ws_think_wrap", "limit_candidates": 10, "trace_limit_steps": 20, "max_chars": 400 } } }
    }));
    let watch_budget_text = extract_tool_text(&watch_budget);
    let watch_budget_obj = watch_budget_text
        .get("result")
        .and_then(|v| v.get("budget"))
        .expect("budget");
    let watch_used = watch_budget_obj
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .expect("used_chars");
    let watch_max = watch_budget_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .expect("max_chars");
    assert!(
        watch_used <= watch_max,
        "think_watch budget must not exceed max_chars"
    );
    let watch_entries_len = watch_budget_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    let watch_count = watch_budget_text
        .get("result")
        .and_then(|v| v.get("trace"))
        .and_then(|v| v.get("pagination"))
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        watch_count as usize, watch_entries_len,
        "think_watch pagination.count must match entries length"
    );

    // Keep at least one of the ids used to avoid “dead store” refactors.
    assert!(!hypo1_id.is_empty());
    assert!(!hypo2_id.is_empty());
}
