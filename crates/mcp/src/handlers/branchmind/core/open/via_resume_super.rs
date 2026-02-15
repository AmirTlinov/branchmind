#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct OpenTargetViaResumeSuperArgs<'a> {
    pub(super) open_id: &'a str,
    pub(super) target_kind: &'a str,
    pub(super) target_key: &'a str,
    pub(super) target_id: &'a str,
    pub(super) include_drafts: bool,
    pub(super) include_content: bool,
    pub(super) max_chars: Option<usize>,
    pub(super) limit: usize,
    pub(super) limit_explicit: bool,
    pub(super) extra_resume_args: Option<serde_json::Map<String, Value>>,
}

pub(super) fn open_target_via_resume_super(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    args: OpenTargetViaResumeSuperArgs<'_>,
) -> Result<(Value, Vec<Value>, Vec<Value>), Value> {
    // `open` is read-only by contract. For targets, delegate to the existing
    // super-resume machinery (budget-aware + deterministic), but shape it
    // into a small navigation-friendly payload.
    let resume_max_chars = args.max_chars.unwrap_or(12_000);
    let resume_max_chars = resume_max_chars.saturating_sub(1_200).max(2_000);
    let resume_max_chars = args
        .max_chars
        .map(|cap| resume_max_chars.min(cap))
        .unwrap_or(resume_max_chars);

    let mut resume_args = serde_json::Map::new();
    resume_args.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );
    resume_args.insert(
        args.target_key.to_string(),
        Value::String(args.target_id.to_string()),
    );
    resume_args.insert("read_only".to_string(), Value::Bool(true));
    resume_args.insert(
        "view".to_string(),
        Value::String(if args.include_drafts {
            "audit".to_string()
        } else {
            "focus_only".to_string()
        }),
    );
    resume_args.insert(
        "max_chars".to_string(),
        Value::Number(serde_json::Number::from(resume_max_chars as i64)),
    );
    if args.limit_explicit {
        resume_args.insert(
            "cards_limit".to_string(),
            Value::Number(serde_json::Number::from(args.limit as i64)),
        );
    }
    if let Some(extra) = args.extra_resume_args {
        resume_args.extend(extra);
    }

    let resume_resp = server.tool_tasks_resume_super(Value::Object(resume_args));
    if !resume_resp
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(resume_resp);
    }

    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();
    if let Some(extra) = resume_resp.get("warnings").and_then(|v| v.as_array()) {
        warnings.extend(extra.iter().cloned());
    }
    if let Some(extra) = resume_resp.get("suggestions").and_then(|v| v.as_array()) {
        suggestions.extend(extra.iter().cloned());
    }

    let resume = resume_resp.get("result").cloned().unwrap_or(Value::Null);
    let truncated = resume
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut out = json!({
        "workspace": workspace.as_str(),
        "kind": args.target_kind,
        "id": args.open_id,
        "target": resume.get("target").cloned().unwrap_or(Value::Null),
        "reasoning_ref": resume.get("reasoning_ref").cloned().unwrap_or(Value::Null),
        "budget": resume.get("budget").cloned().unwrap_or(Value::Null),
        "capsule": resume.get("capsule").cloned().unwrap_or(Value::Null),
        "step_focus": resume.get("step_focus").cloned().unwrap_or(Value::Null),
        "degradation": resume.get("degradation").cloned().unwrap_or(Value::Null),
        "truncated": truncated
    });

    // Portal UX: optionally include the most-used content blocks for the target so
    // agents don't have to bounce between `open` and `tasks.snapshot` for the common
    // “what’s next + what changed” loop.
    if args.include_content
        && let Some(obj) = out.as_object_mut()
    {
        let mut content = serde_json::Map::new();
        for key in [
            "radar",
            "steps",
            "signals",
            "memory",
            "timeline",
            "graph_diff",
        ] {
            if let Some(v) = resume.get(key) {
                content.insert(key.to_string(), v.clone());
            }
        }
        obj.insert("content".to_string(), Value::Object(content));
    }

    Ok((out, warnings, suggestions))
}
