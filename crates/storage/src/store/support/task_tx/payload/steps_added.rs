#![forbid(unsafe_code)]

use super::super::super::super::StepRef;

pub(in crate::store) fn build_steps_added_payload(
    task_id: &str,
    parent_path: Option<&str>,
    steps: &[StepRef],
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"parent_path\":");
    match parent_path {
        None => out.push_str("null"),
        Some(path) => {
            out.push('"');
            out.push_str(path);
            out.push('"');
        }
    }
    out.push_str(",\"steps\":[");
    for (i, step) in steps.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push_str("{\"step_id\":\"");
        out.push_str(&step.step_id);
        out.push_str("\",\"path\":\"");
        out.push_str(&step.path);
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}
