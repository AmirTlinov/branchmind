#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn think_watch_filters_other_agents_by_default() {
    let mut server = Server::start_initialized("think_watch_filters_other_agents_by_default");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_watch" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_watch",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_watch",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let watch_a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.watch", "args": { "workspace": "ws_lanes_watch", "agent_id": "agent-a", "limit_hypotheses": 10, "limit_candidates": 10 } } }
    }));
    let watch_a_text = extract_tool_text(&watch_a);
    let capsule_type = watch_a_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(capsule_type, "watch_capsule");
    let lane_kind = watch_a_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("lane"))
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(lane_kind, "shared");
    let frontier_hypotheses = watch_a_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .expect("frontier.hypotheses");
    assert!(
        frontier_hypotheses
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "default watch should include non-draft cards"
    );
    assert!(
        frontier_hypotheses
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "default watch should exclude drafts"
    );
}

#[test]
fn think_watch_all_lanes_is_explicit_opt_in() {
    let mut server = Server::start_initialized("think_watch_all_lanes_is_explicit_opt_in");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_watch_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_watch_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_watch_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let watch_all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.watch", "args": { "workspace": "ws_lanes_watch_all", "agent_id": "agent-a", "all_lanes": true, "limit_hypotheses": 10, "limit_candidates": 10 } } }
    }));
    let watch_all_text = extract_tool_text(&watch_all);
    let lane_kind = watch_all_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("lane"))
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        lane_kind, "all",
        "all-lanes mode must be explicit in capsule"
    );

    let frontier_hypotheses = watch_all_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .expect("frontier.hypotheses");
    assert!(
        frontier_hypotheses
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "all_lanes=true should include drafts"
    );
}

#[test]
fn tasks_resume_super_smart_filters_other_agent_cards() {
    let mut server =
        Server::start_initialized("tasks_resume_super_smart_filters_other_agent_cards");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.bootstrap", "args": {
                "workspace": "ws_lanes_tasks",
                "plan_title": "Plan",
                "task_title": "Task",
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

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_tasks",
            "target": task_id.clone(),
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_tasks",
            "target": task_id.clone(),
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let _dec_a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_tasks",
            "target": task_id.clone(),
            "agent_id": "agent-a",
            "card": { "id": "DEC-A", "type": "decision", "title": "A decision", "text": "from agent a" }
        } } }
    }));
    let _dec_b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_tasks",
            "target": task_id.clone(),
            "agent_id": "agent-b",
            "card": { "id": "DEC-B", "type": "decision", "title": "B decision", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let resume = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": {
                "workspace": "ws_lanes_tasks",
                "task": task_id.clone(),
                "view": "smart",
                "agent_id": "agent-a",
                "cards_limit": 10,
                "decisions_limit": 10,
                "max_chars": 8000
            } } }
    }));
    let resume_text = extract_tool_text(&resume);
    let cards = resume_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("memory.cards");

    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "smart view should include non-draft cards"
    );
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "smart view should exclude drafts"
    );

    let decisions = resume_text
        .get("result")
        .and_then(|v| v.get("signals"))
        .and_then(|v| v.get("decisions"))
        .and_then(|v| v.as_array())
        .expect("signals.decisions");
    assert!(
        decisions
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("DEC-A")),
        "smart view should include non-draft decisions"
    );
    assert!(
        decisions
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("DEC-B")),
        "smart view should exclude draft decisions from signals"
    );

    // Audit view is explicit opt-in: it should include drafts.
    let audit = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.resume.super", "args": {
                "workspace": "ws_lanes_tasks",
                "task": task_id,
                "view": "audit",
                "agent_id": "agent-a",
                "cards_limit": 20,
                "decisions_limit": 10,
                "max_chars": 12000
            } } }
    }));
    let audit_text = extract_tool_text(&audit);
    let lane_kind = audit_text
        .get("result")
        .and_then(|v| v.get("capsule"))
        .and_then(|v| v.get("where"))
        .and_then(|v| v.get("lane"))
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(lane_kind, "all", "audit view must advertise all lanes");

    let audit_cards = audit_text
        .get("result")
        .and_then(|v| v.get("memory"))
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("memory.cards");
    assert!(
        audit_cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "audit view should include drafts"
    );

    let audit_decisions = audit_text
        .get("result")
        .and_then(|v| v.get("signals"))
        .and_then(|v| v.get("decisions"))
        .and_then(|v| v.as_array())
        .expect("signals.decisions");
    assert!(
        audit_decisions
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("DEC-B")),
        "audit view should include draft decisions in signals"
    );
}

