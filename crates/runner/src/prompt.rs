#![forbid(unsafe_code)]

use serde_json::Value;

use super::{RunnerConfig, value_as_str};

pub(crate) fn sanitize_single_line(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
}

pub(crate) fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    let sanitized = sanitize_single_line(text).trim().to_string();
    if sanitized.chars().count() <= max_chars {
        return sanitized;
    }
    let mut out = String::new();
    for (i, ch) in sanitized.chars().enumerate() {
        if i >= max_chars.saturating_sub(1) {
            break;
        }
        out.push(ch);
    }
    out.push('\u{2026}');
    out
}

pub(crate) fn render_job_thread(opened: &Value) -> String {
    const MAX_EVENTS: usize = 12;
    const MAX_MESSAGE_CHARS: usize = 160;
    const MAX_REFS: usize = 3;

    let Some(events) = opened.get("events").and_then(|v| v.as_array()) else {
        return String::new();
    };

    // events are newest-first; filter, then show oldest->newest for readability.
    let mut picked: Vec<&Value> = Vec::new();
    for ev in events {
        let kind = ev.get("kind").and_then(value_as_str).unwrap_or("");
        if kind.eq_ignore_ascii_case("heartbeat") {
            continue;
        }
        let msg = ev.get("message").and_then(value_as_str).unwrap_or("");
        if msg.to_ascii_lowercase().starts_with("runner:") {
            continue;
        }
        picked.push(ev);
        if picked.len() >= MAX_EVENTS {
            break;
        }
    }
    picked.reverse();

    let mut lines: Vec<String> = Vec::new();
    for ev in picked {
        let kind = ev.get("kind").and_then(value_as_str).unwrap_or("event");
        let message = truncate_for_prompt(
            ev.get("message").and_then(value_as_str).unwrap_or("-"),
            MAX_MESSAGE_CHARS,
        );

        let mut refs: Vec<String> = ev
            .get("refs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .take(MAX_REFS)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        refs.retain(|r| !r.starts_with("JOB-"));

        if refs.is_empty() {
            lines.push(format!("- {kind}: {message}"));
        } else {
            lines.push(format!("- {kind}: {message} (refs: {})", refs.join(", ")));
        }
    }

    if lines.is_empty() {
        "(no messages)".to_string()
    } else {
        lines.join("\n")
    }
}

pub(crate) fn extract_cascade_retry_context(meta: Option<&Value>) -> String {
    let Some(obj) = meta.and_then(|v| v.as_object()) else {
        return String::new();
    };
    // cascade_retry_hints: array of strings from pre/post validator.
    let hints = obj
        .get("cascade_retry_hints")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n- ")
        });
    // cascade_retry_feedback: string from previous attempt failure.
    let feedback = obj
        .get("cascade_retry_feedback")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    // cascade_previous_ref: artifact ref from previous scout/writer.
    let prev_ref = obj
        .get("cascade_previous_ref")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let mut out = String::new();
    if let Some(h) = hints.filter(|s| !s.is_empty()) {
        out.push_str(&format!("\nRETRY CONTEXT (from validator):\n- {h}\n"));
    }
    if let Some(fb) = feedback {
        out.push_str(&format!("RETRY FEEDBACK:\n{fb}\n"));
    }
    if let Some(pr) = prev_ref {
        out.push_str(&format!("Previous attempt ref: {pr}\n"));
    }
    out
}

