#![forbid(unsafe_code)]

use super::ai_error;
use super::artifact_contracts::{
    scout_policy_from_meta, validate_builder_diff_batch, validate_scout_context_pack,
    validate_validator_report_v2, validate_writer_patch_pack,
};
use super::warning;
use bm_core::ids::WorkspaceId;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JobArtifactSource {
    Store,
    SummaryFallback,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedJobArtifact {
    pub job_id: String,
    pub artifact_key: String,
    pub content_text: String,
    pub content_len: i64,
    pub created_at_ms: i64,
    pub source: JobArtifactSource,
    pub truncated: bool,
    pub offset: usize,
    pub warnings: Vec<Value>,
}

fn trim_to_char_boundary(text: &str, idx: usize) -> usize {
    if idx >= text.len() {
        return text.len();
    }
    let mut safe = idx;
    while safe > 0 && !text.is_char_boundary(safe) {
        safe -= 1;
    }
    safe
}

fn bounded_preview(text: &str, offset: usize, max_chars: usize) -> (String, usize, bool) {
    if text.is_empty() {
        return (String::new(), 0, false);
    }
    let start = trim_to_char_boundary(text, offset);
    let max_chars = max_chars.max(1);
    let mut end = (start + max_chars).min(text.len());
    end = trim_to_char_boundary(text, end);
    let truncated = end < text.len();
    (text[start..end].to_string(), start, truncated)
}

fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .or_else(|_| serde_json::to_string(value))
        .unwrap_or_else(|_| value.to_string())
}

pub(crate) fn parse_job_artifact_ref(raw: &str) -> Option<(String, String)> {
    let rest = raw.trim().strip_prefix("artifact://jobs/")?;
    let mut parts = rest.split('/');
    let job_id = parts.next()?.trim();
    let artifact_key = parts.next()?.trim();
    if parts.next().is_some() {
        return None;
    }
    if job_id.is_empty() || artifact_key.is_empty() {
        return None;
    }
    Some((job_id.to_string(), artifact_key.to_string()))
}

