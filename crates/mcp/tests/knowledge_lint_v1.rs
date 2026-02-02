#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn knowledge_lint_reports_duplicate_content_same_anchor() {
    let mut server =
        Server::start_initialized("knowledge_lint_reports_duplicate_content_same_anchor");

    let _ = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_dup",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism",
                    "card": { "title": "Determinism", "text": "Must be deterministic." }
                }
            }
        }
    }));

    let _ = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_dup",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism-copy",
                    "card": { "title": "Determinism", "text": "Must be deterministic." }
                }
            }
        }
    }));

    let lint = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_dup",
                "op": "knowledge.lint",
                "args": { "anchor": "core", "limit": 50 }
            }
        }
    }));
    let text = extract_tool_text(&lint);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));

    let issues = text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues");
    let dup = issues.iter().find(|issue| {
        issue.get("code").and_then(|v| v.as_str())
            == Some("KNOWLEDGE_DUPLICATE_CONTENT_SAME_ANCHOR")
    });
    assert!(dup.is_some(), "expected duplicate-content warning");
    let evidence = dup
        .and_then(|v| v.get("evidence"))
        .and_then(|v| v.as_object())
        .expect("evidence");
    assert_eq!(
        evidence.get("anchor_id").and_then(|v| v.as_str()),
        Some("a:core")
    );

    let keys = evidence
        .get("keys")
        .and_then(|v| v.as_array())
        .expect("keys");
    let rendered = keys
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        rendered.contains("determinism") && rendered.contains("determinism-copy"),
        "expected both keys present, got: {rendered}"
    );

    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions");
    assert!(
        actions.iter().any(|a| {
            a.get("action_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .starts_with("knowledge.lint.duplicate.open::a:core::")
        }),
        "expected open-helper action for duplicates"
    );
}

#[test]
fn knowledge_lint_does_not_flag_non_duplicates() {
    let mut server = Server::start_initialized("knowledge_lint_does_not_flag_non_duplicates");

    let _ = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_clean",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "determinism",
                    "card": { "title": "Determinism", "text": "Must be deterministic." }
                }
            }
        }
    }));

    let _ = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_clean",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "concurrency",
                    "card": { "title": "Concurrency", "text": "Be explicit about locks." }
                }
            }
        }
    }));

    let lint = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_clean",
                "op": "knowledge.lint",
                "args": { "anchor": "core", "limit": 50 }
            }
        }
    }));
    let text = extract_tool_text(&lint);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));

    let issues = text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues");
    assert!(
        !issues.iter().any(|issue| {
            issue.get("code").and_then(|v| v.as_str())
                == Some("KNOWLEDGE_DUPLICATE_CONTENT_SAME_ANCHOR")
        }),
        "did not expect duplicate-content warning"
    );
}

#[test]
fn knowledge_lint_reports_overloaded_key_across_anchors() {
    let mut server =
        Server::start_initialized("knowledge_lint_reports_overloaded_key_across_anchors");

    let _ = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_overloaded",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "core",
                    "key": "overview",
                    "card": { "title": "Overview", "text": "Core overview." }
                }
            }
        }
    }));

    let _ = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_overloaded",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "storage",
                    "key": "overview",
                    "card": { "title": "Overview", "text": "Storage overview." }
                }
            }
        }
    }));

    let lint = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws_kb_lint_overloaded",
                "op": "knowledge.lint",
                "args": { "limit": 200 }
            }
        }
    }));
    let text = extract_tool_text(&lint);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));

    let issues = text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues");
    let overloaded = issues.iter().find(|issue| {
        issue.get("code").and_then(|v| v.as_str())
            == Some("KNOWLEDGE_KEY_OVERLOADED_ACROSS_ANCHORS")
            && issue
                .get("evidence")
                .and_then(|v| v.get("key"))
                .and_then(|v| v.as_str())
                == Some("overview")
    });
    assert!(overloaded.is_some(), "expected overloaded-key info finding");

    let actions = text
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions");
    assert!(
        actions.iter().any(|a| {
            a.get("action_id").and_then(|v| v.as_str()) == Some("knowledge.lint.key.open::overview")
        }),
        "expected open-helper action for overloaded key"
    );
}
