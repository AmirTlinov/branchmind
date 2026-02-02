#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_lint_suggests_recall_and_seed_when_anchored_task_has_no_knowledge() {
    let mut server = Server::start_initialized(
        "tasks_lint_suggests_recall_and_seed_when_anchored_task_has_no_knowledge",
    );

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Knowledge Lint",
                "task_title": "Task Knowledge Lint",
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
        .expect("task id");

    let _ = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "macro_anchor_note", "arguments": {
            "workspace": "ws1",
            "target": task_id,
            "anchor": "a:core",
            "title": "Core",
            "kind": "component",
            "content": "Bind task to anchor a:core",
            "card_type": "note",
            "visibility": "canon"
        } }
    }));

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let result = lint_text.get("result").expect("result");
    let empty_issues = Vec::new();
    let issues = result
        .get("issues")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_issues);
    assert!(
        issues.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "KNOWLEDGE_EMPTY_FOR_ANCHOR")
        }),
        "expected KNOWLEDGE_EMPTY_FOR_ANCHOR issue, got:\n{lint_text}"
    );

    let empty_actions = Vec::new();
    let actions = result
        .get("actions")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_actions);
    assert!(
        actions.iter().any(|action| {
            action
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "action:task:knowledge:recall")
        }),
        "expected action:task:knowledge:recall action, got:\n{lint_text}"
    );
    assert!(
        actions.iter().any(|action| {
            action
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "action:task:knowledge:seed")
        }),
        "expected action:task:knowledge:seed action, got:\n{lint_text}"
    );

    let upsert = server.request_raw(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws1",
                "op": "knowledge.upsert",
                "args": {
                    "anchor": "a:core",
                    "key": "invariants",
                    "card": { "title": "Core invariants", "text": "Claim: Must be deterministic.\nApply: Avoid nondeterministic IO.\nProof: CMD: make check\nExpiry: 2027-01-01" }
                }
            }
        }
    }));
    let upsert_text = extract_tool_text(&upsert);
    assert_eq!(
        upsert_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "expected upsert to succeed, got:\n{upsert_text}"
    );

    let lint_2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_2_text = extract_tool_text(&lint_2);

    let result_2 = lint_2_text.get("result").expect("result");
    let empty_issues_2 = Vec::new();
    let issues_2 = result_2
        .get("issues")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_issues_2);
    assert!(
        !issues_2.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "KNOWLEDGE_EMPTY_FOR_ANCHOR")
        }),
        "expected KNOWLEDGE_EMPTY_FOR_ANCHOR to disappear after upsert, got:\n{lint_2_text}"
    );

    let empty_actions_2 = Vec::new();
    let actions_2 = result_2
        .get("actions")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_actions_2);
    assert!(
        actions_2.iter().any(|action| {
            action
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "action:task:knowledge:recall")
        }),
        "expected action:task:knowledge:recall to remain available, got:\n{lint_2_text}"
    );
    assert!(
        !actions_2.iter().any(|action| {
            action
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "action:task:knowledge:seed")
        }),
        "expected action:task:knowledge:seed to disappear after upsert, got:\n{lint_2_text}"
    );
}
