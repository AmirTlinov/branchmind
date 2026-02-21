#![forbid(unsafe_code)]

use super::markdown::parse_tool_markdown;
use bm_core::{ThoughtBranch, ThoughtCommit};
use bm_storage::{AppendCommitRequest, ListBranchesRequest, ShowCommitRequest, StoreError};
use serde_json::{Value, json};

use crate::McpServer;

pub(crate) fn handle(server: &mut McpServer, args: Value) -> Value {
    let parsed =
        match parse_tool_markdown(args, "think", &["commit", "log", "show", "delete", "amend"]) {
            Ok(v) => v,
            Err(err) => return err,
        };

    match parsed.command.verb.as_str() {
        "commit" => handle_commit(server, &parsed.workspace, &parsed.command),
        "log" => handle_log(server, &parsed.workspace, &parsed.command),
        "show" => handle_show(server, &parsed.workspace, &parsed.command),
        "delete" => handle_delete(server, &parsed.workspace, &parsed.command),
        "amend" => handle_amend(server, &parsed.workspace, &parsed.command),
        _ => crate::ai_error_with(
            "UNKNOWN_VERB",
            "Unsupported think verb",
            Some("Use one of: commit, log, show, delete, amend."),
            Vec::new(),
        ),
    }
}

fn handle_commit(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    if let Err(err) =
        command.reject_unknown_args(&["branch", "commit", "message", "body", "parent"])
    {
        return err;
    }

    let branch_id = match command.require_arg("branch") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let commit_id = match command.require_arg("commit") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let message = match command.require_arg("message") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let body = command
        .optional_arg("body")
        .map(ToOwned::to_owned)
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            if command.body.is_empty() {
                None
            } else {
                Some(command.body.clone())
            }
        })
        .unwrap_or_else(|| message.clone());

    let parent_commit_id = command.optional_arg("parent").map(ToOwned::to_owned);
    let request = AppendCommitRequest {
        workspace_id: workspace.to_string(),
        branch_id,
        commit_id,
        parent_commit_id,
        message,
        body,
        created_at_ms: crate::now_ms_i64(),
    };

    match server.store.append_commit(request) {
        Ok(commit) => crate::ai_ok("think.commit", json!({ "commit": commit_to_json(&commit) })),
        Err(err) => map_store_error(err),
    }
}

fn handle_log(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    if let Err(err) = command.reject_unknown_args(&["branch", "limit", "offset", "from"]) {
        return err;
    }

    let branch_id = match command.require_arg("branch") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let limit = match command.optional_usize_arg("limit", 20) {
        Ok(v) => v.min(200),
        Err(err) => return err,
    };
    let offset = match command.optional_usize_arg("offset", 0) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let branch = match find_branch_by_id(server, workspace, &branch_id) {
        Ok(Some(branch)) => branch,
        Ok(None) => {
            return crate::ai_error_with(
                "UNKNOWN_ID",
                "Unknown branch",
                Some("Create the branch first or check branch list."),
                Vec::new(),
            );
        }
        Err(err) => return map_store_error(err),
    };

    let mut cursor = command
        .optional_arg("from")
        .map(ToOwned::to_owned)
        .or_else(|| branch.head_commit_id().map(ToOwned::to_owned));
    let mut seen = std::collections::BTreeSet::new();
    let mut skipped = 0usize;
    let mut commits = Vec::new();
    let mut truncated = false;

    while let Some(commit_id) = cursor.clone() {
        if commits.len() >= limit {
            truncated = true;
            break;
        }

        if !seen.insert(commit_id.clone()) {
            return crate::ai_error_with(
                "STORE_ERROR",
                "commit history loop detected",
                Some("Inspect commit parent links for corruption."),
                Vec::new(),
            );
        }

        let commit = match server.store.show_commit(ShowCommitRequest {
            workspace_id: workspace.to_string(),
            commit_id: commit_id.clone(),
        }) {
            Ok(Some(v)) => v,
            Ok(None) => {
                return crate::ai_error_with(
                    "UNKNOWN_ID",
                    &format!("Unknown commit: {commit_id}"),
                    Some("Use think show to verify commit ids."),
                    Vec::new(),
                );
            }
            Err(err) => return map_store_error(err),
        };

        cursor = commit.parent_commit_id().map(ToOwned::to_owned);

        if skipped < offset {
            skipped += 1;
            continue;
        }
        commits.push(commit_to_json(&commit));
    }

    let mut result = json!({
        "workspace": workspace,
        "branch": branch_id,
        "limit": limit,
        "offset": offset,
        "items": commits,
        "next_commit_id": cursor,
    });
    if truncated && let Some(obj) = result.as_object_mut() {
        obj.insert("truncated".to_string(), Value::Bool(true));
    }
    crate::ai_ok("think.log", result)
}

