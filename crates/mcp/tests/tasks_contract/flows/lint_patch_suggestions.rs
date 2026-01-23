#![forbid(unsafe_code)]

use super::super::support::*;
use serde_json::json;

#[test]
fn tasks_lint_patch_suggestions_seed_missing_criteria() {
    let mut server =
        Server::start_initialized("tasks_lint_patch_suggestions_seed_missing_criteria");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task Lint",
                "steps": [
                    { "title": "Implement: thing", "success_criteria": ["c1"], "tests": ["t1"] }
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
    let task_rev = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("task revision");
    let step_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id");

    // Remove criteria so lint suggests a seed patch instead of a confirm patch.
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_patch",
            "arguments": {
                "workspace": "ws1",
                "task": task_id,
                "expected_revision": task_rev,
                "kind": "step",
                "step_id": step_id,
                "ops": [
                    { "op": "unset", "field": "success_criteria" }
                ]
            }
        }
    }));

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);
    let patches = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .expect("patches array");
    assert!(
        patches.iter().any(|patch| {
            patch
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id.contains("seed_success_criteria"))
                && patch
                    .get("apply")
                    .and_then(|v| v.get("tool"))
                    .and_then(|v| v.as_str())
                    == Some("tasks_patch")
        }),
        "expected a seed_success_criteria tasks_patch suggestion, got:\n{lint_text}"
    );
}

