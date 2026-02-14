#![forbid(unsafe_code)]

use super::*;
use crate::support::{
    PreValidatorVerdict, ScoutAnchorType, ScoutAnchorV2, ScoutNoveltyPolicy, ScoutQualityProfile,
    ScoutValidationPolicy, ensure_artifact_ref, now_ms_i64, pre_validate_scout_pack,
    validate_builder_diff_batch as validate_builder_diff_batch_contract,
    validate_scout_context_pack as validate_scout_context_pack_contract,
    validate_validator_report as validate_validator_report_contract,
};
use serde_json::{Value, json};

const DEFAULT_JOBS_MODEL: &str = "gpt-5.3-codex";
const DEFAULT_SCOUT_EXECUTOR: &str = "claude_code";
const DEFAULT_SCOUT_MODEL: &str = "haiku";
const DEFAULT_VALIDATOR_EXECUTOR: &str = "claude_code";
const DEFAULT_VALIDATOR_MODEL: &str = "opus-4.6";
const DEFAULT_VALIDATOR_PROFILE: &str = "audit";
const DEFAULT_EXECUTOR_PROFILE: &str = "xhigh";
const DEFAULT_STRICT_SCOUT_STALE_AFTER_S: i64 = 900;
const MAX_CONTEXT_RETRY_LIMIT: u64 = 2;

#[derive(Clone, Debug)]
pub(super) struct SliceBinding {
    pub plan_id: String,
    pub slice_task_id: String,
    pub spec: crate::support::SlicePlanSpec,
}

pub(super) fn require_non_empty_string(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, Value> {
    let raw = require_string(args_obj, key)?;
    let v = raw.trim();
    if v.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    Ok(v.to_string())
}

pub(super) fn optional_non_empty_string(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, Value> {
    let Some(v) = optional_string(args_obj, key)? else {
        return Ok(None);
    };
    let v = v.trim();
    if v.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must not be empty"),
        ));
    }
    Ok(Some(v.to_string()))
}

pub(super) fn resolve_slice_binding_optional(
    server: &mut crate::McpServer,
    workspace: &WorkspaceId,
    slice_id: &str,
) -> Result<Option<SliceBinding>, Value> {
    let binding = match server.store.plan_slice_get_by_slice_id(workspace, slice_id) {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(None),
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let slice_task = match server.store.get_task(workspace, &binding.slice_task_id) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Err(ai_error(
                "PRECONDITION_FAILED",
                "slice binding references unknown slice_task_id",
            ));
        }
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let spec =
        crate::support::parse_slice_plan_spec_from_task_context(slice_task.context.as_deref())?
            .ok_or_else(|| {
                ai_error(
                    "PRECONDITION_FAILED",
                    "slice_task_id missing slice_plan_spec JSON in context",
                )
            })?;
    Ok(Some(SliceBinding {
        plan_id: binding.plan_id,
        slice_task_id: binding.slice_task_id,
        spec,
    }))
}

fn optional_ratio(
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
    default: f64,
) -> Result<f64, Value> {
    let Some(raw) = args_obj.get(key) else {
        return Ok(default);
    };
    let Some(value) = raw.as_f64() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be a number in range 0..1"),
        ));
    };
    if !(0.0..=1.0).contains(&value) {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{key} must be in range 0..1"),
        ));
    }
    Ok(value)
}

pub(super) fn scout_policy_from_meta(
    meta: &serde_json::Map<String, Value>,
) -> ScoutValidationPolicy {
    crate::support::scout_policy_from_meta(meta)
}

pub(super) fn normalize_string_array(
    value: Option<&Value>,
    field: &str,
) -> Result<Vec<String>, Value> {
    let Some(v) = value else {
        return Ok(Vec::new());
    };
    let Some(arr) = v.as_array() else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected array of strings"),
        ));
    };
    let mut out = Vec::<String>::new();
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{field}: expected array of strings"),
            ));
        };
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        if !out.iter().any(|v| v == s) {
            out.push(s.to_string());
        }
    }
    Ok(out)
}

pub(super) struct MeshMessageRequest {
    pub task_id: Option<String>,
    pub from_agent_id: Option<String>,
    pub thread_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub kind: String,
    pub summary: String,
    pub payload: Value,
}

pub(super) fn publish_optional_mesh_message(
    server: &mut McpServer,
    workspace: &bm_core::ids::WorkspaceId,
    req: MeshMessageRequest,
) -> Result<Option<Value>, Value> {
    if !server.jobs_mesh_v1_enabled {
        return Ok(None);
    }
    let Some(idempotency_key) = req.idempotency_key else {
        return Ok(None);
    };
    let thread_id = req.thread_id.unwrap_or_else(|| {
        req.task_id
            .as_deref()
            .map(|v| format!("task/{}", v.trim()))
            .unwrap_or_else(|| "workspace/main".to_string())
    });
    let from_agent_id = req
        .from_agent_id
        .or_else(|| server.default_agent_id.clone())
        .unwrap_or_else(|| "manager".to_string());
    let payload_json = serde_json::to_string(&req.payload).ok();
    let published = match server.store.job_bus_publish(
        workspace,
        bm_storage::JobBusPublishRequest {
            idempotency_key,
            thread_id,
            from_agent_id,
            from_job_id: None,
            to_agent_id: None,
            kind: req.kind,
            summary: req.summary,
            refs: Vec::new(),
            payload_json,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let m = published.message;
    Ok(Some(json!({
        "deduped": published.deduped,
        "message": {
            "seq": m.seq,
            "ts_ms": m.ts_ms,
            "thread_id": m.thread_id,
            "kind": m.kind,
            "summary": m.summary,
            "idempotency_key": m.idempotency_key
        }
    })))
}

pub(super) fn parse_meta_map(raw: Option<&str>) -> serde_json::Map<String, Value> {
    raw.and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn build_decision_ref(
    task: &str,
    slice_id: &str,
    decision: &str,
    builder_job_id: &str,
    validator_job_id: &str,
    builder_revision: i64,
) -> String {
    format!(
        "artifact://pipeline/gate/{}/{}/{}/builder/{}/validator/{}/rev/{}",
        task.trim(),
        slice_id.trim(),
        decision.trim().to_ascii_lowercase(),
        builder_job_id.trim(),
        validator_job_id.trim(),
        builder_revision
    )
}

fn parse_scout_anchors_v2(pack: &Value) -> Vec<ScoutAnchorV2> {
    pack.get("anchors")
        .and_then(|v| v.as_array())
        .map(|anchors| {
            anchors
                .iter()
                .filter_map(|item| {
                    let obj = item.as_object()?;
                    let id = obj.get("id")?.as_str()?.trim();
                    if id.is_empty() {
                        return None;
                    }
                    let anchor_type = obj
                        .get("anchor_type")
                        .and_then(|v| v.as_str())
                        .and_then(|raw| ScoutAnchorType::from_str(raw).ok())?;
                    let code_ref = obj
                        .get("code_ref")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|v| !v.is_empty())?;
                    let rationale = obj
                        .get("rationale")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .unwrap_or("scout anchor");
                    let content = obj
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .unwrap_or(rationale);
                    let line_count = obj
                        .get("line_count")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or_else(|| content.lines().count().max(1) as u32);
                    let meta_hint = obj
                        .get("meta_hint")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string);
                    Some(ScoutAnchorV2 {
                        id: id.to_string(),
                        anchor_type,
                        rationale: rationale.to_string(),
                        code_ref: code_ref.to_string(),
                        content: content.to_string(),
                        line_count,
                        meta_hint,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

mod dispatch_builder;
mod dispatch_gate;
mod dispatch_scout;
mod dispatch_validator;
