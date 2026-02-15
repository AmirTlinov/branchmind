#![forbid(unsafe_code)]

use super::ai::ai_error;
use super::code_ref::{parse_code_ref_required, validate_code_ref};
use super::repo_paths::{normalize_repo_rel, repo_rel_from_path_input};
use bm_core::ids::WorkspaceId;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod pipeline_v2;
pub(crate) use pipeline_v2::*;

const SCOUT_FORBIDDEN_KEYS: &[&str] = &["diff", "patch", "code", "apply", "unified_diff"];
const SCOUT_MIN_ANCHORS: usize = 3;
const SCOUT_MIN_CHANGE_HINTS: usize = 2;
const SCOUT_MIN_TEST_HINTS: usize = 2;
const SCOUT_MIN_RISK_MAP: usize = 2;
const SCOUT_MIN_SUMMARY_CHARS: usize = 240;
const SCOUT_FLAGSHIP_MIN_SUMMARY_CHARS: usize = 320;

#[derive(Clone, Debug)]
pub(crate) struct ValidatorReportContract {
    pub recommendation: String,
    pub normalized: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScoutQualityProfile {
    Standard,
    Flagship,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScoutNoveltyPolicy {
    Strict,
    Warn,
}

#[derive(Clone, Debug)]
pub(crate) struct ScoutValidationPolicy {
    pub quality_profile: ScoutQualityProfile,
    pub novelty_policy: ScoutNoveltyPolicy,
    pub critic_pass: bool,
    pub max_anchor_overlap: f64,
    pub max_ref_redundancy: f64,
    pub require_objective_coverage: bool,
    pub require_dod_coverage: bool,
    pub required_test_hints: usize,
    pub required_risk_falsifier_pairs: usize,
}

impl Default for ScoutValidationPolicy {
    fn default() -> Self {
        Self {
            quality_profile: ScoutQualityProfile::Flagship,
            novelty_policy: ScoutNoveltyPolicy::Strict,
            critic_pass: true,
            max_anchor_overlap: 0.35,
            max_ref_redundancy: 0.25,
            require_objective_coverage: true,
            require_dod_coverage: true,
            required_test_hints: 3,
            required_risk_falsifier_pairs: 3,
        }
    }
}

pub(crate) fn scout_policy_from_meta(
    meta: &serde_json::Map<String, Value>,
) -> ScoutValidationPolicy {
    let mut policy = ScoutValidationPolicy::default();
    let quality_profile = meta
        .get("quality_profile")
        .and_then(|v| v.as_str())
        .unwrap_or("flagship")
        .to_ascii_lowercase();
    policy.quality_profile = if quality_profile == "standard" {
        ScoutQualityProfile::Standard
    } else {
        ScoutQualityProfile::Flagship
    };

    let novelty_policy = meta
        .get("novelty_policy")
        .and_then(|v| v.as_str())
        .unwrap_or("strict")
        .to_ascii_lowercase();
    policy.novelty_policy = if novelty_policy == "warn" {
        ScoutNoveltyPolicy::Warn
    } else {
        ScoutNoveltyPolicy::Strict
    };
    policy.critic_pass = meta
        .get("critic_pass")
        .and_then(|v| v.as_bool())
        .unwrap_or(matches!(
            policy.quality_profile,
            ScoutQualityProfile::Flagship
        ));
    policy.max_anchor_overlap = meta
        .get("max_anchor_overlap")
        .and_then(|v| v.as_f64())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(policy.max_anchor_overlap);
    policy.max_ref_redundancy = meta
        .get("max_ref_redundancy")
        .and_then(|v| v.as_f64())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(policy.max_ref_redundancy);
    if let Some(targets) = meta.get("coverage_targets").and_then(|v| v.as_object()) {
        policy.require_objective_coverage = targets
            .get("require_objective_coverage")
            .and_then(|v| v.as_bool())
            .unwrap_or(policy.require_objective_coverage);
        policy.require_dod_coverage = targets
            .get("require_dod_coverage")
            .and_then(|v| v.as_bool())
            .unwrap_or(policy.require_dod_coverage);
        policy.required_test_hints = targets
            .get("require_test_hints")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(policy.required_test_hints)
            .clamp(1, 12);
        policy.required_risk_falsifier_pairs = targets
            .get("require_risk_falsifier_pairs")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(policy.required_risk_falsifier_pairs)
            .clamp(1, 12);
    }

    if matches!(policy.quality_profile, ScoutQualityProfile::Standard) {
        policy.required_test_hints = policy.required_test_hints.min(2);
        policy.required_risk_falsifier_pairs = policy.required_risk_falsifier_pairs.min(2);
    }
    policy
}

fn push_warning(warnings: &mut Vec<Value>, code: &str, message: impl Into<String>) {
    warnings.push(json!({ "code": code, "message": message.into() }));
}

fn normalize_signature(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_claim_path_key(raw: &str, repo_root: Option<&Path>) -> Result<String, Value> {
    let repo_rel = match repo_rel_from_path_input(raw, repo_root) {
        Ok(v) => v,
        Err(_) => normalize_repo_rel(raw)?,
    };
    Ok(repo_rel)
}

fn change_hint_is_directory_like(
    raw_path: &str,
    normalized_path: &str,
    repo_root: Option<&Path>,
) -> bool {
    if raw_path.trim_end().ends_with('/') || normalized_path.ends_with('/') {
        return true;
    }
    let Some(root) = repo_root else {
        return false;
    };
    root.join(normalized_path).is_dir()
}

fn change_hint_path_bound(
    raw_path: &str,
    normalized_path: &str,
    bound_path_keys: &HashSet<String>,
    repo_root: Option<&Path>,
) -> bool {
    let normalized_key = normalized_path.to_ascii_lowercase();
    if bound_path_keys.contains(&normalized_key) {
        return true;
    }
    if !change_hint_is_directory_like(raw_path, normalized_path, repo_root) {
        return false;
    }
    let dir = normalized_key.trim_end_matches('/');
    if dir.is_empty() {
        return false;
    }
    let prefix = format!("{dir}/");
    bound_path_keys
        .iter()
        .any(|bound| bound.starts_with(&prefix))
}

fn unique_violation_message(field: &str, what: &str) -> String {
    format!("{field} contains duplicated {what}; duplicates are not allowed")
}

fn enforce_uniqueness_by_policy(
    warnings: &mut Vec<Value>,
    policy: &ScoutValidationPolicy,
    field: &str,
    what: &str,
    duplicate_count: usize,
) -> Result<(), Value> {
    if duplicate_count == 0 {
        return Ok(());
    }
    let message = unique_violation_message(field, what);
    if matches!(policy.novelty_policy, ScoutNoveltyPolicy::Strict) {
        return Err(ai_error("INVALID_INPUT", &message));
    }
    push_warning(warnings, "SCOUT_NOVELTY_WARN", message);
    Ok(())
}

fn ratio_0_1(obj: &serde_json::Map<String, Value>, key: &str, field: &str) -> Result<f64, Value> {
    let value = obj.get(key).and_then(|v| v.as_f64()).ok_or_else(|| {
        ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} is required and must be a number in range 0..1"),
        )
    })?;
    if !(0.0..=1.0).contains(&value) {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} must be in range 0..1"),
        ));
    }
    Ok(value)
}

