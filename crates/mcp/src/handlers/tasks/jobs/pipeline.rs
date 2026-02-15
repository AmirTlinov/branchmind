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
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::support::planfs::{
    PlanFsReadLimits, PlanFsSlice, parse_plan_with_front_matter, parse_slice_with_front_matter,
};

const DEFAULT_JOBS_MODEL: &str = "gpt-5.3-codex";
const DEFAULT_SCOUT_EXECUTOR: &str = "claude_code";
const DEFAULT_SCOUT_MODEL: &str = "haiku";
const DEFAULT_VALIDATOR_EXECUTOR: &str = "claude_code";
const DEFAULT_VALIDATOR_MODEL: &str = "opus-4.6";
const DEFAULT_VALIDATOR_PROFILE: &str = "audit";
const DEFAULT_EXECUTOR_PROFILE: &str = "xhigh";
const DEFAULT_STRICT_SCOUT_STALE_AFTER_S: i64 = 900;
const MAX_CONTEXT_RETRY_LIMIT: u64 = 2;
const PLANFS_EXCERPT_MAX_CHARS: usize = 1200;

#[derive(Clone, Debug)]
pub(super) struct SliceBinding {
    pub plan_id: String,
    pub slice_task_id: String,
    pub spec: crate::support::SlicePlanSpec,
}

#[derive(Clone, Debug)]
pub(super) struct PlanFsTargetContext {
    pub target_ref: String,
    pub plan_slug: String,
    pub plan_path: String,
    pub slice_file: String,
    pub slice_id: String,
    pub spec: crate::support::SlicePlanSpec,
    pub excerpt: String,
}

#[derive(Clone, Debug)]
enum PlanFsSelector {
    SliceId(String),
    SliceFile(String),
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

pub(super) fn resolve_planfs_target_optional(
    server: &mut crate::McpServer,
    workspace: &WorkspaceId,
    args_obj: &serde_json::Map<String, Value>,
) -> Result<Option<PlanFsTargetContext>, Value> {
    let Some(target_ref) = optional_non_empty_string(args_obj, "target_ref")? else {
        return Ok(None);
    };
    let (plan_slug, selector) = parse_planfs_target_ref(&target_ref)?;
    let repo_root = workspace_repo_root_for_jobs(&server.store, workspace)?;
    let limits = PlanFsReadLimits::default();
    let plan_dir = repo_root.join("docs").join("plans").join(&plan_slug);
    if !plan_dir.exists() || !plan_dir.is_dir() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("target_ref points to unknown planfs directory: docs/plans/{plan_slug}"),
            Some("Run tasks.planfs.init first, then retry with a valid target_ref."),
            Vec::new(),
        ));
    }

    let plan_file = plan_dir.join("PLAN.md");
    let plan_raw = read_limited_text_file(&plan_file, limits.max_file_bytes)?;
    let (_plan, refs) = parse_plan_with_front_matter(&plan_raw, true, &limits)?;

    let picked = refs
        .into_iter()
        .find(|slice_ref| match &selector {
            PlanFsSelector::SliceId(id) => slice_ref.id.eq_ignore_ascii_case(id),
            PlanFsSelector::SliceFile(file) => slice_ref.file.eq_ignore_ascii_case(file),
        })
        .ok_or_else(|| {
            ai_error_with(
                "INVALID_INPUT",
                "target_ref slice selector not found in PLAN.md",
                Some("Use target_ref=planfs:<slug>#SLICE-<n> or target_ref=planfs:<slug>/Slice-<n>.md."),
                Vec::new(),
            )
        })?;

    let slice_path = plan_dir.join(&picked.file);
    let slice_raw = read_limited_text_file(&slice_path, limits.max_file_bytes)?;
    let slice = parse_slice_with_front_matter(&slice_raw, true, &limits)?;
    let spec = planfs_slice_to_slice_spec(&slice);
    let excerpt = render_planfs_slice_excerpt(&slice, PLANFS_EXCERPT_MAX_CHARS);

    Ok(Some(PlanFsTargetContext {
        target_ref,
        plan_slug: plan_slug.clone(),
        plan_path: format!("docs/plans/{plan_slug}"),
        slice_file: picked.file,
        slice_id: slice.id,
        spec,
        excerpt,
    }))
}

fn parse_planfs_target_ref(raw: &str) -> Result<(String, PlanFsSelector), Value> {
    let trimmed = raw.trim();
    let rest = trimmed
        .strip_prefix("planfs://")
        .or_else(|| trimmed.strip_prefix("planfs:"))
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "target_ref must start with planfs: (e.g. planfs:<slug>#SLICE-1)",
            )
        })?;
    let rest = rest.trim_start_matches('/');
    let (slug_raw, selector_raw) = rest
        .split_once('#')
        .or_else(|| rest.split_once('/'))
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "target_ref must include slug and slice selector (planfs:<slug>#SLICE-1)",
            )
        })?;

    let slug = normalize_planfs_slug(slug_raw).ok_or_else(|| {
        ai_error(
            "INVALID_INPUT",
            "target_ref slug must use lowercase letters/digits and '-' separators",
        )
    })?;
    let selector = normalize_planfs_selector(selector_raw)?;
    Ok((slug, selector))
}

fn normalize_planfs_slug(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.ends_with('-') {
        return None;
    }
    if trimmed
        .chars()
        .any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'))
    {
        return None;
    }
    Some(trimmed)
}