fn find_branch_by_id(
    server: &McpServer,
    workspace: &str,
    branch_id: &str,
) -> Result<Option<ThoughtBranch>, StoreError> {
    const PAGE_SIZE: usize = 1024;
    let mut offset = 0usize;

    loop {
        let page = server.store.list_branches(ListBranchesRequest {
            workspace_id: workspace.to_string(),
            limit: PAGE_SIZE,
            offset,
        })?;
        if let Some(found) = page.iter().find(|branch| branch.branch_id() == branch_id) {
            return Ok(Some(found.clone()));
        }
        if page.len() < PAGE_SIZE {
            return Ok(None);
        }
        offset = offset.saturating_add(PAGE_SIZE);
    }
}

fn handle_show(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    if let Err(err) = command.reject_unknown_args(&["commit"]) {
        return err;
    }

    let commit_id = match command.require_arg("commit") {
        Ok(v) => v,
        Err(err) => return err,
    };
    match server.store.show_commit(ShowCommitRequest {
        workspace_id: workspace.to_string(),
        commit_id: commit_id.clone(),
    }) {
        Ok(Some(commit)) => {
            crate::ai_ok("think.show", json!({ "commit": commit_to_json(&commit) }))
        }
        Ok(None) => crate::ai_error_with(
            "UNKNOWN_ID",
            &format!("Unknown commit: {commit_id}"),
            Some("Call think log to discover commits on a branch."),
            Vec::new(),
        ),
        Err(err) => map_store_error(err),
    }
}

fn handle_amend(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    if let Err(err) =
        command.reject_unknown_args(&["commit", "new_commit", "branch", "message", "body"])
    {
        return err;
    }

    let source_commit_id = match command.require_arg("commit") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let new_commit_id = match command.require_arg("new_commit") {
        Ok(v) => v,
        Err(err) => return err,
    };

    let Some(source_commit) = (match server.store.show_commit(ShowCommitRequest {
        workspace_id: workspace.to_string(),
        commit_id: source_commit_id.clone(),
    }) {
        Ok(v) => v,
        Err(err) => return map_store_error(err),
    }) else {
        return crate::ai_error_with(
            "UNKNOWN_ID",
            &format!("Unknown commit: {source_commit_id}"),
            Some("Call think log to discover existing commits."),
            Vec::new(),
        );
    };

    let branch_id = command
        .optional_arg("branch")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| source_commit.branch_id().to_string());
    let message = command
        .optional_arg("message")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| source_commit.message().to_string());
    let body = command
        .optional_arg("body")
        .map(ToOwned::to_owned)
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            if command.body.is_empty() {
                None
            } else {
                Some(command.body.clone())
            }
        })
        .unwrap_or_else(|| source_commit.body().to_string());

    let request = AppendCommitRequest {
        workspace_id: workspace.to_string(),
        branch_id,
        commit_id: new_commit_id,
        parent_commit_id: source_commit.parent_commit_id().map(ToOwned::to_owned),
        message,
        body,
        created_at_ms: crate::now_ms_i64(),
    };

    match server.store.append_commit(request) {
        Ok(amended) => crate::ai_ok(
            "think.amend",
            json!({
                "source_commit_id": source_commit_id,
                "amended_commit": commit_to_json(&amended),
            }),
        ),
        Err(err) => map_store_error(err),
    }
}