fn require_object<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a serde_json::Map<String, Value>, Value> {
    value
        .as_object()
        .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{field}: expected object")))
}

fn require_string(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    field: &str,
) -> Result<String, Value> {
    let Some(raw) = obj.get(key).and_then(|v| v.as_str()) else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} is required"),
        ));
    };
    let value = raw.trim();
    if value.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key} must not be empty"),
        ));
    }
    Ok(value.to_string())
}

fn string_array(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    field: &str,
) -> Result<Vec<String>, Value> {
    let Some(arr) = obj.get(key).and_then(|v| v.as_array()) else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key}: expected array"),
        ));
    };
    let mut out = Vec::<String>::new();
    for item in arr {
        let Some(raw) = item.as_str() else {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("{field}.{key}[]: expected string"),
            ));
        };
        let value = raw.trim();
        if !value.is_empty() {
            out.push(value.to_string());
        }
    }
    Ok(out)
}

fn scout_test_hints_array(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    field: &str,
) -> Result<Vec<String>, Value> {
    let Some(arr) = obj.get(key).and_then(|v| v.as_array()) else {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.{key}: expected array"),
        ));
    };
    let mut out = Vec::<String>::new();
    for item in arr {
        match item {
            Value::String(raw) => {
                let value = raw.trim();
                if !value.is_empty() {
                    out.push(value.to_string());
                }
            }
            Value::Object(map) => {
                let name = map
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        ai_error(
                            "INVALID_INPUT",
                            &format!("{field}.{key}[].name is required"),
                        )
                    })?;
                let intent = map
                    .get("intent")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap_or_default();
                if intent.is_empty() {
                    out.push(name.to_string());
                } else {
                    out.push(format!("{name}: {intent}"));
                }
            }
            _ => {
                return Err(ai_error(
                    "INVALID_INPUT",
                    &format!("{field}.{key}[]: expected string or object{{name,intent}}"),
                ));
            }
        }
    }
    Ok(out)
}

