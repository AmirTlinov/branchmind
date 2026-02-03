#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_think_card_and_context_smoke() {
    let mut server = Server::start_initialized("branchmind_think_card_and_context_smoke");

    let created_plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_think", "kind": "plan", "title": "Plan A" } } }
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
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": { "workspace": "ws_think", "kind": "task", "parent": plan_id, "title": "Task A" } } }
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
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.radar", "args": { "workspace": "ws_think", "task": task_id.clone() } } }
    }));
    let radar_text = extract_tool_text(&radar);
    let canonical_branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let trace_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.trace_doc")
        .to_string();
    let graph_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("graph_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.graph_doc")
        .to_string();

    let template = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.reasoning.seed", "args": { "workspace": "ws_think", "type": "hypothesis" } } }
    }));
    let template_text = extract_tool_text(&template);
    assert_eq!(
        template_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let auto_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 55,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": { "workspace": "ws_think", "target": task_id.clone(), "card": "Quick note" } } }
    }));
    let auto_card_text = extract_tool_text(&auto_card);
    assert_eq!(
        auto_card_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let auto_id = auto_card_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("auto card id");
    assert!(auto_id.starts_with("CARD-"), "auto id must be generated");

    let card_id = "CARD-EXPLICIT-1";
    let title = "Hypothesis";
    let text = "It should improve UX";
    let think_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": { "workspace": "ws_think", "target": task_id.clone(), "card": { "id": card_id, "type": "hypothesis", "title": title, "text": text, "tags": ["UX", "MVP"], "meta": { "why": "smoke" } } } } }
    }));
    let think_card_text = extract_tool_text(&think_card);
    assert_eq!(
        think_card_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        think_card_text
            .get("result")
            .and_then(|v| v.get("card_id"))
            .and_then(|v| v.as_str()),
        Some(card_id)
    );
    let inserted = think_card_text
        .get("result")
        .and_then(|v| v.get("inserted"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(inserted, "first think_card call must insert trace entry");

    let show_trace = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.show", "args": { "workspace": "ws_think", "branch": canonical_branch.clone(), "doc": trace_doc.clone(), "limit": 50 } } }
    }));
    let show_trace_text = extract_tool_text(&show_trace);
    let entries = show_trace_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        entries.iter().any(|e| {
            e.get("kind").and_then(|v| v.as_str()) == Some("note")
                && e.get("format").and_then(|v| v.as_str()) == Some("think_card")
                && e.get("title").and_then(|v| v.as_str()) == Some(title)
                && e.get("meta")
                    .and_then(|v| v.get("card_id"))
                    .and_then(|v| v.as_str())
                    == Some(card_id)
        }),
        "trace must include think_card note entry"
    );

    let query_graph = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "graph", "arguments": { "op": "call", "cmd": "graph.query", "args": { "workspace": "ws_think", "branch": canonical_branch.clone(), "doc": graph_doc.clone(), "ids": [card_id], "include_edges": false, "limit": 10 } } }
    }));
    let query_graph_text = extract_tool_text(&query_graph);
    assert_eq!(
        query_graph_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let nodes = query_graph_text
        .get("result")
        .and_then(|v| v.get("nodes"))
        .and_then(|v| v.as_array())
        .expect("nodes");
    assert!(
        nodes.iter().any(|n| {
            n.get("id").and_then(|v| v.as_str()) == Some(card_id)
                && n.get("type").and_then(|v| v.as_str()) == Some("hypothesis")
                && n.get("title").and_then(|v| v.as_str()) == Some(title)
        }),
        "graph must include think card node"
    );

    let think_card_again = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": { "workspace": "ws_think", "target": task_id.clone(), "card": { "id": card_id, "type": "hypothesis", "title": title, "text": text, "tags": ["ux", "mvp"], "meta": { "why": "smoke" } } } } }
    }));
    let think_card_again_text = extract_tool_text(&think_card_again);
    assert_eq!(
        think_card_again_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    let inserted2 = think_card_again_text
        .get("result")
        .and_then(|v| v.get("inserted"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(
        !inserted2,
        "second think_card call must be idempotent (inserted=false)"
    );

    let ctx = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": { "workspace": "ws_think", "branch": canonical_branch, "graph_doc": graph_doc, "include_drafts": true, "limit_cards": 10, "max_chars": 2000 } } }
    }));
    let ctx_text = extract_tool_text(&ctx);
    assert_eq!(
        ctx_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );
    let cards = ctx_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some(card_id)),
        "think_context must include the card"
    );
}