fn handle_delete(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    if let Err(err) =
        command.reject_unknown_args(&["commit", "new_commit", "branch", "message", "body"])
    {
        return err;
    }

    let source_commit_id = match command.require_arg("commit") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let new_commit_id = match command.require_arg("new_commit") {
        Ok(v) => v,
        Err(err) => return err,
    };

    let Some(source_commit) = (match server.store.show_commit(ShowCommitRequest {
        workspace_id: workspace.to_string(),
        commit_id: source_commit_id.clone(),
    }) {
        Ok(v) => v,
        Err(err) => return map_store_error(err),
    }) else {
        return crate::ai_error_with(
            "UNKNOWN_ID",
            &format!("Unknown commit: {source_commit_id}"),
            Some("Call think log to discover existing commits."),
            Vec::new(),
        );
    };

    let branch_id = command
        .optional_arg("branch")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| source_commit.branch_id().to_string());
    let message = command
        .optional_arg("message")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("delete {}", source_commit.commit_id()));
    let body = command
        .optional_arg("body")
        .map(ToOwned::to_owned)
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            if command.body.is_empty() {
                None
            } else {
                Some(command.body.clone())
            }
        })
        .unwrap_or_else(|| format!("tombstone for {}", source_commit.commit_id()));

    let request = AppendCommitRequest {
        workspace_id: workspace.to_string(),
        branch_id,
        commit_id: new_commit_id,
        parent_commit_id: source_commit.parent_commit_id().map(ToOwned::to_owned),
        message,
        body,
        created_at_ms: crate::now_ms_i64(),
    };

    match server.store.append_commit(request) {
        Ok(tombstone) => crate::ai_ok_with_warnings(
            "think.delete",
            json!({
                "mode": "soft_delete",
                "source_commit_id": source_commit_id,
                "tombstone_commit": commit_to_json(&tombstone),
            }),
            vec![crate::warning(
                "SOFT_DELETE",
                "delete creates a tombstone commit instead of removing history",
                "Use branch delete to remove an entire branch history.",
            )],
            Vec::new(),
        ),
        Err(err) => map_store_error(err),
    }
}

fn commit_to_json(commit: &ThoughtCommit) -> Value {
    json!({
        "workspace_id": commit.workspace_id(),
        "branch_id": commit.branch_id(),
        "commit_id": commit.commit_id(),
        "parent_commit_id": commit.parent_commit_id(),
        "message": commit.message(),
        "body": commit.body(),
        "created_at_ms": commit.created_at_ms(),
    })
}

fn map_store_error(err: StoreError) -> Value {
    match err {
        StoreError::InvalidInput(msg) => crate::ai_error_with(
            "INVALID_INPUT",
            msg,
            Some("Fix input fields and retry."),
            Vec::new(),
        ),
        StoreError::UnknownId | StoreError::UnknownBranch => crate::ai_error_with(
            "UNKNOWN_ID",
            "Unknown branch or commit id",
            Some("Check identifiers and retry."),
            Vec::new(),
        ),
        StoreError::BranchAlreadyExists => crate::ai_error_with(
            "ALREADY_EXISTS",
            "Branch already exists",
            Some("Use a different branch name."),
            Vec::new(),
        ),
        StoreError::BranchCycle | StoreError::BranchDepthExceeded => crate::ai_error_with(
            "INVALID_INPUT",
            &crate::format_store_error(err),
            Some("Fix branch ancestry and retry."),
            Vec::new(),
        ),
        other => crate::ai_error_with(
            "STORE_ERROR",
            &crate::format_store_error(other),
            Some("Retry. If it persists, inspect local store state."),
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_storage::{CreateBranchRequest, SqliteStore};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bm_think_tool_{label}_{nanos}"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn test_server(dir: &PathBuf) -> McpServer {
        let store = SqliteStore::open(dir).expect("store should open");
        let runner_autostart_enabled = Arc::new(AtomicBool::new(false));
        let runner_autostart_state = Arc::new(Mutex::new(crate::RunnerAutostartState::default()));
        McpServer::new(
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
                default_workspace: None,
                workspace_explicit: false,
                workspace_allowlist: None,
                workspace_lock: false,
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
    fn think_log_branch_lookup_is_not_capped_to_single_page() {
        let dir = temp_dir("branch_lookup_not_capped");
        let mut server = test_server(&dir);
        let workspace = "ws-branch-scan";

        server
            .store
            .create_branch(CreateBranchRequest {
                workspace_id: workspace.to_string(),
                branch_id: "main".to_string(),
                parent_branch_id: None,
                created_at_ms: crate::now_ms_i64(),
            })
            .expect("main branch should exist");

        let mut target_branch = String::new();
        for idx in 0..10_050usize {
            let branch_id = format!("b{idx:05}");
            server
                .store
                .create_branch(CreateBranchRequest {
                    workspace_id: workspace.to_string(),
                    branch_id: branch_id.clone(),
                    parent_branch_id: Some("main".to_string()),
                    created_at_ms: crate::now_ms_i64(),
                })
                .expect("branch create should succeed");
            target_branch = branch_id;
        }

        let response = handle(
            &mut server,
            json!({
                "workspace": workspace,
                "markdown": format!("```bm\nlog branch={} limit=1\n```", target_branch),
            }),
        );
        assert_eq!(
            response.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "branch beyond first page must still resolve; got: {response}"
        );
    }
}
