#![forbid(unsafe_code)]

use super::markdown::parse_tool_markdown;
use bm_core::MergeRecord;
use bm_storage::{CreateMergeRecordRequest, StoreError};
use serde_json::{Value, json};

use crate::McpServer;

pub(crate) fn handle(server: &mut McpServer, args: Value) -> Value {
    let parsed = match parse_tool_markdown(args, "merge", &["into"]) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match parsed.command.verb.as_str() {
        "into" => handle_into(server, &parsed.workspace, &parsed.command),
        _ => crate::ai_error_with(
            "UNKNOWN_VERB",
            "Unsupported merge verb",
            Some("Use merge with verb: into."),
            Vec::new(),
        ),
    }
}

fn handle_into(
    server: &mut McpServer,
    workspace: &str,
    command: &super::markdown::ParsedCommand,
) -> Value {
    let target_branch_id = match command.require_arg("target") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let from = match command.require_arg("from") {
        Ok(v) => v,
        Err(err) => return err,
    };
    let source_branches = from
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if source_branches.is_empty() {
        return crate::ai_error_with(
            "INVALID_INPUT",
            "from must include at least one source branch",
            Some("Use comma-separated source branches, e.g. from=feature,hotfix."),
            Vec::new(),
        );
    }

    let strategy = command.optional_arg("strategy").unwrap_or("squash").to_string();
    let summary = command.optional_arg("summary").map(ToOwned::to_owned).unwrap_or_else(|| {
        format!(
            "merge {} into {}",
            source_branches.join(","),
            target_branch_id
        )
    });
    let synthesis_message = command
        .optional_arg("message")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| summary.clone());
    let synthesis_body = command
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
        .unwrap_or_else(|| summary.clone());

    let now_ms = crate::now_ms_i64();
    let mut merges = Vec::new();
    let mut warnings = Vec::new();

    for (idx, source_branch_id) in source_branches.iter().enumerate() {
        let suffix = format!(
            "{}-{}-{}-{}",
            sanitize_id_part(&target_branch_id),
            sanitize_id_part(source_branch_id),
            now_ms,
            idx
        );
        let merge_id = truncate_id(format!("merge-{suffix}"));
        let synthesis_commit_id = truncate_id(format!("c-merge-{suffix}"));
        let request = CreateMergeRecordRequest {
            workspace_id: workspace.to_string(),
            merge_id,
            source_branch_id: source_branch_id.clone(),
            target_branch_id: target_branch_id.clone(),
            strategy: strategy.clone(),
            summary: summary.clone(),
            synthesis_commit_id,
            synthesis_message: synthesis_message.clone(),
            synthesis_body: synthesis_body.clone(),
            created_at_ms: now_ms,
        };

        match server.store.create_merge_record(request) {
            Ok(merge_record) => merges.push(merge_to_json(&merge_record)),
            Err(err) => warnings.push(merge_warning(source_branch_id, err)),
        }
    }

    if merges.is_empty() {
        return crate::ai_error_with(
            "MERGE_FAILED",
            "No source branches were merged",
            Some("Inspect warnings and retry with valid source/target branches."),
            Vec::new(),
        );
    }

    let result = json!({
        "workspace": workspace,
        "target": target_branch_id,
        "strategy": strategy,
        "summary": summary,
        "merged": merges,
    });
    if warnings.is_empty() {
        crate::ai_ok("merge.into", result)
    } else {
        crate::ai_ok_with_warnings("merge.into", result, warnings, Vec::new())
    }
}

fn merge_to_json(merge: &MergeRecord) -> Value {
    json!({
        "workspace_id": merge.workspace_id(),
        "merge_id": merge.merge_id(),
        "source_branch_id": merge.source_branch_id(),
        "target_branch_id": merge.target_branch_id(),
        "synthesis_commit_id": merge.synthesis_commit_id(),
        "strategy": merge.strategy(),
        "summary": merge.summary(),
        "created_at_ms": merge.created_at_ms(),
    })
}

fn merge_warning(source_branch: &str, err: StoreError) -> Value {
    let (code, message, recovery): (&str, String, &str) = match err {
        StoreError::InvalidInput(msg) => ("INVALID_INPUT", msg.to_string(), "Fix input and retry."),
        StoreError::UnknownId | StoreError::UnknownBranch => (
            "UNKNOWN_ID",
            "unknown source/target branch".to_string(),
            "Check branch ids and retry.",
        ),
        StoreError::BranchAlreadyExists => (
            "ALREADY_EXISTS",
            "merge record already exists".to_string(),
            "Use a different merge id seed (retry).",
        ),
        other => (
            "STORE_ERROR",
            crate::format_store_error(other),
            "Retry. If it persists, inspect local store state.",
        ),
    };
    json!({
        "code": "MERGE_SOURCE_FAILED",
        "source_branch_id": source_branch,
        "error_code": code,
        "message": message,
        "recovery": recovery,
    })
}

fn sanitize_id_part(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "x".to_string() } else { out }
}

fn truncate_id(id: String) -> String {
    const MAX_ID_CHARS: usize = 120;
    if id.chars().count() <= MAX_ID_CHARS {
        return id;
    }
    id.chars().take(MAX_ID_CHARS).collect::<String>()
}
