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

fn new_server(store: SqliteStore) -> crate::McpServer {
    let runner_autostart_enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let runner_autostart_state =
        std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
    crate::McpServer::new(
        store,
        crate::McpServerConfig {
            toolset: crate::Toolset::Core,
            response_verbosity: crate::ResponseVerbosity::Full,
            dx_mode: false,
            ux_proof_v2_enabled: true,
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
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
    )
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
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
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
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
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
fn open_code_ref_reads_repo_lines() {
    let base = temp_dir();
    let repo_root = base.join("repo");
    fs::create_dir_all(repo_root.join(".git")).unwrap();
    fs::create_dir_all(repo_root.join("src")).unwrap();
    fs::write(repo_root.join("src/lib.rs"), "line1\nline2\nline3\nline4\n").unwrap();

    let storage_dir = repo_root.join(".agents").join("mcp").join(".branchmind");
    let mut store = SqliteStore::open(&storage_dir).unwrap();
    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    store.workspace_init(&workspace).unwrap();

    let mut server = new_server(store);
    let resp = server.tool_branchmind_open(json!({
        "id": "code:src/lib.rs#L2-L3"
    }));

    assert!(
        resp.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    );
    let result = resp.get("result").cloned().unwrap_or_default();
    assert_eq!(result.get("kind").and_then(|v| v.as_str()), Some("code"));
    let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("");
    assert!(content.contains("    2 | line2"));
    assert!(content.contains("    3 | line3"));
    let code_ref = result.get("ref").and_then(|v| v.as_str()).unwrap_or("");
    assert!(code_ref.starts_with("code:src/lib.rs#L2-L3@sha256:"));

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn open_uses_default_workspace_when_omitted() {
    let base = temp_dir();
    let repo_root = base.join("repo");
    fs::create_dir_all(repo_root.join(".git")).unwrap();
    fs::create_dir_all(repo_root.join("src")).unwrap();
    fs::write(repo_root.join("src/lib.rs"), "x\ny\nz\n").unwrap();

    let storage_dir = repo_root.join(".agents").join("mcp").join(".branchmind");
    let mut store = SqliteStore::open(&storage_dir).unwrap();
    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    store.workspace_init(&workspace).unwrap();

    let mut server = new_server(store);
    let resp = server.tool_branchmind_open(json!({
        "id": "code:src/lib.rs#L1-L2"
    }));

    assert!(
        resp.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    );
    let result = resp.get("result").cloned().unwrap_or_default();
    assert_eq!(result.get("kind").and_then(|v| v.as_str()), Some("code"));

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn open_code_ref_warns_on_stale_sha() {
    let base = temp_dir();
    let repo_root = base.join("repo");
    fs::create_dir_all(repo_root.join(".git")).unwrap();
    fs::create_dir_all(repo_root.join("src")).unwrap();
    fs::write(repo_root.join("src/lib.rs"), "a\nb\nc\n").unwrap();

    let storage_dir = repo_root.join(".agents").join("mcp").join(".branchmind");
    let mut store = SqliteStore::open(&storage_dir).unwrap();
    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    store.workspace_init(&workspace).unwrap();

    let mut server = new_server(store);
    let stale_ref = format!("code:src/lib.rs#L1-L2@sha256:{}", "0".repeat(64));
    let resp = server.tool_branchmind_open(json!({
        "workspace": "demo",
        "id": stale_ref
    }));

    assert!(
        resp.get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    );
    let warnings = resp
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.get("code").and_then(|v| v.as_str()) == Some("CODE_REF_STALE"))
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn open_code_ref_rejects_traversal() {
    let base = temp_dir();
    let repo_root = base.join("repo");
    fs::create_dir_all(repo_root.join(".git")).unwrap();
    let storage_dir = repo_root.join(".agents").join("mcp").join(".branchmind");
    let mut store = SqliteStore::open(&storage_dir).unwrap();
    let workspace = crate::WorkspaceId::try_new("demo".to_string()).unwrap();
    store.workspace_init(&workspace).unwrap();

    let mut server = new_server(store);
    let resp = server.tool_branchmind_open(json!({
        "workspace": "demo",
        "id": "code:../secrets.txt#L1-L2"
    }));

    assert!(
        !resp
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    );
    let code = resp
        .get("error")
        .and_then(|err| err.get("code"))
        .and_then(|v| v.as_str());
    assert_eq!(code, Some("INVALID_INPUT"));

    let _ = fs::remove_dir_all(&base);
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
            knowledge_autolint_enabled: true,
            note_promote_enabled: true,
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