fn contains_forbidden_key(value: &Value) -> Option<String> {
    match value {
        Value::Object(obj) => {
            for key in obj.keys() {
                let lower = key.to_ascii_lowercase();
                if SCOUT_FORBIDDEN_KEYS.iter().any(|f| *f == lower) {
                    return Some(key.clone());
                }
            }
            for nested in obj.values() {
                if let Some(hit) = contains_forbidden_key(nested) {
                    return Some(hit);
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(contains_forbidden_key),
        _ => None,
    }
}

fn markdown_block_too_long(text: &str, max_lines: usize) -> bool {
    let mut in_block = false;
    let mut lines = 0usize;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_block && lines > max_lines {
                return true;
            }
            in_block = !in_block;
            if in_block {
                lines = 0;
            }
            continue;
        }
        if in_block {
            lines += 1;
        }
    }
    in_block && lines > max_lines
}

fn any_string_violate_markdown_limit(value: &Value, max_lines: usize) -> bool {
    match value {
        Value::String(s) => markdown_block_too_long(s, max_lines),
        Value::Object(obj) => obj
            .values()
            .any(|nested| any_string_violate_markdown_limit(nested, max_lines)),
        Value::Array(arr) => arr
            .iter()
            .any(|nested| any_string_violate_markdown_limit(nested, max_lines)),
        _ => false,
    }
}

pub(crate) fn ensure_artifact_ref(raw: &str, field: &str) -> Result<String, Value> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field} must not be empty"),
        ));
    }
    if !value.starts_with("artifact://") {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected artifact://... ref"),
        ));
    }
    Ok(value.to_string())
}

pub(crate) fn extract_job_id_from_ref(raw: &str) -> Option<String> {
    for token in raw.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
        if token.len() < 5 {
            continue;
        }
        let upper = token.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("JOB-")
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            return Some(format!("JOB-{rest}"));
        }
    }
    None
}

pub(crate) fn parse_json_object_from_text(raw: &str, field: &str) -> Result<Value, Value> {
    let parsed: Value = serde_json::from_str(raw).map_err(|_| {
        ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected JSON object in summary text"),
        )
    })?;
    if !parsed.is_object() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}: expected JSON object in summary text"),
        ));
    }
    Ok(parsed)
}