#[test]
fn think_publish_promotes_to_shared_lane_and_is_visible_without_agent_id() {
    let mut server = Server::start_initialized("think_publish_promotes_to_shared_lane");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_publish" } } }
    }));

    let _src = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_publish",
            "agent_id": "agent-a",
            "card": { "id": "CARD-DEC", "type": "decision", "title": "Decision", "text": "draft" }
        } } }
    }));

    let publish = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.publish", "args": { "workspace": "ws_lanes_publish", "card_id": "CARD-DEC", "agent_id": "agent-a", "pin": true } } }
    }));
    let publish_text = extract_tool_text(&publish);
    let published_id = publish_text
        .get("result")
        .and_then(|v| v.get("published_card_id"))
        .and_then(|v| v.as_str())
        .expect("published_card_id");
    assert_eq!(published_id, "CARD-PUB-CARD-DEC");

    let watch_shared = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.watch", "args": { "workspace": "ws_lanes_publish", "limit_candidates": 20 } } }
    }));
    let watch_shared_text = extract_tool_text(&watch_shared);
    let candidates = watch_shared_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .expect("candidates");
    assert!(
        candidates
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-PUB-CARD-DEC")),
        "published card should be visible in shared view"
    );
}

#[test]
fn think_pack_and_query_filter_other_agent_lanes() {
    let mut server = Server::start_initialized("think_pack_and_query_filter_other_agent_lanes");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_pack" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_pack",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_pack",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let pack_a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.pack", "args": {
            "workspace": "ws_lanes_pack",
            "agent_id": "agent-a",
            "limit_candidates": 20,
            "limit_hypotheses": 20,
            "max_chars": 8000
        } } }
    }));
    let pack_a_text = extract_tool_text(&pack_a);
    let candidates = pack_a_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .expect("candidates");
    assert!(
        candidates
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "think_pack should include non-draft candidates"
    );
    assert!(
        candidates
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "think_pack should exclude drafts by default"
    );

    let query_a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.query", "args": {
            "workspace": "ws_lanes_pack",
            "agent_id": "agent-a",
            "types": ["hypothesis"],
            "limit": 20,
            "max_chars": 8000
        } } }
    }));
    let query_a_text = extract_tool_text(&query_a);
    let cards = query_a_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "think_query should include non-draft cards"
    );
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "think_query should exclude drafts by default"
    );
}

#[test]
fn think_pack_all_lanes_is_explicit_opt_in() {
    let mut server = Server::start_initialized("think_pack_all_lanes_is_explicit_opt_in");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_pack_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_pack_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a" }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_pack_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let pack_all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.pack", "args": { "workspace": "ws_lanes_pack_all", "agent_id": "agent-a", "all_lanes": true, "limit_candidates": 20, "limit_hypotheses": 20 } } }
    }));
    let pack_all_text = extract_tool_text(&pack_all);
    let candidates = pack_all_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .expect("candidates");
    assert!(
        candidates
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "think_pack all_lanes=true should include drafts"
    );
}

