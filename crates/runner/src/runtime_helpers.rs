#![forbid(unsafe_code)]

use super::*;

pub(super) fn merged_meta(base: &serde_json::Map<String, Value>, extra: Value) -> Value {
    let mut merged = base.clone();
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            merged.insert(k.clone(), v.clone());
        }
    }
    Value::Object(merged)
}

pub(super) fn push_unique_ref(out: &mut Vec<String>, value: String, seen: &mut HashSet<String>) {
    let v = value.trim();
    if v.is_empty() {
        return;
    }
    if seen.insert(v.to_string()) {
        out.push(v.to_string());
    }
}

pub(super) fn canonicalize_ref(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    // Keep these prefixes copy/paste friendly and stable.
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("cmd:") {
        return Some(format!("CMD:{}", &s[4..]));
    }
    if lower.starts_with("link:") {
        return Some(format!("LINK:{}", &s[5..]));
    }
    if lower.starts_with("card-") {
        return Some(format!("CARD-{}", &s[5..]));
    }
    if lower.starts_with("task-") {
        return Some(format!("TASK-{}", &s[5..]));
    }
    if lower.starts_with("notes@") {
        return Some(format!("notes@{}", &s[6..]));
    }

    Some(s.to_string())
}

pub(super) fn salvage_proof_refs_from_text(text: &str) -> Vec<String> {
    // Goal: reduce false proof-gate CONTINUE when the delegated agent put proof-like refs
    // into summary/messages instead of top-level refs[].
    //
    // Strategy:
    // 1) Prefer full-line extraction for `CMD:` / `LINK:` (they contain spaces).
    // 2) Token extraction for CARD-/TASK-/notes@ references embedded in prose.
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let lower = l.to_ascii_lowercase();
        if let Some(norm) = lower
            .find("cmd:")
            .and_then(|idx| canonicalize_ref(&l[idx..]))
        {
            push_unique_ref(&mut out, norm, &mut seen);
        }
        if let Some(norm) = lower
            .find("link:")
            .and_then(|idx| canonicalize_ref(&l[idx..]))
        {
            push_unique_ref(&mut out, norm, &mut seen);
        }

        // Some agents write proof as markdown bullets without CMD:/LINK: prefixes.
        // We salvage only when the bullet strongly looks like a shell command
        // (avoid turning prose into fake proof).
        let mut bullet = l;
        if let Some(rest) = bullet.strip_prefix("- ") {
            bullet = rest.trim();
        } else if let Some(rest) = bullet.strip_prefix("* ") {
            bullet = rest.trim();
        } else if let Some(rest) = bullet.strip_prefix("• ") {
            bullet = rest.trim();
        }
        if let Some(rest) = bullet.strip_prefix("$ ") {
            bullet = rest.trim();
        } else if let Some(rest) = bullet.strip_prefix("> ") {
            bullet = rest.trim();
        }
        if !bullet.is_empty() {
            let b = bullet.to_ascii_lowercase();
            let looks_like_cmd = [
                "cargo ",
                "cargo",
                "pytest",
                "go test",
                "npm ",
                "pnpm ",
                "yarn ",
                "bun ",
                "make ",
                "just ",
                "git ",
                "rg ",
                "python ",
                "python3 ",
                "node ",
                "deno ",
                "docker ",
                "kubectl ",
                "helm ",
                "terraform ",
            ]
            .into_iter()
            .any(|p| b == p || b.starts_with(p));
            if looks_like_cmd {
                push_unique_ref(&mut out, format!("CMD: {bullet}"), &mut seen);
            }
        }
    }

    // Tokenize on common separators; keep it cheap and dependency-free.
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
            )
    }) {
        let token =
            raw.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '`'));
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            push_unique_ref(&mut out, format!("LINK: {token}"), &mut seen);
            continue;
        }
        if (lower.starts_with("card-") || lower.starts_with("task-") || lower.starts_with("notes@"))
            && let Some(norm) = canonicalize_ref(token)
        {
            push_unique_ref(&mut out, norm, &mut seen);
        }
    }

    out
}