pub(crate) fn validate_scout_context_pack(
    store: &bm_storage::SqliteStore,
    workspace: &WorkspaceId,
    raw: &Value,
    max_context_refs: usize,
    policy: &ScoutValidationPolicy,
) -> Result<(Value, Vec<Value>), Value> {
    let obj = require_object(raw, "scout_context_pack")?;
    if let Some(key) = contains_forbidden_key(raw) {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("scout_context_pack: forbidden key `{key}`"),
        ));
    }
    if any_string_violate_markdown_limit(raw, 20) {
        return Err(ai_error(
            "INVALID_INPUT",
            "scout_context_pack: markdown code block over 20 lines is forbidden",
        ));
    }

    let objective = require_string(obj, "objective", "scout_context_pack")?;
    let scope_obj = require_object(
        obj.get("scope")
            .ok_or_else(|| ai_error("INVALID_INPUT", "scout_context_pack.scope is required"))?,
        "scout_context_pack.scope",
    )?;
    let scope_in = string_array(scope_obj, "in", "scout_context_pack.scope")?;
    let scope_out = string_array(scope_obj, "out", "scout_context_pack.scope")?;

    let refs = string_array(obj, "code_refs", "scout_context_pack")?;
    if refs.len() < 3 {
        return Err(ai_error(
            "INVALID_INPUT",
            "scout_context_pack.code_refs must contain at least 3 CODE_REF entries",
        ));
    }
    if refs.len() > max_context_refs {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("scout_context_pack.code_refs exceeds max_context_refs ({max_context_refs})"),
        ));
    }
    let mut code_refs = Vec::<String>::new();
    let mut warnings = Vec::<Value>::new();
    let repo_root = store
        .workspace_path_primary_get(workspace)
        .map_err(|err| ai_error("STORE_ERROR", &format!("{err:?}")))?
        .map(PathBuf::from);
    let mut bound_path_keys = HashSet::<String>::new();
    for raw_ref in refs {
        let parsed = parse_code_ref_required(&raw_ref, "scout_context_pack.code_refs[]")?;
        let validated = validate_code_ref(store, workspace, &parsed)?;
        let normalized =
            parse_code_ref_required(&validated.normalized, "scout_context_pack.code_refs[]")
                .map_err(|_| {
                    ai_error(
                        "INVALID_INPUT",
                        "internal: normalized CODE_REF parsing failed",
                    )
                })?;
        bound_path_keys.insert(normalized.repo_rel.to_ascii_lowercase());
        code_refs.push(validated.normalized);
        warnings.extend(validated.warnings);
    }

    let anchors = obj
        .get("anchors")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "scout_context_pack.anchors: expected array",
            )
        })?;
    let mut anchors_out = Vec::<Value>::new();
    let mut anchor_signatures = Vec::<String>::new();
    let anchors_total = anchors.len().max(code_refs.len()).max(1);
    for (idx, item) in anchors.iter().enumerate() {
        let anchor = require_object(item, "scout_context_pack.anchors[]")?;
        let id = require_string(anchor, "id", "scout_context_pack.anchors[]")?;
        let rationale = require_string(anchor, "rationale", "scout_context_pack.anchors[]")?;
        anchor_signatures.push(normalize_signature(&format!("{id} {rationale}")));

        let anchor_type = match anchor.get("anchor_type") {
            Some(value) => {
                let raw = value.as_str().ok_or_else(|| {
                    ai_error(
                        "INVALID_INPUT",
                        "scout_context_pack.anchors[].anchor_type must be string",
                    )
                })?;
                ScoutAnchorType::from_str(raw)?.as_str().to_string()
            }
            None => {
                if idx == 0 {
                    "primary".to_string()
                } else if idx + 1 == anchors_total {
                    "reference".to_string()
                } else if idx == 1 {
                    "dependency".to_string()
                } else {
                    "structural".to_string()
                }
            }
        };

        let code_ref = match anchor.get("code_ref") {
            Some(value) => {
                let raw = value.as_str().ok_or_else(|| {
                    ai_error(
                        "INVALID_INPUT",
                        "scout_context_pack.anchors[].code_ref must be string",
                    )
                })?;
                let raw = raw.trim();
                if raw.is_empty() {
                    code_refs
                        .get(idx)
                        .cloned()
                        .or_else(|| code_refs.first().cloned())
                        .unwrap_or_default()
                } else {
                    let parsed =
                        parse_code_ref_required(raw, "scout_context_pack.anchors[].code_ref")?;
                    let validated = validate_code_ref(store, workspace, &parsed)?;
                    warnings.extend(validated.warnings);
                    validated.normalized
                }
            }
            None => code_refs
                .get(idx)
                .cloned()
                .or_else(|| code_refs.first().cloned())
                .unwrap_or_default(),
        };
        if !code_ref.trim().is_empty() {
            let parsed =
                parse_code_ref_required(&code_ref, "scout_context_pack.anchors[].code_ref")
                    .map_err(|_| {
                        ai_error(
                            "INVALID_INPUT",
                            "internal: normalized anchor CODE_REF parsing failed",
                        )
                    })?;
            bound_path_keys.insert(parsed.repo_rel.to_ascii_lowercase());
        }

        let content = anchor
            .get("content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| rationale.clone());
        let line_count = match anchor.get("line_count") {
            Some(value) => {
                let n = value.as_u64().ok_or_else(|| {
                    ai_error(
                        "INVALID_INPUT",
                        "scout_context_pack.anchors[].line_count must be positive integer",
                    )
                })?;
                if n == 0 {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        "scout_context_pack.anchors[].line_count must be > 0",
                    ));
                }
                n as u32
            }
            None => content.lines().count().max(1) as u32,
        };
        let meta_hint = anchor
            .get("meta_hint")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);

        anchors_out.push(json!({
            "id": id,
            "anchor_type": anchor_type,
            "rationale": rationale,
            "code_ref": code_ref,
            "content": content,
            "line_count": line_count,
            "meta_hint": meta_hint
        }));
    }
    if anchors_out.len() < SCOUT_MIN_ANCHORS {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("scout_context_pack.anchors must contain at least {SCOUT_MIN_ANCHORS} items"),
        ));
    }

    let change_hints_raw = obj
        .get("change_hints")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "scout_context_pack.change_hints: expected array",
            )
        })?;
    let mut change_hints = Vec::<Value>::new();
    for item in change_hints_raw {
        let o = require_object(item, "scout_context_pack.change_hints[]")?;
        let raw_path = require_string(o, "path", "scout_context_pack.change_hints[]")?;
        let normalized_path = normalize_claim_path_key(&raw_path, repo_root.as_deref())?;
        if !change_hint_path_bound(
            &raw_path,
            &normalized_path,
            &bound_path_keys,
            repo_root.as_deref(),
        ) {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "scout_context_pack.change_hints[].path must be bound by code_refs/anchors CODE_REF paths: {raw_path}"
                ),
            ));
        }
        let intent = require_string(o, "intent", "scout_context_pack.change_hints[]")?;
        let risk = require_string(o, "risk", "scout_context_pack.change_hints[]")?;
        change_hints.push(json!({
            "path": normalized_path,
            "intent": intent,
            "risk": risk
        }));
    }
    if change_hints.len() < SCOUT_MIN_CHANGE_HINTS {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!(
                "scout_context_pack.change_hints must contain at least {SCOUT_MIN_CHANGE_HINTS} items"
            ),
        ));
    }
    let mut change_signature_set = HashSet::<String>::new();
    let mut duplicate_change_hints = 0usize;
    for item in &change_hints {
        if let Some(obj) = item.as_object() {
            let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or_default();
            let intent = obj
                .get("intent")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let signature = normalize_signature(&format!("{path} {intent}"));
            if !signature.is_empty() && !change_signature_set.insert(signature) {
                duplicate_change_hints = duplicate_change_hints.saturating_add(1);
            }
        }
    }
    enforce_uniqueness_by_policy(
        &mut warnings,
        policy,
        "scout_context_pack.change_hints",
        "path+intent entries",
        duplicate_change_hints,
    )?;

    let test_hints = scout_test_hints_array(obj, "test_hints", "scout_context_pack")?;
    let mut unique_test_hints = HashSet::<String>::new();
    let mut duplicate_test_hints = 0usize;
    for hint in &test_hints {
        let signature = normalize_signature(hint);
        if !signature.is_empty() && !unique_test_hints.insert(signature) {
            duplicate_test_hints = duplicate_test_hints.saturating_add(1);
        }
    }
    enforce_uniqueness_by_policy(
        &mut warnings,
        policy,
        "scout_context_pack.test_hints",
        "test hints",
        duplicate_test_hints,
    )?;
    let min_test_hints = if matches!(policy.quality_profile, ScoutQualityProfile::Flagship) {
        policy.required_test_hints.max(SCOUT_MIN_TEST_HINTS)
    } else {
        SCOUT_MIN_TEST_HINTS
    };
    if test_hints.len() < min_test_hints {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("scout_context_pack.test_hints must contain at least {min_test_hints} items"),
        ));
    }

    let risk_map = obj
        .get("risk_map")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "scout_context_pack.risk_map: expected array",
            )
        })?
        .iter()
        .map(|item| {
            let o = require_object(item, "scout_context_pack.risk_map[]")?;
            Ok(json!({
                "risk": require_string(o, "risk", "scout_context_pack.risk_map[]")?,
                "falsifier": require_string(o, "falsifier", "scout_context_pack.risk_map[]")?
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;
    let min_risk_map = if matches!(policy.quality_profile, ScoutQualityProfile::Flagship) {
        policy.required_risk_falsifier_pairs.max(SCOUT_MIN_RISK_MAP)
    } else {
        SCOUT_MIN_RISK_MAP
    };
    if risk_map.len() < min_risk_map {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("scout_context_pack.risk_map must contain at least {min_risk_map} items"),
        ));
    }
    let mut unique_risks = HashSet::<String>::new();
    let mut duplicate_risks = 0usize;
    for item in &risk_map {
        if let Some(obj) = item.as_object() {
            let risk = obj.get("risk").and_then(|v| v.as_str()).unwrap_or_default();
            let signature = normalize_signature(risk);
            if !signature.is_empty() && !unique_risks.insert(signature) {
                duplicate_risks = duplicate_risks.saturating_add(1);
            }
        }
    }
    enforce_uniqueness_by_policy(
        &mut warnings,
        policy,
        "scout_context_pack.risk_map",
        "risk entries",
        duplicate_risks,
    )?;
    let open_questions = string_array(obj, "open_questions", "scout_context_pack")?;

    let summary_for_builder = require_string(obj, "summary_for_builder", "scout_context_pack")?;
    let min_summary_chars = if matches!(policy.quality_profile, ScoutQualityProfile::Flagship) {
        SCOUT_FLAGSHIP_MIN_SUMMARY_CHARS
    } else {
        SCOUT_MIN_SUMMARY_CHARS
    };
    if summary_for_builder.chars().count() < min_summary_chars {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("scout_context_pack.summary_for_builder must be >= {min_summary_chars} chars"),
        ));
    }
    if summary_for_builder.chars().count() > 1200 {
        return Err(ai_error(
            "INVALID_INPUT",
            "scout_context_pack.summary_for_builder must be <= 1200 chars",
        ));
    }
    let architecture_map = match obj.get("architecture_map") {
        Some(value) => {
            let map_obj = require_object(value, "scout_context_pack.architecture_map")?;
            let nodes = string_array(map_obj, "nodes", "scout_context_pack.architecture_map")?;
            if nodes.is_empty() {
                return Err(ai_error(
                    "INVALID_INPUT",
                    "scout_context_pack.architecture_map.nodes must not be empty",
                ));
            }
            let edges = string_array(map_obj, "edges", "scout_context_pack.architecture_map")?;
            let entrypoints = string_array(
                map_obj,
                "entrypoints",
                "scout_context_pack.architecture_map",
            )?;
            let critical_paths = string_array(
                map_obj,
                "critical_paths",
                "scout_context_pack.architecture_map",
            )?;
            Some(json!({
                "nodes": nodes,
                "edges": edges,
                "entrypoints": entrypoints,
                "critical_paths": critical_paths
            }))
        }
        None => None,
    };
    let mermaid_compact = match obj.get("mermaid_compact") {
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                let line_count = trimmed.lines().count();
                if line_count > 20 {
                    return Err(ai_error(
                        "INVALID_INPUT",
                        "scout_context_pack.mermaid_compact must be <= 20 lines",
                    ));
                }
                Some(trimmed.to_string())
            }
        }
        Some(_) => {
            return Err(ai_error(
                "INVALID_INPUT",
                "scout_context_pack.mermaid_compact must be string",
            ));
        }
        None => None,
    };

    let mut normalized = json!({
        "objective": objective,
        "scope": { "in": scope_in, "out": scope_out },
        "anchors": anchors_out,
        "code_refs": code_refs,
        "change_hints": change_hints,
        "test_hints": test_hints,
        "risk_map": risk_map,
        "open_questions": open_questions,
        "summary_for_builder": summary_for_builder
    });
    if let Some(map) = architecture_map
        && let Some(obj) = normalized.as_object_mut()
    {
        obj.insert("architecture_map".to_string(), map);
    }
    if let Some(mermaid) = mermaid_compact
        && let Some(obj) = normalized.as_object_mut()
    {
        obj.insert("mermaid_compact".to_string(), Value::String(mermaid));
    }

    let mut signature_counts = HashMap::<String, usize>::new();
    for sig in &anchor_signatures {
        if !sig.is_empty() {
            *signature_counts.entry(sig.clone()).or_insert(0) += 1;
        }
    }
    let duplicate_anchor_count = signature_counts.values().filter(|v| **v > 1).count();
    let anchor_overlap_ratio = if anchor_signatures.is_empty() {
        0.0
    } else {
        duplicate_anchor_count as f64 / anchor_signatures.len() as f64
    };
    if anchor_overlap_ratio > policy.max_anchor_overlap {
        let message = format!(
            "scout_context_pack contains overlapping anchors (ratio={anchor_overlap_ratio:.2}, allowed<={:.2})",
            policy.max_anchor_overlap
        );
        if matches!(policy.novelty_policy, ScoutNoveltyPolicy::Strict) {
            return Err(ai_error("INVALID_INPUT", &message));
        }
        push_warning(&mut warnings, "SCOUT_NOVELTY_WARN", message);
    }

    let unique_code_refs = code_refs.iter().collect::<HashSet<_>>().len();
    let computed_ref_redundancy = if code_refs.is_empty() {
        0.0
    } else {
        1.0 - (unique_code_refs as f64 / code_refs.len() as f64)
    };
    if computed_ref_redundancy > policy.max_ref_redundancy {
        let message = format!(
            "scout_context_pack code_refs redundancy too high (ratio={computed_ref_redundancy:.2}, allowed<={:.2})",
            policy.max_ref_redundancy
        );
        if matches!(policy.novelty_policy, ScoutNoveltyPolicy::Strict) {
            return Err(ai_error("INVALID_INPUT", &message));
        }
        push_warning(&mut warnings, "SCOUT_NOVELTY_WARN", message);
    }

    if matches!(policy.quality_profile, ScoutQualityProfile::Flagship)
        && let Some(norm) = normalized.as_object_mut()
    {
        // Flagship profile is enforced via deterministic, store-backed checks above
        // (budgets, novelty caps, min anchors/code_refs/test_hints/risk_map, etc.).
        //
        // We intentionally do NOT require extra "semantic" fields here because scout
        // models may be cheap (e.g. Haiku) and must reliably produce valid JSON.
        norm.insert(
            "quality_profile".to_string(),
            Value::String("flagship".to_string()),
        );
    }

    Ok((normalized, warnings))
}

