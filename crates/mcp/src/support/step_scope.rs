#![forbid(unsafe_code)]

use serde_json::Value;

pub(crate) fn step_path_matches(focus: &str, path: &str) -> bool {
    if path == focus {
        return true;
    }
    if !path.starts_with(focus) {
        return false;
    }
    // Segment-safe prefix: "s:0" matches "s:0.s:1" but not "s:01".
    matches!(path.as_bytes().get(focus.len()), Some(b'.'))
}

pub(crate) fn step_meta_matches(meta: &Value, focus_task_id: &str, focus_step_path: &str) -> bool {
    let Some(step) = meta.get("step").and_then(|v| v.as_object()) else {
        return false;
    };

    if step.get("task_id").and_then(|v| v.as_str()) != Some(focus_task_id) {
        return false;
    }

    let Some(path) = step.get("path").and_then(|v| v.as_str()) else {
        return false;
    };

    step_path_matches(focus_step_path, path)
}