#[test]
fn context_pack_filters_other_agent_lanes_in_graph_slices() {
    let mut server = Server::start_initialized("context_pack_filters_other_agent_lanes");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_context_pack" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_context_pack",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "decision", "title": "A", "text": "from agent a" }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_context_pack",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "decision", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context.pack", "args": {
            "workspace": "ws_lanes_context_pack",
            "agent_id": "agent-a",
            "limit_cards": 20,
            "decisions_limit": 20,
            "max_chars": 8000
        } } }
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
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "context_pack should include non-draft cards"
    );
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "context_pack should exclude drafts by default"
    );

    let decisions = pack_text
        .get("result")
        .and_then(|v| v.get("signals"))
        .and_then(|v| v.get("decisions"))
        .and_then(|v| v.as_array())
        .expect("signals.decisions");
    assert!(
        decisions
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "context_pack signals should include non-draft decisions"
    );
    assert!(
        decisions
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "context_pack signals should exclude draft decisions by default"
    );
}

#[test]
fn think_context_all_lanes_is_explicit_opt_in() {
    let mut server = Server::start_initialized("think_context_all_lanes_is_explicit_opt_in");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_think_context_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_context_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_context_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let filtered = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": {
            "workspace": "ws_lanes_think_context_all",
            "agent_id": "agent-a",
            "limit_cards": 20,
            "max_chars": 8000
        } } }
    }));
    let filtered_text = extract_tool_text(&filtered);
    let cards = filtered_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "think_context should include non-draft cards"
    );
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "think_context should exclude drafts by default"
    );

    let all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context", "args": {
            "workspace": "ws_lanes_think_context_all",
            "agent_id": "agent-a",
            "all_lanes": true,
            "limit_cards": 20,
            "max_chars": 8000
        } } }
    }));
    let all_text = extract_tool_text(&all);
    let cards = all_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "think_context all_lanes=true should include drafts"
    );
}

#[test]
fn think_frontier_all_lanes_is_explicit_opt_in() {
    let mut server = Server::start_initialized("think_frontier_all_lanes_is_explicit_opt_in");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_think_frontier_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_frontier_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_frontier_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let filtered = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.frontier", "args": {
            "workspace": "ws_lanes_think_frontier_all",
            "agent_id": "agent-a",
            "limit_hypotheses": 20,
            "max_chars": 8000
        } } }
    }));
    let filtered_text = extract_tool_text(&filtered);
    let hypotheses = filtered_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .expect("frontier.hypotheses");
    assert!(
        hypotheses
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "think_frontier should include non-draft cards"
    );
    assert!(
        hypotheses
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "think_frontier should exclude drafts by default"
    );

    let all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.frontier", "args": {
            "workspace": "ws_lanes_think_frontier_all",
            "agent_id": "agent-a",
            "all_lanes": true,
            "limit_hypotheses": 20,
            "max_chars": 8000
        } } }
    }));
    let all_text = extract_tool_text(&all);
    let hypotheses = all_text
        .get("result")
        .and_then(|v| v.get("frontier"))
        .and_then(|v| v.get("hypotheses"))
        .and_then(|v| v.as_array())
        .expect("frontier.hypotheses");
    assert!(
        hypotheses
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "think_frontier all_lanes=true should include drafts"
    );
}

#[test]
fn think_next_all_lanes_can_select_other_lane_candidate() {
    let mut server = Server::start_initialized("think_next_all_lanes_can_select_other_lane");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_think_next_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_next_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_next_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let filtered = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.next", "args": {
            "workspace": "ws_lanes_think_next_all",
            "agent_id": "agent-a",
            "max_chars": 8000
        } } }
    }));
    let filtered_text = extract_tool_text(&filtered);
    let candidate = filtered_text
        .get("result")
        .and_then(|v| v.get("candidate"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        candidate, "CARD-A",
        "default think_next should avoid drafts"
    );

    let all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.next", "args": {
            "workspace": "ws_lanes_think_next_all",
            "agent_id": "agent-a",
            "all_lanes": true,
            "max_chars": 8000
        } } }
    }));
    let all_text = extract_tool_text(&all);
    let candidate = all_text
        .get("result")
        .and_then(|v| v.get("candidate"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        candidate, "CARD-B",
        "all_lanes=true should allow selection from drafts"
    );
}

