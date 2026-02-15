#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

pub(super) fn trim_compact_open_result_for_budget(v: &mut Value, limit: usize) -> bool {
    let mut changed = false;
    // Compact open results should keep navigation handles; drop payload-heavy fields first.
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["content"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["next_action"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["reasoning_ref"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["focus"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["steps"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["slice_plan_spec"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["slice_task"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["actions"]);
    }
    if json_len_chars(v) > limit {
        // Job artifact previews can be large; trim content_text deterministically first.
        if let Some(Value::String(text)) = v.get_mut("content_text") {
            let target = (limit / 3).clamp(64, 480);
            let trimmed = truncate_string(text, target);
            if *text != trimmed {
                *text = trimmed;
                changed = true;
            }
        }
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &[], &["content_text"]);
    }
    if json_len_chars(v) > limit {
        // Keep artifact binding, drop non-essential artifact fields.
        if let Some(artifact) = v.get_mut("artifact") {
            let (job_id, artifact_key) = match artifact.as_object() {
                Some(obj) => (obj.get("job_id").cloned(), obj.get("artifact_key").cloned()),
                None => (None, None),
            };
            if job_id.is_some() || artifact_key.is_some() {
                let mut out = serde_json::Map::new();
                if let Some(job_id) = job_id {
                    out.insert("job_id".to_string(), job_id);
                }
                if let Some(artifact_key) = artifact_key {
                    out.insert("artifact_key".to_string(), artifact_key);
                }
                *artifact = Value::Object(out);
                changed = true;
            }
        }
    }
    if json_len_chars(v) > limit {
        // Keep top-level binding stable; `budget.truncated` already carries the truncation state.
        changed |= drop_fields_at(v, &[], &["truncated"]);
    }

    if json_len_chars(v) > limit {
        // Last resort: keep binding under tight budgets.
        // Prefer trimming large nested strings first (so callers still get a hint of context),
        // then drop if we still can't fit.
        if let Some(slice) = v.get_mut("slice").and_then(|s| s.as_object_mut())
            && let Some(Value::String(obj)) = slice.get_mut("objective")
        {
            let target = (limit / 4).clamp(64, 320);
            let trimmed = truncate_string(obj, target);
            if *obj != trimmed {
                *obj = trimmed;
                changed = true;
            }
        }
        if let Some(slice) = v.get_mut("slice").and_then(|s| s.as_object_mut())
            && let Some(Value::String(title)) = slice.get_mut("title")
        {
            let target = (limit / 5).clamp(48, 240);
            let trimmed = truncate_string(title, target);
            if *title != trimmed {
                *title = trimmed;
                changed = true;
            }
        }
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["slice"], &["objective"]);
    }
    if json_len_chars(v) > limit {
        changed |= drop_fields_at(v, &["slice"], &["title"]);
    }
    if json_len_chars(v) > limit {
        // Keep the slice binding, drop non-essential slice fields.
        changed |= drop_fields_at(
            v,
            &["slice"],
            &[
                "budgets_json",
                "created_at_ms",
                "updated_at_ms",
                "status",
                // slice_id is redundant with top-level `id` for SLC-*.
                "slice_id",
            ],
        );
    }
    if json_len_chars(v) > limit {
        // Future-proof last resort: if `slice` still doesn't fit (e.g. fat/unknown fields),
        // shrink it to only the binding needed for deterministic navigation.
        if let Some(slice) = v.get_mut("slice") {
            let (plan_id, slice_task_id) = match slice.as_object() {
                Some(obj) => (
                    obj.get("plan_id").cloned(),
                    obj.get("slice_task_id").cloned(),
                ),
                None => (None, None),
            };
            if plan_id.is_some() || slice_task_id.is_some() {
                let mut out = serde_json::Map::new();
                if let Some(plan_id) = plan_id {
                    out.insert("plan_id".to_string(), plan_id);
                }
                if let Some(slice_task_id) = slice_task_id {
                    out.insert("slice_task_id".to_string(), slice_task_id);
                }
                *slice = Value::Object(out);
                changed = true;
            }
        }
    }
    if json_len_chars(v) > limit {
        // Final fallback before `signal=minimal`: drop the whole slice block.
        // This still preserves top-level binding (id/workspace/kind + budget).
        changed |= drop_fields_at(v, &[], &["slice"]);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_open_slice_keeps_binding_when_budgets_json_is_fat() {
        let mut v = json!({
            "id": "SLC-00000001",
            "kind": "slice",
            "workspace": "ws1",
            "truncated": false,
            "slice": {
                "plan_id": "PLAN-1",
                "slice_id": "SLC-00000001",
                "slice_task_id": "TASK-1",
                "title": "Title",
                "objective": "Objective",
                "budgets_json": "X".repeat(12_000),
                "extra_big_future_field": "Y".repeat(12_000),
                "created_at_ms": 1,
                "updated_at_ms": 2,
                "status": "planned"
            }
        });

        let limit = 200;
        let mut truncated = false;
        let mut minimal = false;
        let _used = ensure_budget_limit(&mut v, limit, &mut truncated, &mut minimal, |val| {
            trim_compact_open_result_for_budget(val, limit)
        });

        assert!(!minimal, "must not fall back to signal=minimal: {v}");
        assert!(truncated, "must be marked truncated under tight limit");

        assert_eq!(v.get("id").and_then(|v| v.as_str()), Some("SLC-00000001"));
        assert_eq!(v.get("kind").and_then(|v| v.as_str()), Some("slice"));
        assert_eq!(v.get("workspace").and_then(|v| v.as_str()), Some("ws1"));

        let slice = v.get("slice").expect("slice must survive");
        assert_eq!(
            slice.get("plan_id").and_then(|v| v.as_str()),
            Some("PLAN-1"),
            "slice.plan_id must survive for binding"
        );
        assert_eq!(
            slice.get("slice_task_id").and_then(|v| v.as_str()),
            Some("TASK-1"),
            "slice.slice_task_id must survive for binding"
        );

        assert!(
            slice.get("budgets_json").is_none(),
            "fat slice.budgets_json must be dropped"
        );
        assert!(
            slice.get("extra_big_future_field").is_none(),
            "unknown fat slice fields must be dropped via minimal-slice shrink"
        );
    }
}
