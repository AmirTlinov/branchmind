#![forbid(unsafe_code)]

use super::markdown::parse_tool_markdown;
use crate::{McpServer, WorkspaceId};
use bm_core::ThoughtBranch;
use bm_storage::{
    CreateBranchRequest, DeleteBranchRequest, ListBranchesRequest, StoreError,
};
use serde_json::{Value, json};

pub(crate) fn handle(server: &mut McpServer, args: Value) -> Value {
    let parsed = match parse_tool_markdown(
        args,
        "branch",
        &["create", "list", "checkout", "delete", "main"],
    ) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match parsed.command.verb.as_str() {
        "create" => handle_create(server, &parsed.workspace, &parsed.command),
        "list" => handle_list(server, &parsed.workspace, &parsed.command),
        "checkout" => handle_checkout(server, &parsed.workspace, &parsed.command),
        "delete" => handle_delete(server, &parsed.workspace, &parsed.command),
        "main" => handle_main(server, &parsed.workspace),
        _ => crate::ai_error_with(
            "UNKNOWN_VERB",
            "Unsupported branch verb",
            Some("Use one of: create, list, checkout, delete, main."),
            Vec::new(),
        ),
    }
}

fn handle_create(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    let branch_id = match command.require_arg("branch") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let parent_branch_id = command.optional_arg("from").map(ToOwned::to_owned);

    match server.store.create_branch(CreateBranchRequest {
        workspace_id: workspace.to_string(),
        branch_id,
        parent_branch_id,
        created_at_ms: crate::now_ms_i64(),
    }) {
        Ok(branch) => crate::ai_ok("branch.create", json!({ "branch": branch_to_json(&branch) })),
        Err(err) => map_store_error(err),
    }
}

fn handle_list(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    let limit = match command.optional_usize_arg("limit", 50) {
        Ok(v) => v.min(500),
        Err(err) => return err,
    };
    let offset = match command.optional_usize_arg("offset", 0) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match server.store.list_branches(ListBranchesRequest {
        workspace_id: workspace.to_string(),
        limit,
        offset,
    }) {
        Ok(branches) => crate::ai_ok(
            "branch.list",
            json!({
                "workspace": workspace,
                "items": branches.iter().map(branch_to_json).collect::<Vec<_>>(),
                "limit": limit,
                "offset": offset,
            }),
        ),
        Err(err) => map_store_error(err),
    }
}

fn handle_checkout(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    let branch_id = match command.require_arg("branch") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let workspace_id = match WorkspaceId::try_new(workspace.to_string()) {
        Ok(v) => v,
        Err(_) => {
            return crate::ai_error_with(
                "INVALID_INPUT",
                "workspace must be a valid WorkspaceId",
                Some("Use only letters, digits, '.', '-', '_' or '/'."),
                Vec::new(),
            )
        }
    };

    let exists = match server.store.branch_exists(&workspace_id, &branch_id) {
        Ok(v) => v,
        Err(err) => return map_store_error(err),
    };
    if !exists {
        return crate::ai_error_with(
            "UNKNOWN_ID",
            "Unknown branch",
            Some("Call branch with ```bm\\nlist\\n``` to inspect available branches."),
            Vec::new(),
        );
    }

    match server.store.branch_checkout_set(&workspace_id, &branch_id) {
        Ok((previous_branch, active_branch)) => crate::ai_ok(
            "branch.checkout",
            json!({
                "workspace": workspace,
                "branch": active_branch,
                "previous_branch": previous_branch,
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
    let branch_id = match command.require_arg("branch") {
        Ok(v) => v,
        Err(err) => return err,
    };
    match server.store.delete_branch(DeleteBranchRequest {
        workspace_id: workspace.to_string(),
        branch_id: branch_id.clone(),
    }) {
        Ok(()) => crate::ai_ok(
            "branch.delete",
            json!({
                "workspace": workspace,
                "branch": branch_id,
                "deleted": true
            }),
        ),
        Err(err) => map_store_error(err),
    }
}

fn handle_main(server: &mut McpServer, workspace: &str) -> Value {
    let workspace_id = match WorkspaceId::try_new(workspace.to_string()) {
        Ok(v) => v,
        Err(_) => {
            return crate::ai_error_with(
                "INVALID_INPUT",
                "workspace must be a valid WorkspaceId",
                Some("Use only letters, digits, '.', '-', '_' or '/'."),
                Vec::new(),
            )
        }
    };

    let exists = match server.store.branch_exists(&workspace_id, "main") {
        Ok(v) => v,
        Err(err) => return map_store_error(err),
    };
    if !exists {
        match server.store.create_branch(CreateBranchRequest {
            workspace_id: workspace.to_string(),
            branch_id: "main".to_string(),
            parent_branch_id: None,
            created_at_ms: crate::now_ms_i64(),
        }) {
            Ok(_) => {}
            Err(err) => return map_store_error(err),
        }
    }

    match server.store.branch_checkout_set(&workspace_id, "main") {
        Ok((previous_branch, active_branch)) => crate::ai_ok(
            "branch.main",
            json!({
                "workspace": workspace,
                "branch": active_branch,
                "previous_branch": previous_branch,
                "checked_out": true
            }),
        ),
        Err(err) => map_store_error(err),
    }
}

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
            Some("Use a different branch name or list existing branches."),
            Vec::new(),
        ),
        StoreError::BranchCycle => crate::ai_error_with(
            "INVALID_INPUT",
            "Branch parent cycle is not allowed",
            Some("Choose a parent branch that does not create a cycle."),
            Vec::new(),
        ),
        StoreError::BranchDepthExceeded => crate::ai_error_with(
            "INVALID_INPUT",
            "Branch depth limit exceeded",
            Some("Choose a shallower parent branch."),
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
