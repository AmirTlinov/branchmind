#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) fn parse_response_verbosity(
    args_obj: &serde_json::Map<String, Value>,
    fallback: ResponseVerbosity,
) -> Result<ResponseVerbosity, Value> {
    let raw = match optional_string(args_obj, "verbosity")? {
        Some(v) => v,
        None => return Ok(fallback),
    };
    let trimmed = raw.trim();
    ResponseVerbosity::from_str(trimmed)
        .ok_or_else(|| ai_error("INVALID_INPUT", "verbosity must be one of: full|compact"))
}

pub(super) fn compact_open_result(id: &str, result: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("id".to_string(), Value::String(id.to_string()));
    if let Some(workspace) = result.get("workspace") {
        out.insert("workspace".to_string(), workspace.clone());
    }
    if let Some(kind) = result.get("kind") {
        out.insert("kind".to_string(), kind.clone());
    }
    let kind_str = result.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(ref_val) = result.get("ref") {
        out.insert("ref".to_string(), ref_val.clone());
    }
    if let Some(budget) = result.get("budget") {
        out.insert("budget".to_string(), budget.clone());
    }
    if let Some(truncated) = result.get("truncated") {
        out.insert("truncated".to_string(), truncated.clone());
    }
    if let Some(reasoning_ref) = result.get("reasoning_ref") {
        out.insert("reasoning_ref".to_string(), reasoning_ref.clone());
    }
    if let Some(content) = result.get("content") {
        out.insert("content".to_string(), content.clone());
    }
    if let Some(jump) = result.get("jump") {
        out.insert("jump".to_string(), jump.clone());
    }

    // Keep compact slice open operational: binding + ready-to-run orchestration actions.
    if kind_str.eq_ignore_ascii_case("slice") {
        if let Some(slice) = result.get("slice") {
            out.insert("slice".to_string(), slice.clone());
        }
        if let Some(actions) = result.get("actions") {
            out.insert("actions".to_string(), actions.clone());
        }
        if let Some(slice_task) = result.get("slice_task") {
            out.insert("slice_task".to_string(), slice_task.clone());
        }
        if let Some(spec) = result.get("slice_plan_spec") {
            out.insert("slice_plan_spec".to_string(), spec.clone());
        }
        if let Some(steps) = result.get("steps") {
            out.insert("steps".to_string(), steps.clone());
        }
    }
    // Keep compact job-artifact open operational: binding + preview + read action.
    if kind_str.eq_ignore_ascii_case("job_artifact") {
        if let Some(artifact) = result.get("artifact") {
            out.insert("artifact".to_string(), artifact.clone());
        }
        if let Some(text) = result.get("content_text") {
            out.insert("content_text".to_string(), text.clone());
        }
        if let Some(actions) = result.get("actions") {
            out.insert("actions".to_string(), actions.clone());
        }
    }
    if let Some(card) = result.get("card") {
        if let Some(card_id) = card.get("id") {
            out.insert("card_id".to_string(), card_id.clone());
        }
        if let Some(card_type) = card.get("type") {
            out.insert("card_type".to_string(), card_type.clone());
        }
    }
    if let Some(entry) = result.get("entry") {
        if let Some(doc) = entry.get("doc") {
            out.insert("entry_doc".to_string(), doc.clone());
        }
        if let Some(seq) = entry.get("seq") {
            out.insert("entry_seq".to_string(), seq.clone());
        }
    }
    if let Some(capsule) = result.get("capsule") {
        if let Some(focus) = capsule.get("focus") {
            out.insert("focus".to_string(), focus.clone());
        }
        if let Some(action) = capsule.get("action") {
            let mut action_out = serde_json::Map::new();
            if let Some(tool) = action.get("tool") {
                action_out.insert("tool".to_string(), tool.clone());
            }
            if let Some(args) = action.get("args") {
                action_out.insert("args".to_string(), args.clone());
            }
            if !action_out.is_empty() {
                out.insert("next_action".to_string(), Value::Object(action_out));
            }
        }
    }
    Value::Object(out)
}
