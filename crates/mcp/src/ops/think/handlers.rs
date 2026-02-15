#![forbid(unsafe_code)]

use crate::ops::{Envelope, OpResponse, name_to_cmd_segments};

pub(crate) fn handle_reasoning_seed(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_template", env.args.clone())
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_template dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(crate) fn handle_reasoning_pipeline(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_pipeline", env.args.clone())
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_pipeline dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(crate) fn handle_idea_branch_merge(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let handler_resp = crate::handlers::dispatch_handler(server, "merge", env.args.clone())
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "merge dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(crate) fn should_skip_handler_name(name: &str) -> bool {
    if name.starts_with("tasks_") {
        return true;
    }
    if name.starts_with("graph_") {
        return true;
    }
    if matches!(
        name,
        // Dedicated v1 portals:
        "status" | "open" | "workspace_use" | "workspace_reset"
            // System:
            | "storage" | "init" | "help" | "skill" | "diagnostics"
            // VCS / docs:
            | "macro_branch_note"
            | "branch_create"
            | "branch_list"
            | "checkout"
            | "branch_rename"
            | "branch_delete"
            | "notes_commit"
            | "commit"
            | "log"
            | "reflog"
            | "reset"
            | "show"
            | "diff"
            | "merge"
            | "tag_create"
            | "tag_list"
            | "tag_delete"
            | "docs_list"
            | "transcripts_search"
            | "transcripts_open"
            | "transcripts_digest"
            | "export"
            // Curated cmds (registered explicitly):
            | "think_lint"
            | "think_template"
            | "think_pipeline"
            | "atlas_suggest"
            | "macro_atlas_apply"
            | "atlas_bindings_list"
            | "macro_counter_hypothesis_stub"
            | "trace_sequential_step"
    ) {
        return true;
    }
    false
}

pub(crate) fn handler_think_cmd(name: &str) -> String {
    if let Some(suffix) = name.strip_prefix("think_") {
        return format!("think.{}", name_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchors_") {
        return format!("think.anchor.{}", name_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchor_") {
        return format!("think.anchor.{}", name_to_cmd_segments(suffix));
    }
    format!("think.{}", name_to_cmd_segments(name))
}
