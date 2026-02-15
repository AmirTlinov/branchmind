#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn open_job_artifact(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    artifact_ref: &str,
    max_chars: usize,
) -> Result<Value, Value> {
    let Some((job_id, artifact_key)) = crate::support::parse_job_artifact_ref(artifact_ref) else {
        return Err(ai_error(
            "INVALID_INPUT",
            "job artifact ref must be artifact://jobs/JOB-.../<artifact_key>",
        ));
    };

    let resolved = crate::support::resolve_job_artifact_text(
        &mut server.store,
        workspace,
        &job_id,
        &artifact_key,
        0,
        max_chars,
    )?;

    let artifact_ref = format!(
        "artifact://jobs/{}/{}",
        resolved.job_id, resolved.artifact_key
    );
    let action_tool_call = json!({
        "tool": "jobs",
        "op": "call",
        "cmd": "jobs.artifact.get",
        "reason": "Read a bounded slice of this job artifact (supports paging via offset).",
        "priority": "high",
        "args": {
            "workspace": workspace.as_str(),
            "job": &resolved.job_id,
            "artifact_key": &resolved.artifact_key,
            "offset": 0,
            "max_chars": max_chars
        }
    });
    let action_open = json!({
        "tool": "open",
        "reason": "Open artifact by stable ref (readable, fallback-aware).",
        "priority": "high",
        "args": {
            "workspace": workspace.as_str(),
            "id": &artifact_ref,
            "max_chars": max_chars
        }
    });
    let mut out = json!({
        "workspace": workspace.as_str(),
        "kind": "job_artifact",
        "job_id": resolved.job_id,
        "artifact_key": resolved.artifact_key,
        "id": artifact_ref,
        "content_text": resolved.content_text,
        "truncated": resolved.truncated,
        "artifact": {
            "job_id": resolved.job_id,
            "artifact_key": resolved.artifact_key,
            "content_len": resolved.content_len,
            "created_at_ms": resolved.created_at_ms,
            "source": match resolved.source {
                crate::support::JobArtifactSource::Store => "store",
                crate::support::JobArtifactSource::SummaryFallback => "summary_fallback"
            }
        },
        "artifact_ref": artifact_ref,
    });

    let actions = vec![action_open, action_tool_call];
    if let Some(obj) = out.as_object_mut() {
        obj.insert("actions".to_string(), Value::Array(actions));
    }
    Ok(out)
}
