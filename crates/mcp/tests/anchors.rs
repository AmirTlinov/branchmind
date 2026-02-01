#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn anchors_autoregister_from_canon_tagged_cards() {
    let mut server = Server::start_initialized("anchors_autoregister_from_canon_tagged_cards");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_autoregister" } }
    }));

    // Simulate the daily "map attach" flow that uses think_card + v:canon but does not call
    // macro_anchor_note. The store should auto-register the anchor so it can be listed/opened.
    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_anchors_autoregister",
            "card": {
                "type": "note",
                "title": "Anchor attach note",
                "text": "Anchor should be auto-registered from canon-tagged cards.",
                "tags": ["a:auto-anchor", "v:canon"]
            }
        } }
    }));

    let list = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "anchors_list", "arguments": { "workspace": "ws_anchors_autoregister", "limit": 50 } }
    }));
    let list_text = extract_tool_text(&list);
    let anchors = list_text
        .get("result")
        .and_then(|v| v.get("anchors"))
        .and_then(|v| v.as_array())
        .expect("anchors_list result.anchors");
    assert!(
        anchors
            .iter()
            .any(|a| a.get("id").and_then(|v| v.as_str()) == Some("a:auto-anchor")),
        "anchors_list should include the auto-registered anchor id"
    );

    let opened = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_autoregister", "id": "a:auto-anchor", "limit": 20 } }
    }));
    let opened_text = extract_tool_text(&opened);
    let anchor = opened_text
        .get("result")
        .and_then(|v| v.get("anchor"))
        .and_then(|v| v.as_object())
        .expect("open(a:auto-anchor) result.anchor");
    assert_eq!(
        anchor.get("registered").and_then(|v| v.as_bool()),
        Some(true),
        "open(a:auto-anchor) should treat the anchor as registered"
    );
}

#[test]
fn anchors_macro_note_snapshot_and_export_smoke() {
    let mut server = Server::start_initialized("anchors_smoke");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_smoke" } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "macro_anchor_note", "arguments": {
            "workspace": "ws_anchors_smoke",
            "anchor": "a:core",
            "title": "Core",
            "kind": "component",
            "content": "Anchor registry note for a:core",
            "visibility": "canon",
            "pin": true
        } }
    }));

    // Cross-graph linking: cards written into other branches/docs with `a:*` tags should show up in anchor_snapshot.
    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": { "name": "branch_create", "arguments": {
            "workspace": "ws_anchors_smoke",
            "name": "task/anchor-links-smoke",
            "from": "main"
        } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_anchors_smoke",
            "branch": "task/anchor-links-smoke",
            "trace_doc": "task-trace",
            "graph_doc": "task-graph",
            "card": {
                "type": "decision",
                "title": "Decision: anchor snapshot should be cross-graph",
                "text": "Test card committed into a task graph doc",
                "tags": ["a:core"]
            }
        } }
    }));

    let list = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "anchors_list", "arguments": { "workspace": "ws_anchors_smoke", "limit": 50 } }
    }));
    let list_text = extract_tool_text(&list);
    if let Some(anchors) = list_text
        .get("result")
        .and_then(|v| v.get("anchors"))
        .and_then(|v| v.as_array())
    {
        assert!(
            anchors
                .iter()
                .any(|a| a.get("id").and_then(|v| v.as_str()) == Some("a:core")),
            "anchors_list should include the created anchor"
        );
    } else if let Some(s) = list_text.as_str() {
        assert!(
            s.contains("a:core"),
            "anchors_list text output should mention the created anchor"
        );
    } else {
        panic!("anchors_list output is neither json nor text");
    }

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "anchor_snapshot", "arguments": { "workspace": "ws_anchors_smoke", "anchor": "a:core", "limit": 20 } }
    }));
    let snapshot_text = extract_tool_text(&snapshot);
    if let Some(cards) = snapshot_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
    {
        assert!(
            cards.iter().any(|c| c.get("title").and_then(|v| v.as_str())
                == Some("Decision: anchor snapshot should be cross-graph")),
            "anchor_snapshot should include cross-graph cards linked by anchor tag"
        );
        assert!(
            cards.iter().any(|c| {
                c.get("tags")
                    .and_then(|v| v.as_array())
                    .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some("a:core")))
            }),
            "anchor_snapshot should include the anchor-tagged note"
        );
    } else if let Some(s) = snapshot_text.as_str() {
        assert!(
            s.contains("a:core"),
            "anchor_snapshot text output should mention the anchor"
        );
    } else {
        panic!("anchor_snapshot output is neither json nor text");
    }

    let export_1 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "anchors_export", "arguments": { "workspace": "ws_anchors_smoke", "format": "text" } }
    }));
    let export_1_text = extract_tool_text(&export_1);
    let exported_1 = if let Some(v) = export_1_text
        .get("result")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
    {
        v.to_string()
    } else if let Some(s) = export_1_text.as_str() {
        s.to_string()
    } else {
        panic!("anchors_export output is neither json nor text");
    };

    let export_2 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "anchors_export", "arguments": { "workspace": "ws_anchors_smoke", "format": "text" } }
    }));
    let export_2_text = extract_tool_text(&export_2);
    let exported_2 = if let Some(v) = export_2_text
        .get("result")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
    {
        v.to_string()
    } else if let Some(s) = export_2_text.as_str() {
        s.to_string()
    } else {
        panic!("anchors_export output is neither json nor text");
    };

    assert!(
        exported_1.contains("a:core"),
        "anchors_export(text) should include anchor ids"
    );
    assert_eq!(
        exported_1, exported_2,
        "anchors_export should be deterministic"
    );
}