#[test]
fn tasks_lint_patch_suggestions_active_limit_exceeded() {
    let mut server =
        Server::start_initialized("tasks_lint_patch_suggestions_active_limit_exceeded");

    let bootstrap_1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task 1",
                "steps": [
                    { "title": "Step 1", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_1_text = extract_tool_text(&bootstrap_1);
    let plan_id = bootstrap_1_text
        .get("result")
        .and_then(|v| v.get("plan"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id");

    let task_1_id = bootstrap_1_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task 1 id");

    let mut task_ids = vec![task_1_id.to_string()];
    for idx in 2..=4 {
        let bootstrap_n = server.request(json!({
            "jsonrpc": "2.0",
            "id": 10 + idx,
            "method": "tools/call",
            "params": {
                "name": "tasks_bootstrap",
                "arguments": {
                    "workspace": "ws1",
                    "plan": plan_id,
                    "task_title": format!("Task {idx}"),
                    "steps": [
                        { "title": "Step 1", "success_criteria": ["c1"], "tests": ["t1"] }
                    ]
                }
            }
        }));
        let bootstrap_n_text = extract_tool_text(&bootstrap_n);
        let task_id = bootstrap_n_text
            .get("result")
            .and_then(|v| v.get("task"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .expect("task id");
        task_ids.push(task_id.to_string());
    }

    // Force 4 ACTIVE tasks in the same plan, so the plan lint emits the anti-kasha warning.
    for (i, task_id) in task_ids.iter().enumerate() {
        server.request(json!({
            "jsonrpc": "2.0",
            "id": 20 + i,
            "method": "tools/call",
            "params": {
                "name": "tasks_complete",
                "arguments": {
                    "workspace": "ws1",
                    "task": task_id,
                    "status": "ACTIVE"
                }
            }
        }));
    }

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "plan": plan_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        issues.iter().any(|issue| issue
            .get("code")
            .and_then(|v| v.as_str())
            .is_some_and(|code| code == "ACTIVE_LIMIT_EXCEEDED")),
        "expected ACTIVE_LIMIT_EXCEEDED issue, got:\n{lint_text}"
    );

    let patches = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .expect("patches array");
    assert!(
        patches.iter().any(|patch| {
            let id_ok = patch
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id.starts_with("patch:plan:active_limit:park:"));
            let tool_ok = patch
                .get("apply")
                .and_then(|v| v.get("tool"))
                .and_then(|v| v.as_str())
                == Some("tasks_complete");
            let status_ok = patch
                .get("apply")
                .and_then(|v| v.get("arguments"))
                .and_then(|v| v.get("status"))
                .and_then(|v| v.as_str())
                == Some("TODO");
            id_ok && tool_ok && status_ok
        }),
        "expected a park-to-TODO tasks_complete patch suggestion, got:\n{lint_text}"
    );

    let actions = lint_text
        .get("result")
        .and_then(|v| v.get("actions"))
        .and_then(|v| v.as_array())
        .expect("actions array");
    assert!(
        actions.iter().any(|action| {
            action
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "action:plan:list_active_tasks")
        }),
        "expected action:plan:list_active_tasks in actions, got:\n{lint_text}"
    );
}

#[test]
fn tasks_lint_patch_suggestions_missing_anchor_for_task() {
    let mut server =
        Server::start_initialized("tasks_lint_patch_suggestions_missing_anchor_for_task");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task Without Anchor",
                "steps": [
                    { "title": "Implement: thing", "success_criteria": ["c1"], "tests": ["t1"] }
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

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        issues.iter().any(|issue| issue
            .get("code")
            .and_then(|v| v.as_str())
            .is_some_and(|code| code == "MISSING_ANCHOR")),
        "expected MISSING_ANCHOR issue, got:\n{lint_text}"
    );

    let patches = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .expect("patches array");
    assert!(
        patches.iter().any(|patch| {
            let id_ok = patch
                .get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id.starts_with("patch:task:missing_anchor:attach:"));
            let tool_ok = patch
                .get("apply")
                .and_then(|v| v.get("tool"))
                .and_then(|v| v.as_str())
                == Some("macro_anchor_note");
            let target_ok = patch
                .get("apply")
                .and_then(|v| v.get("arguments"))
                .and_then(|v| v.get("target"))
                .and_then(|v| v.as_str())
                == Some(task_id);
            let visibility_ok = patch
                .get("apply")
                .and_then(|v| v.get("arguments"))
                .and_then(|v| v.get("visibility"))
                .and_then(|v| v.as_str())
                == Some("canon");
            id_ok && tool_ok && target_ok && visibility_ok
        }),
        "expected a macro_anchor_note patch suggestion for missing anchors, got:\n{lint_text}"
    );
}

#[test]
fn tasks_lint_patch_suggestions_active_tasks_missing_anchor_for_plan() {
    let mut server = Server::start_initialized(
        "tasks_lint_patch_suggestions_active_tasks_missing_anchor_for_plan",
    );

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Active Task Without Anchor",
                "steps": [
                    { "title": "Implement: thing", "success_criteria": ["c1"], "tests": ["t1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let plan_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("plan"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id");
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id");

    // Make it ACTIVE so plan coverage KPI is meaningful.
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "tasks_complete",
            "arguments": {
                "workspace": "ws1",
                "task": task_id,
                "status": "ACTIVE"
            }
        }
    }));

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "plan": plan_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        issues.iter().any(|issue| issue
            .get("code")
            .and_then(|v| v.as_str())
            .is_some_and(|code| code == "ACTIVE_TASKS_MISSING_ANCHOR")),
        "expected ACTIVE_TASKS_MISSING_ANCHOR issue, got:\n{lint_text}"
    );

    let patches = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .expect("patches array");
    assert!(
        patches.iter().any(|patch| {
            patch
                .get("apply")
                .and_then(|v| v.get("tool"))
                .and_then(|v| v.as_str())
                == Some("macro_anchor_note")
                && patch
                    .get("apply")
                    .and_then(|v| v.get("arguments"))
                    .and_then(|v| v.get("target"))
                    .and_then(|v| v.as_str())
                    == Some(task_id)
        }),
        "expected a macro_anchor_note patch suggestion for missing anchor ACTIVE tasks, got:\n{lint_text}"
    );
}

#[test]
fn tasks_lint_patch_suggestions_missing_next_action_and_can_patch() {
    let mut server =
        Server::start_initialized("tasks_lint_patch_suggestions_missing_next_action_and_can_patch");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task Without Next",
                "steps": [
                    { "title": "Implement: thing", "success_criteria": ["c1"], "tests": ["t1"] }
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
    let step_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("step id");

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        issues.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "MISSING_NEXT_ACTION")
        }),
        "expected MISSING_NEXT_ACTION issue, got:\n{lint_text}"
    );

    let patch = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .and_then(|patches| {
            patches.iter().find(|patch| {
                patch
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id.contains(":set_next_action"))
            })
        })
        .expect("set_next_action patch");
    assert_eq!(
        patch
            .get("apply")
            .and_then(|v| v.get("tool"))
            .and_then(|v| v.as_str()),
        Some("tasks_patch"),
        "expected patch apply tool tasks_patch, got:\n{lint_text}"
    );

    // Apply the suggested patch immediately and confirm the issue disappears (incremental refinement).
    let apply_args = patch
        .get("apply")
        .and_then(|v| v.get("arguments"))
        .cloned()
        .expect("apply arguments");
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_patch", "arguments": apply_args }
    }));

    let lint_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_after_text = extract_tool_text(&lint_after);
    let issues_after = lint_after_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        !issues_after.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "MISSING_NEXT_ACTION")
        }),
        "expected MISSING_NEXT_ACTION to be fixed after applying patch, got:\n{lint_after_text}"
    );

    // Also ensure the patch targeted the correct step.
    let patched_step_id = patch
        .get("apply")
        .and_then(|v| v.get("arguments"))
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("patched step_id");
    assert_eq!(
        patched_step_id, step_id,
        "expected next_action patch to target first step"
    );
}