pub(super) fn value_as_str(v: &Value) -> Option<&str> {
    v.as_str().map(|s| s.trim()).filter(|s| !s.is_empty())
}

pub(super) fn summary_value_to_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(value @ Value::Object(_)) | Some(value @ Value::Array(_)) => {
            serde_json::to_string(value).unwrap_or_else(|_| "-".to_string())
        }
        Some(other) => {
            let rendered = other.to_string();
            if rendered.trim().is_empty() {
                "-".to_string()
            } else {
                rendered
            }
        }
        None => "-".to_string(),
    }
}

pub(super) fn normalize_builder_summary_revision(summary: &str, claim_revision: i64) -> String {
    let Ok(mut parsed) = serde_json::from_str::<Value>(summary) else {
        return summary.to_string();
    };
    let Some(obj) = parsed.as_object_mut() else {
        return summary.to_string();
    };
    let Some(evidence) = obj
        .get_mut("execution_evidence")
        .and_then(|v| v.as_object_mut())
    else {
        return summary.to_string();
    };
    let stamped_revision = claim_revision.saturating_add(1).max(1);
    evidence.insert("revision".to_string(), json!(stamped_revision));
    serde_json::to_string(&parsed).unwrap_or_else(|_| summary.to_string())
}

fn dedup_string_array_in_place(items: &mut Vec<Value>) -> bool {
    let before = items.len();
    let mut seen = HashSet::<String>::new();
    items.retain(|item| {
        let Some(raw) = item.as_str() else {
            return false;
        };
        let token = raw.trim();
        if token.is_empty() {
            return false;
        }
        seen.insert(token.to_string())
    });
    items.len() != before
}

fn code_ref_path_key(code_ref: &str) -> Option<String> {
    code_ref
        .strip_prefix("code:")
        .and_then(|rest| rest.split_once("#L").map(|(path, _)| path))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| path.to_ascii_lowercase())
}

fn change_hint_path_is_covered(path_key: &str, covered_paths: &HashSet<String>) -> bool {
    if path_key.is_empty() {
        return false;
    }
    if covered_paths.contains(path_key) {
        return true;
    }
    let directory = path_key.trim_end_matches('/');
    if directory.is_empty() || directory == "." {
        return false;
    }
    let prefix = format!("{directory}/");
    covered_paths
        .iter()
        .any(|covered| covered.starts_with(&prefix))
}

fn collect_anchor_paths(anchors: &[Value], primary_structural_only: bool) -> HashSet<String> {
    anchors
        .iter()
        .filter_map(|anchor| anchor.as_object())
        .filter(|obj| {
            if !primary_structural_only {
                return true;
            }
            obj.get("anchor_type")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .map(|raw| {
                    raw.eq_ignore_ascii_case("primary") || raw.eq_ignore_ascii_case("structural")
                })
                .unwrap_or(false)
        })
        .filter_map(|obj| obj.get("code_ref").and_then(|v| v.as_str()))
        .filter_map(code_ref_path_key)
        .collect::<HashSet<_>>()
}