pub(crate) fn validate_execution_evidence_pack(raw: &Value, field: &str) -> Result<Value, Value> {
    let obj = require_object(raw, field)?;
    let revision = obj
        .get("revision")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ai_error("INVALID_INPUT", &format!("{field}.revision is required")))?;
    if revision <= 0 {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.revision must be > 0"),
        ));
    }

    let diff_scope = string_array(obj, "diff_scope", field)?;
    if diff_scope.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.diff_scope must not be empty"),
        ));
    }

    let command_runs = obj
        .get("command_runs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("{field}.command_runs: expected array"),
            )
        })?
        .iter()
        .map(|item| {
            let o = require_object(item, &format!("{field}.command_runs[]"))?;
            let cmd = require_string(o, "cmd", &format!("{field}.command_runs[]"))?;
            let exit_code = o.get("exit_code").and_then(|v| v.as_i64()).ok_or_else(|| {
                ai_error(
                    "INVALID_INPUT",
                    &format!("{field}.command_runs[].exit_code is required"),
                )
            })?;
            let stdout_ref = require_string(o, "stdout_ref", &format!("{field}.command_runs[]"))?;
            let stderr_ref = require_string(o, "stderr_ref", &format!("{field}.command_runs[]"))?;
            Ok(json!({
                "cmd": cmd,
                "exit_code": exit_code,
                "stdout_ref": stdout_ref,
                "stderr_ref": stderr_ref
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;
    if command_runs.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.command_runs must not be empty"),
        ));
    }

    let rollback_obj = require_object(
        obj.get("rollback_proof").ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("{field}.rollback_proof is required"),
            )
        })?,
        &format!("{field}.rollback_proof"),
    )?;
    let rollback_strategy =
        require_string(rollback_obj, "strategy", &format!("{field}.rollback_proof"))?;
    let rollback_target_revision = rollback_obj
        .get("target_revision")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("{field}.rollback_proof.target_revision is required"),
            )
        })?;
    let rollback_verification = require_string(
        rollback_obj,
        "verification_cmd_ref",
        &format!("{field}.rollback_proof"),
    )?;
    if !(rollback_verification.starts_with("CMD:")
        || rollback_verification.starts_with("LINK:")
        || rollback_verification.starts_with("FILE:"))
    {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.rollback_proof.verification_cmd_ref must use CMD:/LINK:/FILE:"),
        ));
    }

    let guards_obj = require_object(
        obj.get("semantic_guards").ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                &format!("{field}.semantic_guards is required"),
            )
        })?,
        &format!("{field}.semantic_guards"),
    )?;
    let must_should_may_delta = require_string(
        guards_obj,
        "must_should_may_delta",
        &format!("{field}.semantic_guards"),
    )?;
    let contract_term_consistency = require_string(
        guards_obj,
        "contract_term_consistency",
        &format!("{field}.semantic_guards"),
    )?;

    Ok(json!({
        "revision": revision,
        "diff_scope": diff_scope,
        "command_runs": command_runs,
        "rollback_proof": {
            "strategy": rollback_strategy,
            "target_revision": rollback_target_revision,
            "verification_cmd_ref": rollback_verification
        },
        "semantic_guards": {
            "must_should_may_delta": must_should_may_delta,
            "contract_term_consistency": contract_term_consistency
        }
    }))
}

