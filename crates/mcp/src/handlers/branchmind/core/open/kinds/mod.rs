#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

mod card;
mod doc_entry;
mod job;
mod job_artifact;
mod job_event;
mod runner;
mod slice;

pub(super) struct OpenJobEventRefArgs<'a> {
    pub(super) ref_str: &'a str,
    pub(super) job_id: &'a str,
    pub(super) seq: i64,
    pub(super) include_drafts: bool,
    pub(super) limit: usize,
}

pub(super) fn open_slice(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    slice_id: &str,
    include_content: bool,
) -> Result<Value, Value> {
    slice::open_slice(server, workspace, slice_id, include_content)
}

pub(super) fn open_job_event_ref(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    args: OpenJobEventRefArgs<'_>,
    suggestions: &mut Vec<Value>,
) -> Result<Value, Value> {
    job_event::open_job_event_ref(server, workspace, args, suggestions)
}

pub(super) fn open_runner_ref(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    runner_id: String,
    suggestions: &mut Vec<Value>,
) -> Result<Value, Value> {
    runner::open_runner_ref(server, workspace, runner_id, suggestions)
}

pub(super) fn open_job(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    job_id: &str,
    include_prompt: bool,
    limit: usize,
    suggestions: &mut Vec<Value>,
) -> Result<Value, Value> {
    job::open_job(
        server,
        workspace,
        job_id,
        include_prompt,
        limit,
        suggestions,
    )
}

pub(super) fn open_job_artifact(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    artifact_ref: &str,
    max_chars: usize,
) -> Result<Value, Value> {
    job_artifact::open_job_artifact(server, workspace, artifact_ref, max_chars)
}

pub(super) fn open_card(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    card_id: &str,
    include_content: bool,
) -> Result<Value, Value> {
    card::open_card(server, workspace, card_id, include_content)
}

pub(super) fn open_doc_entry_ref(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    ref_str: &str,
    doc: String,
    seq: i64,
) -> Result<Value, Value> {
    doc_entry::open_doc_entry_ref(server, workspace, ref_str, doc, seq)
}