fn ensure_scout_anchor_coverage(obj: &mut serde_json::Map<String, Value>) -> bool {
    let change_paths = obj
        .get("change_hints")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|hint| hint.as_object())
                .filter_map(|hint| hint.get("path").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(|path| path.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if change_paths.is_empty() {
        return false;
    }

    let mut code_ref_by_path = std::collections::HashMap::<String, String>::new();
    if let Some(code_refs) = obj.get("code_refs").and_then(|v| v.as_array()) {
        for raw in code_refs.iter().filter_map(|v| v.as_str()) {
            if let Some(path_key) = code_ref_path_key(raw) {
                code_ref_by_path
                    .entry(path_key)
                    .or_insert_with(|| raw.trim().to_string());
            }
        }
    }

    let Some(anchors) = obj.get_mut("anchors").and_then(|v| v.as_array_mut()) else {
        return false;
    };

    let mut covered_paths = collect_anchor_paths(anchors, true);
    if covered_paths.is_empty() {
        covered_paths = collect_anchor_paths(anchors, false);
    }

    let mut existing_ids = anchors
        .iter()
        .filter_map(|anchor| anchor.as_object())
        .filter_map(|obj| obj.get("id").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();

    let mut changed = false;
    let mut synthetic_seq: u32 = 1;
    for path_key in change_paths {
        if change_hint_path_is_covered(&path_key, &covered_paths) {
            continue;
        }

        let mut promoted = false;
        for anchor in anchors.iter_mut() {
            let Some(anchor_obj) = anchor.as_object_mut() else {
                continue;
            };
            let anchor_path = anchor_obj
                .get("code_ref")
                .and_then(|v| v.as_str())
                .and_then(code_ref_path_key);
            if anchor_path.as_deref() != Some(path_key.as_str()) {
                continue;
            }

            let is_primary_or_structural = anchor_obj
                .get("anchor_type")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .map(|raw| {
                    raw.eq_ignore_ascii_case("primary") || raw.eq_ignore_ascii_case("structural")
                })
                .unwrap_or(false);
            if !is_primary_or_structural {
                anchor_obj.insert("anchor_type".to_string(), json!("structural"));
                changed = true;
            }
            let content_missing = anchor_obj
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .map(|s| s.is_empty())
                .unwrap_or(true);
            if content_missing {
                let fallback = anchor_obj
                    .get("rationale")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(path_key.as_str())
                    .to_string();
                anchor_obj.insert("content".to_string(), Value::String(fallback));
                changed = true;
            }
            if anchor_obj
                .get("line_count")
                .and_then(|v| v.as_u64())
                .is_none()
            {
                anchor_obj.insert("line_count".to_string(), json!(1));
                changed = true;
            }
            covered_paths.insert(path_key.clone());
            promoted = true;
            break;
        }

        if promoted {
            continue;
        }

        let Some(code_ref) = code_ref_by_path.get(&path_key).cloned() else {
            continue;
        };
        let new_id = loop {
            let candidate = format!("a:auto-coverage-{synthetic_seq}");
            synthetic_seq = synthetic_seq.saturating_add(1);
            if existing_ids.insert(candidate.clone()) {
                break candidate;
            }
        };
        anchors.push(json!({
            "id": new_id,
            "anchor_type": "structural",
            "rationale": format!("Auto-synthesized structural anchor for change_hints path `{path_key}`."),
            "code_ref": code_ref,
            "content": format!("Auto coverage anchor for `{path_key}`."),
            "line_count": 1,
            "meta_hint": "auto_synthesized_coverage_anchor"
        }));
        covered_paths.insert(path_key);
        changed = true;
    }

    changed
}

fn clamp_object_code_refs(
    obj: &mut serde_json::Map<String, Value>,
    max_context_refs: usize,
) -> bool {
    let Some(code_refs) = obj.get_mut("code_refs").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let mut changed = dedup_string_array_in_place(code_refs);
    if max_context_refs > 0 && code_refs.len() > max_context_refs {
        code_refs.truncate(max_context_refs);
        changed = true;
    }
    changed
}

fn clamp_and_normalize_scout_pack(
    obj: &mut serde_json::Map<String, Value>,
    max_context_refs: usize,
) -> bool {
    let mut changed = clamp_object_code_refs(obj, max_context_refs);
    changed |= ensure_scout_anchor_coverage(obj);
    changed
}

pub(super) fn clamp_scout_summary_code_refs(summary: &str, max_context_refs: usize) -> String {
    let Ok(mut parsed) = serde_json::from_str::<Value>(summary) else {
        return summary.to_string();
    };

    let mut changed = false;
    if let Some(obj) = parsed.as_object_mut() {
        changed = clamp_and_normalize_scout_pack(obj, max_context_refs);
        if let Some(inner) = obj
            .get_mut("scout_context_pack")
            .and_then(|v| v.as_object_mut())
        {
            changed |= clamp_and_normalize_scout_pack(inner, max_context_refs);
        }
    }
    if !changed {
        return summary.to_string();
    }
    serde_json::to_string(&parsed).unwrap_or_else(|_| summary.to_string())
}

pub(super) fn normalize_task_snapshot_lines(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("ERROR:")
        || trimmed.contains("\"success\":false")
        || trimmed.contains("\"code\":\"UNKNOWN_ID\"")
    {
        "task snapshot unavailable (bounded); proceed with provided scout context + DoD."
            .to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn detect_tool_calls_from_stderr(stderr_path: &Path, limit: usize) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(stderr_path) else {
        return Vec::new();
    };
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with("tool ") {
            continue;
        }
        let marker = crate::prompt::sanitize_single_line(line);
        if marker.is_empty() || !seen.insert(marker.clone()) {
            continue;
        }
        out.push(marker);
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub(super) fn build_builder_input_only_context_request_summary(
    slice_id: Option<&str>,
    claim_revision: i64,
    tool_calls: &[String],
    stderr_path: &Path,
) -> String {
    let stamped_revision = claim_revision.saturating_add(1).max(1);
    let stderr_ref = format!("FILE:{}", stderr_path.display());
    let mut missing_context = vec![
        "input_mode=strict forbids builder-side tool/repo discovery".to_string(),
        "provide all required context via refreshed scout_context_pack".to_string(),
    ];
    for marker in tool_calls.iter().take(3) {
        missing_context.push(format!("blocked tool call: {marker}"));
    }
    let summary = json!({
        "slice_id": slice_id.unwrap_or("unknown"),
        "changes": [],
        "checks_to_run": [],
        "rollback_plan": "no-op rollback: context-only retry requested before patch generation",
        "proof_refs": [
            "CMD: builder input-only guard triggered context_request",
            stderr_ref
        ],
        "execution_evidence": {
            "revision": stamped_revision,
            "diff_scope": [],
            "command_runs": [{
                "cmd": "input_only_guard",
                "exit_code": 0,
                "stdout_ref": "FILE:runner/input-only-guard/stdout",
                "stderr_ref": format!("FILE:{}", stderr_path.display())
            }],
            "rollback_proof": {
                "strategy": "no-op",
                "target_revision": stamped_revision,
                "verification_cmd_ref": "CMD: no patch generated; rollback not required"
            },
            "semantic_guards": {
                "must_should_may_delta": "none",
                "contract_term_consistency": "input_only_strict_enforced"
            }
        },
        "context_request": {
            "reason": "builder attempted tool-driven context discovery while input_mode=strict",
            "missing_context": missing_context,
            "suggested_scout_focus": [
                "refresh strict CODE_REF anchors for missing loci",
                "expand architecture_map + mermaid_compact for unresolved areas"
            ],
            "suggested_tests": [
                "cargo test -p bm_mcp --test jobs_ai_first_ux",
                "cargo test -p bm_mcp --test pipeline_v2_integration"
            ]
        }
    });
    serde_json::to_string(&summary).unwrap_or_else(|_| {
        "{\"slice_id\":\"unknown\",\"changes\":[],\"checks_to_run\":[],\"rollback_plan\":\"context request fallback\",\"proof_refs\":[\"CMD: input-only guard\"],\"execution_evidence\":{\"revision\":1,\"diff_scope\":[],\"command_runs\":[{\"cmd\":\"input_only_guard\",\"exit_code\":0,\"stdout_ref\":\"FILE:runner/input-only-guard/stdout\",\"stderr_ref\":\"FILE:runner/input-only-guard/stderr\"}],\"rollback_proof\":{\"strategy\":\"no-op\",\"target_revision\":1,\"verification_cmd_ref\":\"CMD: no patch generated\"},\"semantic_guards\":{\"must_should_may_delta\":\"none\",\"contract_term_consistency\":\"input_only_strict_enforced\"}},\"context_request\":{\"reason\":\"input_only strict guard\",\"missing_context\":[\"refresh scout pack\"],\"suggested_scout_focus\":[\"refresh CODE_REF\"],\"suggested_tests\":[\"cargo test -q\"]}}".to_string()
    })
}

pub(super) fn value_as_i64(v: &Value) -> Option<i64> {
    v.as_i64()
}