pub(crate) fn validate_builder_diff_batch(raw: &Value) -> Result<Value, Value> {
    let obj = require_object(raw, "builder_diff_batch")?;
    let slice_id = require_string(obj, "slice_id", "builder_diff_batch")?;
    let changes = obj
        .get("changes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "builder_diff_batch.changes: expected array",
            )
        })?;
    let context_request = match obj.get("context_request") {
        Some(v) => Some(validate_builder_context_request(
            v,
            "builder_diff_batch.context_request",
        )?),
        None => None,
    };
    if changes.is_empty() && context_request.is_none() {
        return Err(ai_error(
            "INVALID_INPUT",
            "builder_diff_batch.changes must not be empty",
        ));
    }
    if !changes.is_empty() && context_request.is_some() {
        return Err(ai_error(
            "INVALID_INPUT",
            "builder_diff_batch.context_request requires changes=[] for unambiguous context-only rework",
        ));
    }
    let changes_norm = changes
        .iter()
        .map(|item| {
            let o = require_object(item, "builder_diff_batch.changes[]")?;
            Ok(json!({
                "path": require_string(o, "path", "builder_diff_batch.changes[]")?,
                "intent": require_string(o, "intent", "builder_diff_batch.changes[]")?,
                "diff_ref": require_string(o, "diff_ref", "builder_diff_batch.changes[]")?,
                "estimated_risk": o.get("estimated_risk").cloned().unwrap_or(Value::String("medium".to_string()))
            }))
        })
        .collect::<Result<Vec<_>, Value>>()?;

    let checks_to_run = string_array(obj, "checks_to_run", "builder_diff_batch")?;
    if checks_to_run.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            "builder_diff_batch.checks_to_run must not be empty",
        ));
    }
    let rollback_plan = require_string(obj, "rollback_plan", "builder_diff_batch")?;
    let proof_refs = string_array(obj, "proof_refs", "builder_diff_batch")?;
    if proof_refs.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            "builder_diff_batch.proof_refs must not be empty",
        ));
    }
    if proof_refs.iter().any(|r| {
        let t = r.trim_start();
        !(t.starts_with("CMD:") || t.starts_with("LINK:") || t.starts_with("FILE:"))
    }) {
        return Err(ai_error(
            "INVALID_INPUT",
            "builder_diff_batch.proof_refs[] must use CMD:/LINK:/FILE:",
        ));
    }
    let execution_evidence = validate_execution_evidence_pack(
        obj.get("execution_evidence").ok_or_else(|| {
            ai_error(
                "INVALID_INPUT",
                "builder_diff_batch.execution_evidence is required",
            )
        })?,
        "builder_diff_batch.execution_evidence",
    )?;

    let mut normalized = json!({
        "slice_id": slice_id,
        "changes": changes_norm,
        "checks_to_run": checks_to_run,
        "rollback_plan": rollback_plan,
        "proof_refs": proof_refs,
        "execution_evidence": execution_evidence
    });
    if let Some(req) = context_request
        && let Some(obj) = normalized.as_object_mut()
    {
        obj.insert("context_request".to_string(), req);
    }

    Ok(normalized)
}