pub(crate) fn expected_artifacts_from_meta_json(meta_json: Option<&str>) -> Vec<String> {
    meta_json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .as_ref()
        .and_then(|v| v.get("expected_artifacts"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn validate_by_artifact_key(
    store: &bm_storage::SqliteStore,
    workspace: &WorkspaceId,
    artifact_key: &str,
    value: &Value,
    meta_json: Option<&str>,
) -> Result<(Value, Vec<Value>), Value> {
    let meta_map = meta_json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|v| v.as_object().cloned());

    match artifact_key.trim() {
        "scout_context_pack" => {
            let max_context_refs = meta_map
                .as_ref()
                .and_then(|m| m.get("max_context_refs"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(24)
                .clamp(8, 64);
            let policy = meta_map
                .as_ref()
                .map(scout_policy_from_meta)
                .unwrap_or_default();
            validate_scout_context_pack(store, workspace, value, max_context_refs, &policy)
        }
        "builder_diff_batch" => Ok((validate_builder_diff_batch(value)?, Vec::new())),
        "validator_report" => {
            let contract = validate_validator_report_v2(value)?;
            Ok((contract.normalized, Vec::new()))
        }
        "writer_patch_pack" => Ok((validate_writer_patch_pack(value)?, Vec::new())),
        other => Err(ai_error(
            "INVALID_INPUT",
            &format!(
                "unknown artifact_key `{other}` for jobs.complete; supported: scout_context_pack | builder_diff_batch | validator_report | writer_patch_pack"
            ),
        )),
    }
}

pub(crate) fn resolve_job_artifact_text(
    store: &mut bm_storage::SqliteStore,
    workspace: &WorkspaceId,
    job_id: &str,
    artifact_key: &str,
    offset: usize,
    max_chars: usize,
) -> Result<ResolvedJobArtifact, Value> {
    let job_id_trimmed = job_id.trim().to_string();
    let artifact_key_trimmed = artifact_key.trim().to_string();
    if job_id_trimmed.is_empty() {
        return Err(ai_error("INVALID_INPUT", "job_id must not be empty"));
    }
    if artifact_key_trimmed.is_empty() {
        return Err(ai_error("INVALID_INPUT", "artifact_key must not be empty"));
    }

    let stored = match store.job_artifact_get(
        workspace,
        bm_storage::JobArtifactGetRequest {
            job_id: job_id_trimmed.clone(),
            artifact_key: artifact_key_trimmed.clone(),
        },
    ) {
        Ok(v) => v,
        Err(bm_storage::StoreError::UnknownId) => {
            return Err(ai_error("UNKNOWN_ID", "Unknown job id"));
        }
        Err(bm_storage::StoreError::InvalidInput(msg)) => {
            return Err(ai_error("INVALID_INPUT", msg));
        }
        Err(err) => return Err(ai_error("STORE_ERROR", &super::format_store_error(err))),
    };

    if let Some(artifact) = stored {
        let full = artifact.content_text;
        let content_len = full.len() as i64;
        let (preview, preview_offset, truncated) =
            bounded_preview(&full, offset, max_chars.clamp(1, 4000));
        return Ok(ResolvedJobArtifact {
            job_id: artifact.job_id,
            artifact_key: artifact.artifact_key,
            content_text: preview,
            content_len,
            created_at_ms: artifact.created_at_ms,
            source: JobArtifactSource::Store,
            truncated,
            offset: preview_offset,
            warnings: Vec::new(),
        });
    }

    // Fallback: derive artifact from job.summary (legacy jobs / pre-materialization).
    let open = match store.job_open(
        workspace,
        bm_storage::JobOpenRequest {
            id: job_id_trimmed.clone(),
            include_prompt: false,
            include_events: false,
            include_meta: true,
            max_events: 0,
            before_seq: None,
        },
    ) {
        Ok(v) => v,
        Err(bm_storage::StoreError::UnknownId) => {
            return Err(ai_error("UNKNOWN_ID", "Unknown job id"));
        }
        Err(bm_storage::StoreError::InvalidInput(msg)) => {
            return Err(ai_error("INVALID_INPUT", msg));
        }
        Err(err) => return Err(ai_error("STORE_ERROR", &super::format_store_error(err))),
    };

    if !open.job.status.eq_ignore_ascii_case("DONE") {
        return Err(ai_error(
            "PRECONDITION_FAILED",
            "job artifact is not available until job is DONE",
        ));
    }

    let summary = open.job.summary.as_deref().unwrap_or("").trim().to_string();
    if summary.is_empty() {
        return Err(ai_error(
            "UNKNOWN_ID",
            "Unknown job artifact_key (no stored artifact and job summary is empty)",
        ));
    }

    let parsed_summary = crate::support::parse_json_object_from_text(
        &summary,
        "jobs.summary",
    )
    .map_err(|_| {
        ai_error(
            "UNKNOWN_ID",
            "Unknown job artifact_key (no stored artifact and job summary is not a valid artifact JSON object)",
        )
    })?;

    let (normalized, mut contract_warnings) = validate_by_artifact_key(
        store,
        workspace,
        &artifact_key_trimmed,
        &parsed_summary,
        open.meta_json.as_deref(),
    )
    .map_err(|err| {
        let msg = err
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("artifact summary does not match requested contract");
        ai_error(
            "UNKNOWN_ID",
            &format!("unknown job artifact_key (summary fallback failed contract: {msg})"),
        )
    })?;

    let full_content = canonical_json_string(&normalized);
    let mut warnings = vec![warning(
        "ARTIFACT_FALLBACK_FROM_SUMMARY",
        "Artifact was not stored; derived from job.summary as a fallback.",
        "Re-run the job on a newer server (or ensure jobs.complete materializes artifacts) to get a stable job_artifacts entry.",
    )];
    warnings.append(&mut contract_warnings);

    let content_len = full_content.len() as i64;
    let (preview, preview_offset, truncated) =
        bounded_preview(&full_content, offset, max_chars.clamp(1, 4000));

    Ok(ResolvedJobArtifact {
        job_id: job_id_trimmed,
        artifact_key: artifact_key_trimmed,
        content_len,
        content_text: preview,
        created_at_ms: open.job.updated_at_ms,
        source: JobArtifactSource::SummaryFallback,
        truncated,
        offset: preview_offset,
        warnings,
    })
}