#[test]
fn anchors_aliases_expand_snapshot_and_open() {
    let mut server = Server::start_initialized("anchors_aliases_expand_snapshot_and_open");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_aliases" } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "macro_anchor_note", "arguments": {
            "workspace": "ws_anchors_aliases",
            "anchor": "a:core",
            "title": "Core",
            "kind": "component",
            "aliases": ["a:foundation"],
            "content": "Anchor registry note for a:core",
            "visibility": "canon",
            "pin": true
        } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_anchors_aliases",
            "card": {
                "id": "CARD-ALIAS-1",
                "type": "decision",
                "title": "Decision tagged with alias",
                "text": "This is old history under the alias id.",
                "tags": ["a:foundation"]
            }
        } }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "anchor_snapshot", "arguments": { "workspace": "ws_anchors_aliases", "anchor": "a:core", "limit": 20 } }
    }));
    let snapshot_text = extract_tool_text(&snapshot);
    if let Some(cards) = snapshot_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
    {
        assert!(
            cards
                .iter()
                .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-ALIAS-1")),
            "anchor_snapshot should include alias-tagged history via aliases[]"
        );
    } else if let Some(s) = snapshot_text.as_str() {
        assert!(
            s.contains("CARD-ALIAS-1"),
            "anchor_snapshot lines output should mention the alias-tagged card id"
        );
    } else {
        panic!("anchor_snapshot output is neither json nor text");
    }

    let open_core = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_aliases", "id": "a:core", "limit": 20 } }
    }));
    let open_core_text = extract_tool_text(&open_core);
    let open_cards = open_core_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("open(a:core) cards");
    assert!(
        open_cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-ALIAS-1")),
        "open(a:core) should include alias-tagged history"
    );

    let open_alias = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_aliases", "id": "a:foundation", "limit": 20 } }
    }));
    let open_alias_text = extract_tool_text(&open_alias);
    let open_alias_cards = open_alias_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("open(a:foundation) cards");
    assert!(
        open_alias_cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-ALIAS-1")),
        "open(a:foundation) should resolve alias and include history"
    );
}

#[test]
fn anchor_snapshot_includes_recent_tasks_lens() {
    let mut server = Server::start_initialized("anchor_snapshot_includes_recent_tasks_lens");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchor_tasks_lens" } }
    }));

    // Register anchor (so anchor_snapshot works even before any history exists).
    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "macro_anchor_note", "arguments": {
            "workspace": "ws_anchor_tasks_lens",
            "anchor": "a:storage",
            "title": "Storage",
            "kind": "component",
            "content": "Anchor registry note for a:storage",
            "visibility": "canon",
            "pin": true
        } }
    }));

    // Create a task and commit an anchor-tagged card into its task graph.
    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": {
            "workspace": "ws_anchor_tasks_lens",
            "kind": "plan",
            "title": "Plan"
        } }
    }));
    let plan_id = extract_tool_text(&plan)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let task = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks_create", "arguments": {
            "workspace": "ws_anchor_tasks_lens",
            "kind": "task",
            "parent": plan_id,
            "title": "Storage task"
        } }
    }));
    let task_id = extract_tool_text(&task)
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let radar = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_radar", "arguments": { "workspace": "ws_anchor_tasks_lens", "task": task_id.clone() } }
    }));
    let radar_text = extract_tool_text(&radar);
    let branch = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.branch")
        .to_string();
    let graph_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("graph_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.graph_doc")
        .to_string();
    let trace_doc = radar_text
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("trace_doc"))
        .and_then(|v| v.as_str())
        .expect("reasoning_ref.trace_doc")
        .to_string();

    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_anchor_tasks_lens",
            "branch": branch,
            "graph_doc": graph_doc,
            "trace_doc": trace_doc,
            "card": {
                "type": "decision",
                "title": "Decision: anchor tasks lens",
                "text": "Anchor-tagged decision inside a task graph",
                "tags": ["a:storage", "v:canon"]
            }
        } }
    }));

    let snapshot = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "anchor_snapshot", "arguments": { "workspace": "ws_anchor_tasks_lens", "anchor": "a:storage", "limit": 20 } }
    }));
    let snapshot_text = extract_tool_text(&snapshot);
    if let Some(s) = snapshot_text.as_str() {
        assert!(
            s.lines()
                .any(|l| l.starts_with("tasks ") && l.contains(task_id.as_str())),
            "anchor_snapshot lines should include the task in the tasks lens"
        );
        assert!(
            s.lines()
                .any(|l| l.starts_with("tasks_snapshot") && l.contains(task_id.as_str())),
            "anchor_snapshot should include a copy/paste-ready tasks_snapshot for the top task"
        );
    } else if let Some(tasks) = snapshot_text
        .get("result")
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_array())
    {
        assert!(
            tasks
                .iter()
                .any(|t| t.get("task").and_then(|v| v.as_str()) == Some(task_id.as_str())),
            "anchor_snapshot json should include recent tasks touching the anchor"
        );
    } else {
        panic!("anchor_snapshot output is neither json nor text");
    }
}