fn validate_builder_context_request(raw: &Value, field: &str) -> Result<Value, Value> {
    let obj = require_object(raw, field)?;
    let reason = require_string(obj, "reason", field)?;
    let missing_context = string_array(obj, "missing_context", field)?;
    if missing_context.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("{field}.missing_context must not be empty"),
        ));
    }
    let suggested_scout_focus = string_array(obj, "suggested_scout_focus", field)?;
    let suggested_tests = string_array(obj, "suggested_tests", field)?;
    Ok(json!({
        "reason": reason,
        "missing_context": missing_context,
        "suggested_scout_focus": suggested_scout_focus,
        "suggested_tests": suggested_tests
    }))
}

// --- Scout v2 ---

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScoutAnchorType {
    Primary,
    Dependency,
    Reference,
    Structural,
}

impl ScoutAnchorType {
    pub(crate) fn from_str(raw: &str) -> Result<Self, Value> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "primary" => Ok(Self::Primary),
            "dependency" => Ok(Self::Dependency),
            "reference" => Ok(Self::Reference),
            "structural" => Ok(Self::Structural),
            _ => Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "scout_anchor.anchor_type must be primary|dependency|reference|structural, got: {raw}"
                ),
            )),
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Dependency => "dependency",
            Self::Reference => "reference",
            Self::Structural => "structural",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ScoutAnchorV2 {
    pub id: String,
    pub anchor_type: ScoutAnchorType,
    pub rationale: String,
    pub code_ref: String,
    pub content: String,
    pub line_count: u32,
    pub meta_hint: Option<String>,
}
