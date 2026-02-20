#![forbid(unsafe_code)]

mod support;

use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use support::*;

fn temp_repo_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_planfs_repo_{test_name}_{nonce}"));
    fs::create_dir_all(&dir).expect("create temp repo dir");
    dir
}

fn call_portal(server: &mut Server, name: &str, cmd: &str, args: Value) -> Value {
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": { "op": "call", "cmd": cmd, "args": args } }
    }));
    extract_tool_text(&resp)
}

fn call_tasks(server: &mut Server, cmd: &str, args: Value) -> Value {
    call_portal(server, "tasks", cmd, args)
}

fn call_docs(server: &mut Server, cmd: &str, args: Value) -> Value {
    call_portal(server, "docs", cmd, args)
}

fn call_vcs(server: &mut Server, cmd: &str, args: Value) -> Value {
    call_portal(server, "vcs", cmd, args)
}

fn call_jobs(server: &mut Server, cmd: &str, args: Value) -> Value {
    call_portal(server, "jobs", cmd, args)
}

fn parse_workspace_from_status_text(text: &str) -> Option<String> {
    let first = text.lines().next()?.trim();
    let marker = "workspace=";
    let idx = first.find(marker)?;
    let rest = &first[idx + marker.len()..];
    let value = rest.split_whitespace().next()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[test]
fn planfs_init_export_import_strict_and_idempotent() {
    let repo_root = temp_repo_dir("planfs_init_export_import_strict_and_idempotent");
    let repo_root_str = repo_root.to_str().expect("repo_root to str").to_string();

    let mut server = Server::start_initialized_with_args(
        "planfs_init_export_import_strict_and_idempotent",
        &["--toolset", "daily"],
    );

    let status_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": repo_root_str } }
    }));
    let status_out = extract_tool_text(&status_resp);
    let status_text = status_out.as_str().expect("status line text");
    let workspace = parse_workspace_from_status_text(status_text).expect("workspace from status");

    let plan_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "plan",
            "title": "PlanFS Smoke Plan"
        }),
    );
    assert!(
        plan_create
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "create plan must succeed: {plan_create}"
    );
    let plan_id = plan_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let task_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "task",
            "parent": plan_id,
            "title": "PlanFS Smoke Task",
            "description": "Validate planfs init/export/import strict loop."
        }),
    );
    assert!(
        task_create
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "create task must succeed: {task_create}"
    );
    let task_id = task_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let root_steps = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "steps": [
                { "title": "Slice-1: Root", "success_criteria": ["SC 1.1", "SC 1.2"] },
                { "title": "Slice-2: Root", "success_criteria": ["SC 2.1", "SC 2.2"] }
            ]
        }),
    );
    assert!(
        root_steps
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "decompose root must succeed: {root_steps}"
    );

    for path in ["s:0", "s:1"] {
        let define = call_tasks(
            &mut server,
            "tasks.define",
            json!({
                "workspace": workspace,
                "task": task_id,
                "path": path,
                "tests": [format!("make check // {path}")],
                "blockers": [format!("No blockers for {path}")]
            }),
        );
        assert!(
            define
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            "define must succeed for {path}: {define}"
        );
    }

    let child_s0 = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "parent": "s:0",
            "steps": [
                { "title": "Task 1", "success_criteria": ["Implement A", "Verify A"] }
            ]
        }),
    );
    assert!(
        child_s0
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "decompose child s:0 must succeed: {child_s0}"
    );

    let child_s1 = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "parent": "s:1",
            "steps": [
                { "title": "Task 2", "success_criteria": ["Implement B", "Verify B"] }
            ]
        }),
    );
    assert!(
        child_s1
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "decompose child s:1 must succeed: {child_s1}"
    );

    let init = call_tasks(
        &mut server,
        "tasks.planfs.init",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-smoke"
        }),
    );
    assert!(
        init.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "planfs init must succeed: {init}"
    );

    let plan_dir = repo_root.join("docs").join("plans").join("planfs-smoke");
    let plan_file = plan_dir.join("PLAN.md");
    let slice1_file = plan_dir.join("Slice-1.md");
    let slice2_file = plan_dir.join("Slice-2.md");
    assert!(plan_file.exists(), "PLAN.md must exist");
    assert!(slice1_file.exists(), "Slice-1.md must exist");
    assert!(slice2_file.exists(), "Slice-2.md must exist");

    let export = call_tasks(
        &mut server,
        "tasks.planfs.export",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-smoke"
        }),
    );
    assert!(
        export
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "planfs export must succeed: {export}"
    );
    let unchanged = export
        .get("result")
        .and_then(|v| v.get("write"))
        .and_then(|v| v.get("unchanged"))
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    assert!(
        unchanged >= 3,
        "idempotent export should keep files unchanged: {export}"
    );

    let original_slice = fs::read_to_string(&slice1_file).expect("read slice1");
    let broken_slice = original_slice.replacen("## Tests\n- ", "## Tests\n- TODO\n- ", 1);
    fs::write(&slice1_file, broken_slice).expect("write broken slice1");

    let import_fail = call_tasks(
        &mut server,
        "tasks.planfs.import",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-smoke",
            "apply": false,
            "strict": true
        }),
    );
    assert!(
        !import_fail
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "strict import must fail on placeholder: {import_fail}"
    );
    assert_eq!(
        import_fail
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "INVALID_INPUT",
        "strict placeholder gate must return INVALID_INPUT: {import_fail}"
    );

    let _restore = call_tasks(
        &mut server,
        "tasks.planfs.export",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-smoke",
            "overwrite": true
        }),
    );

    let import_ok = call_tasks(
        &mut server,
        "tasks.planfs.import",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-smoke",
            "apply": true,
            "strict": true
        }),
    );
    assert!(
        import_ok
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "strict import apply must succeed after restore: {import_ok}"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn planfs_export_uses_parent_plan_metadata_and_step_statuses() {
    let repo_root = temp_repo_dir("planfs_export_uses_parent_plan_metadata_and_step_statuses");
    let repo_root_str = repo_root.to_str().expect("repo_root to str").to_string();

    let mut server = Server::start_initialized_with_args(
        "planfs_export_uses_parent_plan_metadata_and_step_statuses",
        &["--toolset", "daily"],
    );

    let status_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": repo_root_str } }
    }));
    let status_out = extract_tool_text(&status_resp);
    let status_text = status_out.as_str().expect("status line text");
    let workspace = parse_workspace_from_status_text(status_text).expect("workspace from status");

    let plan_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "plan",
            "title": "Parent Plan Title"
        }),
    );
    assert_eq!(
        plan_create.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "create plan must succeed: {plan_create}"
    );
    let plan_id = plan_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let plan_edit = call_tasks(
        &mut server,
        "tasks.edit",
        json!({
            "workspace": workspace,
            "target": { "kind": "plan", "id": plan_id },
            "description": "Parent plan objective from plan card.",
            "context": "{\"tasks\":[\"noise\"],\"dod\":{}}\nPlan-level constraint: keep docs and contracts aligned."
        }),
    );
    assert_eq!(
        plan_edit.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "edit plan metadata must succeed: {plan_edit}"
    );

    let task_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "task",
            "parent": plan_id,
            "title": "Child task title should not leak into PLAN.md",
            "description": "Task-local objective should be overridden by parent plan objective."
        }),
    );
    assert_eq!(
        task_create.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "create task must succeed: {task_create}"
    );
    let task_id = task_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let task_edit = call_tasks(
        &mut server,
        "tasks.edit",
        json!({
            "workspace": workspace,
            "task": task_id,
            "context": "{\"budgets\":{\"max_files\":12},\"shared_context_refs\":[\"PLAN:noise\"],\"tasks\":[1]}\nTask-level human constraint should be used only when parent plan context is absent."
        }),
    );
    assert_eq!(
        task_edit.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "edit task context must succeed: {task_edit}"
    );

    let root_steps = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "steps": [
                { "title": "Slice-1 Root", "success_criteria": ["SC 1.1", "SC 1.2"] },
                { "title": "Slice-2 Root", "success_criteria": ["SC 2.1", "SC 2.2"] }
            ]
        }),
    );
    assert_eq!(
        root_steps.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "decompose root steps must succeed: {root_steps}"
    );

    let mark_done = call_tasks(
        &mut server,
        "tasks.progress",
        json!({
            "workspace": workspace,
            "task": task_id,
            "path": "s:0",
            "completed": true,
            "force": true
        }),
    );
    assert_eq!(
        mark_done.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "marking first slice root completed must succeed: {mark_done}"
    );

    let export = call_tasks(
        &mut server,
        "tasks.planfs.export",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-parent-meta",
            "overwrite": true
        }),
    );
    assert_eq!(
        export.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "planfs export must succeed: {export}"
    );

    let plan_file = repo_root
        .join("docs")
        .join("plans")
        .join("planfs-parent-meta")
        .join("PLAN.md");
    let slice1_file = repo_root
        .join("docs")
        .join("plans")
        .join("planfs-parent-meta")
        .join("Slice-1.md");
    let plan_text = fs::read_to_string(&plan_file).expect("read PLAN.md");
    let slice1_text = fs::read_to_string(&slice1_file).expect("read Slice-1.md");

    assert!(
        plan_text.contains("title: Parent Plan Title"),
        "PLAN.md should use parent plan title: {plan_text}"
    );
    assert!(
        plan_text.contains("objective: Parent plan objective from plan card."),
        "PLAN.md should use parent plan objective: {plan_text}"
    );
    assert!(
        plan_text.contains("Plan-level constraint: keep docs and contracts aligned."),
        "PLAN.md should keep human-readable plan constraint: {plan_text}"
    );
    assert!(
        !plan_text.contains("\"shared_context_refs\"")
            && !plan_text.contains("\"budgets\"")
            && !plan_text.contains("\"tasks\""),
        "PLAN.md constraints must not leak machine JSON blobs: {plan_text}"
    );
    assert!(
        plan_text.contains("status: done") && plan_text.contains("status: todo"),
        "PLAN.md should reflect mixed slice statuses from step completion: {plan_text}"
    );
    assert!(
        slice1_text.contains("status: done"),
        "Slice-1.md should be marked done when root step is completed: {slice1_text}"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn planfs_plan_spec_branch_merge_and_export_roundtrip() {
    let repo_root = temp_repo_dir("planfs_plan_spec_branch_merge_and_export_roundtrip");
    let repo_root_str = repo_root.to_str().expect("repo_root to str").to_string();

    let mut server = Server::start_initialized_with_args(
        "planfs_plan_spec_branch_merge_and_export_roundtrip",
        &["--toolset", "daily"],
    );

    let status_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": repo_root_str } }
    }));
    let status_out = extract_tool_text(&status_resp);
    let status_text = status_out.as_str().expect("status line text");
    let workspace = parse_workspace_from_status_text(status_text).expect("workspace from status");

    let plan_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({ "workspace": workspace, "kind": "plan", "title": "PlanFS PlanSpec Plan" }),
    );
    let plan_id = plan_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let task_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "task",
            "parent": plan_id,
            "title": "PlanFS PlanSpec Task",
            "description": "Validate plan_spec docs branching/merge/export loop."
        }),
    );
    let task_id = task_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let root_steps = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "steps": [
                { "title": "Slice-1: Root", "success_criteria": ["SC 1.1", "SC 1.2"] },
                { "title": "Slice-2: Root", "success_criteria": ["SC 2.1", "SC 2.2"] }
            ]
        }),
    );
    assert!(
        root_steps
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "decompose root must succeed: {root_steps}"
    );

    for path in ["s:0", "s:1"] {
        let define = call_tasks(
            &mut server,
            "tasks.define",
            json!({
                "workspace": workspace,
                "task": task_id,
                "path": path,
                "tests": [format!("make check // {path}")],
                "blockers": [format!("No blockers for {path}")]
            }),
        );
        assert!(
            define
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            "define must succeed for {path}: {define}"
        );
    }

    let _ = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "parent": "s:0",
            "steps": [
                { "title": "Task 1", "success_criteria": ["Implement A", "Verify A"] }
            ]
        }),
    );
    let _ = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "parent": "s:1",
            "steps": [
                { "title": "Task 2", "success_criteria": ["Implement B", "Verify B"] }
            ]
        }),
    );

    let init = call_tasks(
        &mut server,
        "tasks.planfs.init",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-plan-spec"
        }),
    );
    assert_eq!(
        init.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "planfs init must succeed: {init}"
    );
    let plan_spec_status = init
        .get("result")
        .and_then(|v| v.get("plan_spec"))
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        matches!(plan_spec_status, "appended" | "unchanged"),
        "init should persist plan_spec snapshot: {init}"
    );

    let radar = call_tasks(
        &mut server,
        "tasks.radar",
        json!({ "workspace": workspace, "task": task_id }),
    );
    let canonical_branch = radar
        .get("result")
        .and_then(|v| v.get("reasoning_ref"))
        .and_then(|v| v.get("branch"))
        .and_then(|v| v.as_str())
        .expect("reasoning branch")
        .to_string();
    let plan_spec_doc = format!("plan_spec:{task_id}");
    let derived_branch = format!("{canonical_branch}/spec-alt");

    let branch_create = call_vcs(
        &mut server,
        "vcs.branch.create",
        json!({ "workspace": workspace, "name": derived_branch }),
    );
    assert_eq!(
        branch_create.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "branch create must succeed: {branch_create}"
    );

    let diff_before = call_docs(
        &mut server,
        "docs.diff",
        json!({
            "workspace": workspace,
            "from": canonical_branch,
            "to": derived_branch,
            "doc_kind": "plan_spec",
            "doc": plan_spec_doc
        }),
    );
    assert_eq!(
        diff_before.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "plan_spec diff before merge must succeed: {diff_before}"
    );
    let status_before = diff_before
        .get("result")
        .and_then(|v| v.get("plan_spec_diff"))
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        status_before, "missing_to",
        "derived branch should not have plan_spec before merge: {diff_before}"
    );

    let merge = call_docs(
        &mut server,
        "docs.merge",
        json!({
            "workspace": workspace,
            "from": canonical_branch,
            "into": derived_branch,
            "doc_kind": "plan_spec",
            "doc": plan_spec_doc
        }),
    );
    assert_eq!(
        merge.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "plan_spec merge must succeed: {merge}"
    );
    let merged = merge
        .get("result")
        .and_then(|v| v.get("merged"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        merged, 1,
        "plan_spec first merge should merge one entry: {merge}"
    );

    let diff_after = call_docs(
        &mut server,
        "docs.diff",
        json!({
            "workspace": workspace,
            "from": canonical_branch,
            "to": derived_branch,
            "doc_kind": "plan_spec",
            "doc": plan_spec_doc
        }),
    );
    let status_after = diff_after
        .get("result")
        .and_then(|v| v.get("plan_spec_diff"))
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        status_after, "identical",
        "plan_spec diff must become identical after merge: {diff_after}"
    );

    let export_from_spec = call_tasks(
        &mut server,
        "tasks.planfs.export",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-plan-spec",
            "overwrite": true,
            "from_plan_spec": true,
            "plan_spec_branch": derived_branch,
            "plan_spec_doc": plan_spec_doc
        }),
    );
    assert_eq!(
        export_from_spec.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "export from merged plan_spec must succeed: {export_from_spec}"
    );
    assert_eq!(
        export_from_spec
            .get("result")
            .and_then(|v| v.get("source"))
            .and_then(|v| v.as_str()),
        Some("plan_spec_doc"),
        "export source marker must indicate plan_spec mode: {export_from_spec}"
    );
    let unchanged = export_from_spec
        .get("result")
        .and_then(|v| v.get("write"))
        .and_then(|v| v.get("unchanged"))
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    assert!(
        unchanged >= 3,
        "plan_spec export should be deterministic and keep files unchanged: {export_from_spec}"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn jobs_dispatch_accepts_planfs_target_ref_and_surfaces_bounded_excerpt() {
    let repo_root =
        temp_repo_dir("jobs_dispatch_accepts_planfs_target_ref_and_surfaces_bounded_excerpt");
    let repo_root_str = repo_root.to_str().expect("repo_root to str").to_string();

    let mut server = Server::start_initialized_with_args(
        "jobs_dispatch_accepts_planfs_target_ref_and_surfaces_bounded_excerpt",
        &["--toolset", "daily"],
    );

    let status_resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "status", "arguments": { "workspace": repo_root_str } }
    }));
    let status_out = extract_tool_text(&status_resp);
    let status_text = status_out.as_str().expect("status line text");
    let workspace = parse_workspace_from_status_text(status_text).expect("workspace from status");

    let plan_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "plan",
            "title": "PlanFS Dispatch Plan"
        }),
    );
    let plan_id = plan_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let task_create = call_tasks(
        &mut server,
        "tasks.plan.create",
        json!({
            "workspace": workspace,
            "kind": "task",
            "parent": plan_id,
            "title": "PlanFS Dispatch Task"
        }),
    );
    let task_id = task_create
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let _ = call_tasks(
        &mut server,
        "tasks.plan.decompose",
        json!({
            "workspace": workspace,
            "task": task_id,
            "steps": [
                { "title": "Slice-1: Root", "success_criteria": ["SC 1.1", "SC 1.2"] },
                { "title": "Slice-2: Root", "success_criteria": ["SC 2.1", "SC 2.2"] }
            ]
        }),
    );
    for path in ["s:0", "s:1"] {
        let _ = call_tasks(
            &mut server,
            "tasks.define",
            json!({
                "workspace": workspace,
                "task": task_id,
                "path": path,
                "tests": [format!("make check // {path}")],
                "blockers": [format!("No blockers for {path}")]
            }),
        );
    }

    let init = call_tasks(
        &mut server,
        "tasks.planfs.init",
        json!({
            "workspace": workspace,
            "task": task_id,
            "slug": "planfs-dispatch"
        }),
    );
    assert_eq!(
        init.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "planfs init must succeed before target_ref dispatch: {init}"
    );

    let scout = call_jobs(
        &mut server,
        "jobs.macro.dispatch.scout",
        json!({
            "workspace": workspace,
            "task": task_id,
            "anchor": "a:planfs-target",
            "target_ref": "planfs:planfs-dispatch#SLICE-1",
            "executor": "codex",
            "executor_profile": "xhigh",
            "model": "gpt-5.3-codex",
            "dry_run": true
        }),
    );
    assert_eq!(
        scout.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "scout dispatch with target_ref must succeed: {scout}"
    );
    let scout_excerpt_chars = scout
        .get("result")
        .and_then(|v| v.get("planfs"))
        .and_then(|v| v.get("excerpt_chars"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        scout_excerpt_chars > 0 && scout_excerpt_chars <= 1200,
        "planfs excerpt must be bounded and non-empty: {scout}"
    );

    let builder = call_jobs(
        &mut server,
        "jobs.macro.dispatch.builder",
        json!({
            "workspace": workspace,
            "task": task_id,
            "target_ref": "planfs:planfs-dispatch#SLICE-1",
            "scout_pack_ref": "artifact://jobs/JOB-000001/scout_context_pack",
            "dry_run": true
        }),
    );
    assert_eq!(
        builder
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ID"),
        "builder must accept target_ref and move to scout lineage validation: {builder}"
    );

    let validator = call_jobs(
        &mut server,
        "jobs.macro.dispatch.validator",
        json!({
            "workspace": workspace,
            "task": task_id,
            "target_ref": "planfs:planfs-dispatch#SLICE-1",
            "scout_pack_ref": "artifact://jobs/JOB-000001/scout_context_pack",
            "builder_batch_ref": "artifact://jobs/JOB-000002/builder_diff_batch",
            "dry_run": true
        }),
    );
    assert_eq!(
        validator.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "validator dry-run must accept target_ref shape: {validator}"
    );
    let _ = fs::remove_dir_all(&repo_root);
}