fn normalize_planfs_selector(raw: &str) -> Result<PlanFsSelector, Value> {
    let selector = raw.trim();
    if selector.is_empty()
        || selector.contains("..")
        || selector.contains('\\')
        || selector.contains('/')
    {
        return Err(ai_error(
            "INVALID_INPUT",
            "target_ref selector is invalid; use SLICE-<n> or Slice-<n>.md",
        ));
    }
    if selector.to_ascii_lowercase().ends_with(".md") {
        return Ok(PlanFsSelector::SliceFile(selector.to_string()));
    }
    if selector.to_ascii_lowercase().starts_with("slice-") {
        return Ok(PlanFsSelector::SliceId(selector.to_ascii_uppercase()));
    }
    Err(ai_error(
        "INVALID_INPUT",
        "target_ref selector must be SLICE-<n> or Slice-<n>.md",
    ))
}

fn workspace_repo_root_for_jobs(
    store: &bm_storage::SqliteStore,
    workspace: &WorkspaceId,
) -> Result<PathBuf, Value> {
    let root = store
        .workspace_path_primary_get(workspace)
        .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
    let Some(root) = root else {
        return Err(ai_error_with(
            "PRECONDITION_FAILED",
            "workspace has no bound repo path",
            Some("Bind workspace by calling status with workspace=\"/absolute/path/to/repo\"."),
            Vec::new(),
        ));
    };
    let abs = PathBuf::from(root);
    if !abs.exists() || !abs.is_dir() {
        return Err(ai_error_with(
            "PRECONDITION_FAILED",
            "workspace bound path does not exist or is not a directory",
            Some("Re-bind workspace to an existing repository path."),
            Vec::new(),
        ));
    }
    Ok(abs)
}

fn read_limited_text_file(path: &Path, max_file_bytes: usize) -> Result<String, Value> {
    let meta = fs::metadata(path).map_err(|err| {
        ai_error(
            "STORE_ERROR",
            &format!("cannot stat planfs file {}: {err}", path.to_string_lossy()),
        )
    })?;
    if meta.len() > max_file_bytes as u64 {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "planfs file exceeds max_file_bytes: {} > {} ({})",
                meta.len(),
                max_file_bytes,
                path.to_string_lossy()
            ),
            Some("Reduce file size or increase max_file_bytes budget."),
            Vec::new(),
        ));
    }
    fs::read_to_string(path).map_err(|err| {
        ai_error(
            "STORE_ERROR",
            &format!("cannot read planfs file {}: {err}", path.to_string_lossy()),
        )
    })
}

fn planfs_slice_to_slice_spec(slice: &PlanFsSlice) -> crate::support::SlicePlanSpec {
    let budgets = crate::support::SliceBudgets {
        max_context_refs: slice.budgets.max_context_refs.clamp(8, 64),
        max_files: slice.budgets.max_files.clamp(1, 64),
        max_diff_lines: slice.budgets.max_diff_lines.clamp(50, 20_000),
    };

    let mut shared_context_refs = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for item in slice
        .sections
        .interfaces
        .iter()
        .chain(slice.sections.contracts.iter())
        .chain(slice.sections.scope.iter())
    {
        let normalized = item.trim();
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_ascii_lowercase();
        if seen.insert(key) {
            shared_context_refs.push(normalized.to_string());
        }
        if shared_context_refs.len() >= budgets.max_context_refs {
            break;
        }
    }

    let tasks = slice
        .tasks
        .iter()
        .map(|task| crate::support::SliceTaskSpec {
            title: task.title.clone(),
            success_criteria: task.success_criteria.clone(),
            tests: task.tests.clone(),
            blockers: task.blockers.clone(),
            steps: task
                .steps
                .iter()
                .map(|step| crate::support::SliceStepSpec {
                    title: step.title.clone(),
                    success_criteria: step.success_criteria.clone(),
                    tests: step.tests.clone(),
                    blockers: step.blockers.clone(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    crate::support::SlicePlanSpec {
        title: slice.title.clone(),
        objective: slice.objective.clone(),
        non_goals: slice.sections.non_goals.clone(),
        shared_context_refs,
        dod: crate::support::SliceDod {
            criteria: slice.dod.success_criteria.clone(),
            tests: slice.dod.tests.clone(),
            blockers: slice.dod.blockers.clone(),
        },
        tasks,
        budgets,
    }
}

fn render_planfs_slice_excerpt(slice: &PlanFsSlice, max_chars: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!("Slice {} — {}\n", slice.id, slice.title));
    out.push_str(&format!("Objective: {}\n", slice.objective.trim()));
    if !slice.sections.goal.trim().is_empty() {
        out.push_str(&format!("Goal: {}\n", slice.sections.goal.trim()));
    }
    append_bullets(&mut out, "Scope", &slice.sections.scope, 4);
    append_bullets(&mut out, "Non-goals", &slice.sections.non_goals, 4);
    append_bullets(&mut out, "Success criteria", &slice.dod.success_criteria, 4);
    append_bullets(&mut out, "Tests", &slice.dod.tests, 4);
    append_bullets(&mut out, "Blockers", &slice.dod.blockers, 4);
    append_bullets(&mut out, "Risks", &slice.sections.risks, 3);
    truncate_chars(out.trim().to_string(), max_chars)
}

fn append_bullets(out: &mut String, title: &str, items: &[String], max_items: usize) {
    if items.is_empty() {
        return;
    }
    out.push_str(title);
    out.push_str(":\n");
    for item in items.iter().take(max_items) {
        out.push_str("- ");
        out.push_str(item.trim());
        out.push('\n');
    }
    if items.len() > max_items {
        out.push_str(&format!("- … (+{} more)\n", items.len() - max_items));
    }
}

fn truncate_chars(mut text: String, max_chars: usize) -> String {
    let len = text.chars().count();
    if len <= max_chars {
        return text;
    }
    let keep = max_chars.saturating_sub(1);
    text = text.chars().take(keep).collect();
    text.push('…');
    text
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
