#![forbid(unsafe_code)]

use bm_storage::SqliteStore;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("bm_project_guard_test_{nanos}"));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn project_guard_mismatch_errors_when_rebind_disabled() {
    let dir = temp_dir();
    let mut store = SqliteStore::open(&dir).unwrap();
    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    store.workspace_init(&workspace).unwrap();
    store
        .workspace_project_guard_ensure(&workspace, "repo:aaaaaaaaaaaaaaaa")
        .unwrap();

    let runner_autostart_enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runner_autostart_state =
        std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
    let mut server = crate::McpServer::new(
        store,
        crate::McpServerConfig {
            toolset: crate::Toolset::Core,
            response_verbosity: crate::ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            jobs_unknown_args_fail_closed_enabled: true,
            jobs_strict_progress_schema_enabled: true,
            jobs_high_done_proof_gate_enabled: true,
            jobs_wait_stream_v2_enabled: true,
            jobs_mesh_v1_enabled: true,
            slice_plans_v1_enabled: true,
            jobs_slice_first_fail_closed_enabled: true,
            slice_budgets_enforced_enabled: true,
            default_workspace: Some("demo".to_string()),
            workspace_explicit: false,
            workspace_allowlist: None,
            workspace_lock: true,
            project_guard: Some("repo:bbbbbbbbbbbbbbbb".to_string()),
            project_guard_rebind_enabled: false,
            default_agent_id: None,
            runner_autostart_enabled,
            runner_autostart_dry_run: false,
            runner_autostart: runner_autostart_state,
        },
    );

    let resp = server.enforce_project_guard(&workspace);
    assert!(resp.is_some());
    let code = resp
        .and_then(|value| value.get("error").and_then(|err| err.get("code")).cloned())
        .and_then(|value| value.as_str().map(|s| s.to_string()));
    assert_eq!(code.as_deref(), Some("PROJECT_GUARD_MISMATCH"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_guard_mismatch_rebinds_when_enabled() {
    let dir = temp_dir();
    let mut store = SqliteStore::open(&dir).unwrap();
    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    store.workspace_init(&workspace).unwrap();
    store
        .workspace_project_guard_ensure(&workspace, "repo:aaaaaaaaaaaaaaaa")
        .unwrap();

    let runner_autostart_enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runner_autostart_state =
        std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
    let mut server = crate::McpServer::new(
        store,
        crate::McpServerConfig {
            toolset: crate::Toolset::Core,
            response_verbosity: crate::ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            jobs_unknown_args_fail_closed_enabled: true,
            jobs_strict_progress_schema_enabled: true,
            jobs_high_done_proof_gate_enabled: true,
            jobs_wait_stream_v2_enabled: true,
            jobs_mesh_v1_enabled: true,
            slice_plans_v1_enabled: true,
            jobs_slice_first_fail_closed_enabled: true,
            slice_budgets_enforced_enabled: true,
            default_workspace: Some("demo".to_string()),
            workspace_explicit: false,
            workspace_allowlist: None,
            workspace_lock: true,
            project_guard: Some("repo:bbbbbbbbbbbbbbbb".to_string()),
            project_guard_rebind_enabled: true,
            default_agent_id: None,
            runner_autostart_enabled,
            runner_autostart_dry_run: false,
            runner_autostart: runner_autostart_state,
        },
    );

    let resp = server.enforce_project_guard(&workspace);
    assert!(resp.is_none());
    let stored = server
        .store
        .workspace_project_guard_get(&workspace)
        .unwrap()
        .unwrap();
    assert_eq!(stored, "repo:bbbbbbbbbbbbbbbb");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn budget_profiles_can_avoid_truncation_warnings_for_large_reads() {
    // v1 UX uses explicit budget profiles instead of hiding 100+ tools. This test ensures
    // that large reads can be made "untruncated" deterministically by selecting a larger
    // budget profile (without manual max_chars guessing).
    let dir = temp_dir();
    let store = SqliteStore::open(&dir).unwrap();

    let runner_autostart_enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runner_autostart_state =
        std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
    let mut server = crate::McpServer::new(
        store,
        crate::McpServerConfig {
            toolset: crate::Toolset::Daily,
            response_verbosity: crate::ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            jobs_unknown_args_fail_closed_enabled: true,
            jobs_strict_progress_schema_enabled: true,
            jobs_high_done_proof_gate_enabled: true,
            jobs_wait_stream_v2_enabled: true,
            jobs_mesh_v1_enabled: true,
            slice_plans_v1_enabled: true,
            jobs_slice_first_fail_closed_enabled: true,
            slice_budgets_enforced_enabled: true,
            default_workspace: Some("demo".to_string()),
            workspace_explicit: false,
            workspace_allowlist: None,
            workspace_lock: true,
            project_guard: None,
            project_guard_rebind_enabled: false,
            default_agent_id: None,
            runner_autostart_enabled,
            runner_autostart_dry_run: false,
            runner_autostart: runner_autostart_state,
        },
    );

    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    server.store.workspace_init(&workspace).unwrap();

    // Create enough anchors so the portal budget would truncate, but audit should not.
    let title = "T".repeat(120);
    let desc = "x".repeat(280);
    for i in 0..80 {
        let id = format!("a:test-{i:03}");
        server
            .store
            .anchor_upsert(
                &workspace,
                bm_storage::AnchorUpsertRequest {
                    id,
                    title: title.clone(),
                    kind: "ops".to_string(),
                    description: Some(desc.clone()),
                    refs: Vec::new(),
                    aliases: Vec::new(),
                    parent_id: None,
                    depends_on: Vec::new(),
                    status: "active".to_string(),
                },
            )
            .unwrap();
    }

    let resp = server.call_tool(
        "think",
        json!({
            "workspace": "demo",
            "op": "call",
            "cmd": "think.anchor.list",
            "args": { "limit": 80 },
            "budget_profile": "audit",
            "view": "compact"
        }),
    );
    assert_eq!(
        resp.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "expected think(cmd=think.anchor.list) to succeed, got: {resp}"
    );

    let warnings = resp
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings.iter().all(|w| {
            let code = w.get("code").and_then(|v| v.as_str());
            code != Some("BUDGET_TRUNCATED") && code != Some("BUDGET_MINIMAL")
        }),
        "expected no budget truncation warnings under budget_profile=audit, got: {warnings:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}