#[test]
fn anchors_bootstrap_is_idempotent_and_visible_to_open() {
    let mut server = Server::start_initialized("anchors_bootstrap_is_idempotent_and_visible");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_bootstrap" } }
    }));

    let boot_1 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "anchors_bootstrap", "arguments": {
            "workspace": "ws_anchors_bootstrap",
            "anchors": [
                { "id": "a:core", "title": "Core", "kind": "component" },
                { "id": "a:storage", "title": "Storage adapter", "kind": "boundary", "depends_on": ["a:core"] }
            ]
        } }
    }));
    let boot_1_text = extract_tool_text(&boot_1);
    let created_1 = boot_1_text
        .get("result")
        .and_then(|v| v.get("created"))
        .and_then(|v| v.as_u64())
        .expect("anchors_bootstrap result.created");
    let updated_1 = boot_1_text
        .get("result")
        .and_then(|v| v.get("updated"))
        .and_then(|v| v.as_u64())
        .expect("anchors_bootstrap result.updated");
    assert_eq!(created_1, 2, "first bootstrap should create both anchors");
    assert_eq!(updated_1, 0, "first bootstrap should not report updates");

    let boot_2 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "anchors_bootstrap", "arguments": {
            "workspace": "ws_anchors_bootstrap",
            "anchors": [
                { "id": "a:core", "title": "Core", "kind": "component" },
                { "id": "a:storage", "title": "Storage adapter", "kind": "boundary", "depends_on": ["a:core"] }
            ]
        } }
    }));
    let boot_2_text = extract_tool_text(&boot_2);
    let created_2 = boot_2_text
        .get("result")
        .and_then(|v| v.get("created"))
        .and_then(|v| v.as_u64())
        .expect("anchors_bootstrap result.created");
    let updated_2 = boot_2_text
        .get("result")
        .and_then(|v| v.get("updated"))
        .and_then(|v| v.as_u64())
        .expect("anchors_bootstrap result.updated");
    assert_eq!(created_2, 0, "second bootstrap should be idempotent");
    assert_eq!(
        updated_2, 2,
        "second bootstrap should report both as updates"
    );

    let open_core = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_bootstrap", "id": "a:core", "limit": 10 } }
    }));
    let open_core_text = extract_tool_text(&open_core);
    let kind = open_core_text
        .get("result")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .expect("open(a:core) result.kind");
    assert_eq!(kind, "anchor");
    let id = open_core_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("open(a:core) result.id");
    assert_eq!(id, "a:core");
}