#[test]
fn think_query_all_lanes_is_explicit_opt_in() {
    let mut server = Server::start_initialized("think_query_all_lanes_is_explicit_opt_in");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_think_query_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_query_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "from agent a" }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_think_query_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let filtered = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.query", "args": {
            "workspace": "ws_lanes_think_query_all",
            "agent_id": "agent-a",
            "types": ["hypothesis"],
            "limit": 20,
            "max_chars": 8000
        } } }
    }));
    let filtered_text = extract_tool_text(&filtered);
    let cards = filtered_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .all(|c| c.get("id").and_then(|v| v.as_str()) != Some("CARD-B")),
        "think_query should exclude drafts by default"
    );

    let all = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.query", "args": {
            "workspace": "ws_lanes_think_query_all",
            "agent_id": "agent-a",
            "all_lanes": true,
            "types": ["hypothesis"],
            "limit": 20,
            "max_chars": 8000
        } } }
    }));
    let all_text = extract_tool_text(&all);
    let cards = all_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "think_query all_lanes=true should include drafts"
    );
}

#[test]
fn context_pack_all_lanes_is_explicit_opt_in() {
    let mut server = Server::start_initialized("context_pack_all_lanes_is_explicit_opt_in");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_lanes_context_pack_all" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_context_pack_all",
            "agent_id": "agent-a",
            "card": { "id": "CARD-A", "type": "decision", "title": "A", "text": "from agent a" }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_lanes_context_pack_all",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "decision", "title": "B", "text": "from agent b", "tags": ["v:draft"] }
        } } }
    }));

    let pack = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.context.pack", "args": {
            "workspace": "ws_lanes_context_pack_all",
            "agent_id": "agent-a",
            "all_lanes": true,
            "limit_cards": 20,
            "decisions_limit": 20,
            "max_chars": 12000
        } } }
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
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "context_pack all_lanes=true should include drafts in cards"
    );

    let decisions = pack_text
        .get("result")
        .and_then(|v| v.get("signals"))
        .and_then(|v| v.get("decisions"))
        .and_then(|v| v.as_array())
        .expect("signals.decisions");
    assert!(
        decisions
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "context_pack all_lanes=true should include drafts in signals"
    );
}

#[test]
fn default_agent_id_is_injected_when_configured() {
    let mut server = Server::start_initialized_with_args(
        "default_agent_id_is_injected_when_configured",
        &["--agent-id", "agent-a"],
    );

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_default_agent" } } }
    }));

    let _a = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_default_agent",
            "card": { "id": "CARD-A", "type": "hypothesis", "title": "A", "text": "default lane", "tags": ["v:canon"] }
        } } }
    }));
    let _shared = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_default_agent",
            "agent_id": null,
            "card": { "id": "CARD-S", "type": "hypothesis", "title": "S", "text": "shared lane", "tags": ["v:canon"] }
        } } }
    }));
    let _b = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "workspace": "ws_default_agent",
            "agent_id": "agent-b",
            "card": { "id": "CARD-B", "type": "hypothesis", "title": "B", "text": "other lane", "tags": ["v:canon"] }
        } } }
    }));

    // agent_id omitted: default agent id must not partition durable memory.
    let watch_default = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.watch", "args": { "workspace": "ws_default_agent", "limit_hypotheses": 20, "limit_candidates": 20 } } }
    }));
    let watch_text = extract_tool_text(&watch_default);
    let candidates = watch_text
        .get("result")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.as_array())
        .expect("candidates");
    assert!(
        candidates
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-A")),
        "default agent id should include cards created with agent_id omitted"
    );
    assert!(
        candidates
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-S")),
        "explicit agent_id=null should still be visible"
    );
    assert!(
        candidates
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-B")),
        "default agent id must not filter out other writers"
    );
}
