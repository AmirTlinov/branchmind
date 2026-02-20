#![forbid(unsafe_code)]

use super::markdown::parse_tool_markdown;
use bm_core::{ThoughtBranch, ThoughtCommit};
use bm_storage::{AppendCommitRequest, ListBranchesRequest, ShowCommitRequest, StoreError};
use serde_json::{Value, json};

use crate::McpServer;

pub(crate) fn handle(server: &mut McpServer, args: Value) -> Value {
    let parsed = match parse_tool_markdown(args, "think", &["commit", "log", "show", "delete", "amend"]) {
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
        });
    let Some(body) = body else {
        return crate::ai_error_with(
            "INVALID_INPUT",
            "body is required",
            Some("Set body=... or provide body text on the second line inside ```bm."),
            Vec::new(),
        );
    };

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

    let branches = match server.store.list_branches(ListBranchesRequest {
        workspace_id: workspace.to_string(),
        limit: 10_000,
        offset: 0,
    }) {
        Ok(v) => v,
        Err(err) => return map_store_error(err),
    };
    let Some(branch) = branches.iter().find(|b| b.branch_id() == branch_id) else {
        return crate::ai_error_with(
            "UNKNOWN_ID",
            "Unknown branch",
            Some("Create the branch first or check branch list."),
            Vec::new(),
        );
    };

    let mut cursor = command
        .optional_arg("from")
        .map(ToOwned::to_owned)
        .or_else(|| branch.head_commit_id().map(ToOwned::to_owned));
    let mut seen = std::collections::BTreeSet::new();
    let mut skipped = 0usize;
    let mut commits = Vec::new();
    let mut truncated = false;

    while let Some(commit_id) = cursor {
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
                )
            }
            Err(err) => return map_store_error(err),
        };

        cursor = commit.parent_commit_id().map(ToOwned::to_owned);

        if skipped < offset {
            skipped += 1;
            continue;
        }
        if commits.len() >= limit {
            truncated = true;
            break;
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
    if truncated
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("truncated".to_string(), Value::Bool(true));
    }
    crate::ai_ok("think.log", result)
}

fn handle_show(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    let commit_id = match command.require_arg("commit") {
        Ok(v) => v,
        Err(err) => return err,
    };
    match server.store.show_commit(ShowCommitRequest {
        workspace_id: workspace.to_string(),
        commit_id: commit_id.clone(),
    }) {
        Ok(Some(commit)) => crate::ai_ok("think.show", json!({ "commit": commit_to_json(&commit) })),
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

#[allow(dead_code)]
fn branch_to_json(branch: &ThoughtBranch) -> Value {
    json!({
        "workspace_id": branch.workspace_id(),
        "branch_id": branch.branch_id(),
        "parent_branch_id": branch.parent_branch_id(),
        "head_commit_id": branch.head_commit_id(),
        "created_at_ms": branch.created_at_ms(),
        "updated_at_ms": branch.updated_at_ms(),
    })
}

fn map_store_error(err: StoreError) -> Value {
    match err {
        StoreError::InvalidInput(msg) => crate::ai_error_with(
            "INVALID_INPUT",
            &msg,
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