pub(crate) fn build_subagent_prompt(
    cfg: &RunnerConfig,
    job_id: &str,
    job_prompt: &str,
    slice_context: &str,
    skill_pack: &str,
    pipeline_role: Option<&str>,
    job_meta: Option<&Value>,
) -> String {
    let skill_section = if skill_pack.trim().is_empty() {
        String::new()
    } else {
        format!("SKILL PACK (bounded):\n{skill_pack}\n\n")
    };

    let role_contract = match pipeline_role.map(|v| v.trim().to_ascii_lowercase()) {
        Some(role) if role == "scout" => r#"PIPELINE ROLE: SCOUT
MUST return context-only output.
MUST NOT include code/patch/diff/apply instructions.
In `summary`, provide a JSON object `scout_context_pack` with keys:
objective, scope{in,out}, anchors{id,rationale}, code_refs, change_hints,
test_hints, risk_map{risk,falsifier}, open_questions, summary_for_builder.
Each `code_refs[]` item MUST be a CODE_REF token:
`code:<repo_rel_path>#L<start>-L<end>` (optionally `@sha256:<64hex>`; BranchMind normalizes).
Every anchor in `anchors[]` MUST include `anchor_type` + `code_ref` for pre-validator lineage.
For very large code areas, prefer layered anchors: first 8-12 primary anchors, then extended anchors up to 64 max.
QUALITY MINIMA: anchors>=3, change_hints>=2, test_hints>=3, risk_map>=3,
summary_for_builder>=320 chars.
"# 
        .to_string(),
        Some(role) if role == "builder" => r#"PIPELINE ROLE: BUILDER
In `summary`, provide a JSON object `builder_diff_batch` with keys:
slice_id, changes[{path,intent,diff_ref,estimated_risk}], checks_to_run,
rollback_plan, proof_refs,
execution_evidence{
  revision,
  diff_scope,
  command_runs[{cmd,exit_code,stdout_ref,stderr_ref}],
  rollback_proof{strategy,target_revision,verification_cmd_ref},
  semantic_guards{must_should_may_delta,contract_term_consistency}
}.
MUST keep `proof_refs[]` strict: each entry starts with `CMD:` or `LINK:` or `FILE:`.
MUST keep `execution_evidence.command_runs[]` fully shaped:
{cmd,exit_code,stdout_ref,stderr_ref} for every run.
MUST keep `execution_evidence.rollback_proof.verification_cmd_ref` strict:
starts with `CMD:` or `LINK:` or `FILE:`.
"#
        .to_string(),
        Some(role) if role == "writer" => r#"PIPELINE ROLE: WRITER
You receive scout context with annotated code slices. Your ONLY output is patches.
MUST NOT write files to disk. MUST NOT run shell commands.
In `summary`, provide a JSON object `writer_patch_pack` with keys:
slice_id, patches[{path, ops[{kind, old_lines, new_lines, anchor_ref, after, before, content}]}],
summary, affected_files, checks_to_run, insufficient_context (optional escape hatch).
PatchOp kinds: replace, insert_after, insert_before, create_file, delete_file.
For `replace`: old_lines (exact match) + new_lines (replacement). anchor_ref optional.
For `insert_after`/`insert_before`: after/before (context lines) + content (new lines).
For `create_file`: content (full file). For `delete_file`: no extra fields.
If scout context is insufficient, set `insufficient_context` string and leave `patches` empty.
"#
        .to_string(),
        Some(role) if role == "validator" => r#"PIPELINE ROLE: VALIDATOR
In `summary`, provide a JSON object `validator_report` with keys:
slice_id, plan_fit_score, policy_checks, tests, security_findings,
regression_risk, recommendation, rework_actions.
"#
        .to_string(),
        _ => String::new(),
    };

    let retry_context = extract_cascade_retry_context(job_meta);

    format!(
        "You are a delegated coding agent.\n\
You MUST return a single JSON object that matches the provided output schema.\n\
Do not include extra keys.\n\
Keep summary short.\n\
Put stable BranchMind refs into refs[] (TASK-*, JOB-*, CARD-*, notes@seq, a:*).\n\
Proof gate: status=\"DONE\" requires at least one non-job ref (e.g., CARD-* / TASK-* / notes@seq / LINK:/CMD:).\n\
Always include events[] in the final JSON (use [] if none).\n\
Each events[] item MUST include all keys: kind, message, percent, refs (use percent=0 if unknown; refs=[] if none).\n\
\n\
EXAMPLE OUTPUT (valid JSON):\n\
{{\"status\":\"DONE\",\"summary\":\"...\",\"refs\":[\"CMD: cargo test -q\",\"CARD-123\",\"JOB-001\"],\"events\":[{{\"kind\":\"progress\",\"message\":\"...\",\"percent\":10,\"refs\":[\"JOB-001\"]}}]}}\n\
IMPORTANT: Put proof refs (CMD:/LINK:/CARD-/TASK-/notes@seq) into refs[] (or events[].refs). Do not bury them only in summary.\n\
\n\
{role_contract}\
{retry_context}\
\n\
{skill_section}\
FEEDBACK LOOP (low-noise):\n\
- workspace: {workspace}\n\
- job: {job}\n\
If the MCP tool `jobs` is available, send 1–3 short updates while you work (cmd=jobs.report):\n\
- kind: progress|checkpoint|question\n\
- message: short, no logs\n\
- percent: integer (0 if unknown)\n\
- refs: stable ids (CARD-*/TASK-*/notes@seq/a:*)\n\
If you cannot call tools, emit the same updates in the final JSON field `events`.\n\
\n\
MANAGER CONTROL:\n\
- The manager may send messages via `jobs` (cmd=jobs.message).\n\
- Read the JOB THREAD and follow the latest manager instruction.\n\
\n\
TIME-SLICE RULE:\n\
If you cannot fully finish within this slice, return status=\"CONTINUE\" with (a) what you did and (b) the next best action.\n\
\n\
JOB SPEC:\n{job_prompt}\n\n\
{slice_context}\n",
        workspace = cfg.workspace,
        job = job_id,
        job_prompt = job_prompt,
        slice_context = slice_context,
        skill_section = skill_section,
        role_contract = role_contract
    )
}