#[test]
fn anchors_rename_preserves_history_via_alias() {
    let mut server = Server::start_initialized("anchors_rename_preserves_history_via_alias");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_rename" } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "macro_anchor_note", "arguments": {
            "workspace": "ws_anchors_rename",
            "anchor": "a:core",
            "title": "Core",
            "kind": "component",
            "content": "Anchor registry note for a:core",
            "visibility": "canon"
        } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_anchors_rename",
            "card": {
                "id": "CARD-RENAME-1",
                "type": "decision",
                "title": "Decision before rename",
                "text": "This history should survive an anchor id refactor.",
                "tags": ["a:core"]
            }
        } }
    }));

    let renamed = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "anchors_rename", "arguments": {
            "workspace": "ws_anchors_rename",
            "from": "a:core",
            "to": "a:domain"
        } }
    }));
    let renamed_text = extract_tool_text(&renamed);
    assert_eq!(
        renamed_text
            .get("result")
            .and_then(|v| v.get("from"))
            .and_then(|v| v.as_str()),
        Some("a:core"),
        "anchors_rename should echo from"
    );
    assert_eq!(
        renamed_text
            .get("result")
            .and_then(|v| v.get("to"))
            .and_then(|v| v.as_str()),
        Some("a:domain"),
        "anchors_rename should echo to"
    );

    let open_domain = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_rename", "id": "a:domain", "limit": 20 } }
    }));
    let open_domain_text = extract_tool_text(&open_domain);
    let cards_domain = open_domain_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("open(a:domain) cards");
    assert!(
        cards_domain
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-RENAME-1")),
        "open(a:domain) should include cards tagged with the old id via alias mapping"
    );

    let open_core = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_rename", "id": "a:core", "limit": 20 } }
    }));
    let open_core_text = extract_tool_text(&open_core);
    let resolved_id = open_core_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("open(a:core) result.id");
    assert_eq!(
        resolved_id, "a:domain",
        "open(a:core) should resolve the alias to the new canonical id"
    );
    let aliases = open_core_text
        .get("result")
        .and_then(|v| v.get("anchor"))
        .and_then(|v| v.get("aliases"))
        .and_then(|v| v.as_array())
        .expect("open(a:core) result.anchor.aliases");
    assert!(
        aliases.iter().any(|v| v.as_str() == Some("a:core")),
        "renamed anchor should preserve the old id as an alias"
    );
}

#[test]
fn anchors_merge_merges_and_preserves_history_via_alias() {
    let mut server =
        Server::start_initialized("anchors_merge_merges_and_preserves_history_via_alias");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_merge" } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "anchors_bootstrap", "arguments": {
            "workspace": "ws_anchors_merge",
            "anchors": [
                { "id": "a:core", "title": "Core", "kind": "component" },
                { "id": "a:domain", "title": "Domain", "kind": "component" }
            ]
        } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_card", "arguments": {
            "workspace": "ws_anchors_merge",
            "card": {
                "id": "CARD-MERGE-1",
                "type": "decision",
                "title": "Decision before merge",
                "text": "History must survive map hygiene merges.",
                "tags": ["a:core"]
            }
        } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "anchors_merge", "arguments": {
            "workspace": "ws_anchors_merge",
            "into": "a:domain",
            "from": ["a:core"]
        } }
    }));

    let open_domain = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_merge", "id": "a:domain", "limit": 20 } }
    }));
    let open_domain_text = extract_tool_text(&open_domain);
    let cards_domain = open_domain_text
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("open(a:domain) cards");
    assert!(
        cards_domain
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-MERGE-1")),
        "open(a:domain) should include cards tagged with merged ids via alias mapping"
    );

    let open_core = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "workspace": "ws_anchors_merge", "id": "a:core", "limit": 20 } }
    }));
    let open_core_text = extract_tool_text(&open_core);
    let resolved_id = open_core_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("open(a:core) result.id");
    assert_eq!(
        resolved_id, "a:domain",
        "open(a:core) should resolve merged id to the canonical anchor"
    );
    let aliases = open_core_text
        .get("result")
        .and_then(|v| v.get("anchor"))
        .and_then(|v| v.get("aliases"))
        .and_then(|v| v.as_array())
        .expect("open(a:core) result.anchor.aliases");
    assert!(
        aliases.iter().any(|v| v.as_str() == Some("a:core")),
        "canonical anchor should include merged ids as aliases"
    );
}

#[test]
fn anchors_lint_reports_unknown_depends_on() {
    let mut server = Server::start_initialized("anchors_lint_reports_unknown_depends_on");

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_anchors_lint" } }
    }));

    let _ = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "anchors_bootstrap", "arguments": {
            "workspace": "ws_anchors_lint",
            "anchors": [
                { "id": "a:core", "title": "Core", "kind": "component", "depends_on": ["a:missing"] }
            ]
        } }
    }));

    let lint = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "anchors_lint", "arguments": { "workspace": "ws_anchors_lint", "limit": 50 } }
    }));
    let lint_text = extract_tool_text(&lint);
    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("anchors_lint result.issues");
    assert!(
        issues
            .iter()
            .any(|i| i.get("code").and_then(|v| v.as_str()) == Some("UNKNOWN_DEPENDS_ON")),
        "anchors_lint should report unknown depends_on entries"
    );
}