#[test]
fn tasks_lint_patch_suggestions_research_missing_stop_criteria_and_can_patch() {
    let mut server = Server::start_initialized(
        "tasks_lint_patch_suggestions_research_missing_stop_criteria_and_can_patch",
    );

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Research Task",
                "steps": [
                    { "title": "Research: cache policy", "success_criteria": ["c1"], "tests": ["t1"] }
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

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        issues.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "RESEARCH_MISSING_STOP_CRITERIA")
        }),
        "expected RESEARCH_MISSING_STOP_CRITERIA issue, got:\n{lint_text}"
    );

    let patch = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .and_then(|patches| {
            patches.iter().find(|patch| {
                patch
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id.contains(":set_stop_criteria"))
            })
        })
        .expect("set_stop_criteria patch");
    assert_eq!(
        patch
            .get("apply")
            .and_then(|v| v.get("tool"))
            .and_then(|v| v.as_str()),
        Some("tasks_patch"),
        "expected patch apply tool tasks_patch, got:\n{lint_text}"
    );

    let apply_args = patch
        .get("apply")
        .and_then(|v| v.get("arguments"))
        .cloned()
        .expect("apply arguments");
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_patch", "arguments": apply_args }
    }));

    let lint_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_after_text = extract_tool_text(&lint_after);
    let issues_after = lint_after_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        !issues_after.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "RESEARCH_MISSING_STOP_CRITERIA")
        }),
        "expected stop criteria issue to be fixed after applying patch, got:\n{lint_after_text}"
    );
}

#[test]
fn tasks_lint_patch_suggestions_missing_proof_plan_and_can_patch() {
    let mut server =
        Server::start_initialized("tasks_lint_patch_suggestions_missing_proof_plan_and_can_patch");

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws1",
                "plan_title": "Plan Lint",
                "task_title": "Task Missing Proof Plan",
                "steps": [
                    { "title": "Implement: thing", "success_criteria": ["c1"], "tests": ["t1"] },
                    { "title": "Verify: thing", "success_criteria": ["c2"], "tests": ["t2"] }
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
    let verify_step_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("steps"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.get(1))
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("verify step id");

    let lint = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_text = extract_tool_text(&lint);

    let issues = lint_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        issues.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "MISSING_PROOF_PLAN")
        }),
        "expected MISSING_PROOF_PLAN issue, got:\n{lint_text}"
    );

    let patch = lint_text
        .get("result")
        .and_then(|v| v.get("patches"))
        .and_then(|v| v.as_array())
        .and_then(|patches| {
            patches.iter().find(|patch| {
                patch
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id.contains(":require_proof_tests"))
            })
        })
        .expect("require_proof_tests patch");
    let patched_step_id = patch
        .get("apply")
        .and_then(|v| v.get("arguments"))
        .and_then(|v| v.get("step_id"))
        .and_then(|v| v.as_str())
        .expect("patched step id");
    assert_eq!(
        patched_step_id, verify_step_id,
        "expected proof plan patch to prefer Verify step"
    );

    let apply_args = patch
        .get("apply")
        .and_then(|v| v.get("arguments"))
        .cloned()
        .expect("apply arguments");
    server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks_patch", "arguments": apply_args }
    }));

    let lint_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_lint", "arguments": { "workspace": "ws1", "task": task_id } }
    }));
    let lint_after_text = extract_tool_text(&lint_after);
    let issues_after = lint_after_text
        .get("result")
        .and_then(|v| v.get("issues"))
        .and_then(|v| v.as_array())
        .expect("issues array");
    assert!(
        !issues_after.iter().any(|issue| {
            issue
                .get("code")
                .and_then(|v| v.as_str())
                .is_some_and(|code| code == "MISSING_PROOF_PLAN")
        }),
        "expected MISSING_PROOF_PLAN to be fixed after applying patch, got:\n{lint_after_text}"
    );
}
