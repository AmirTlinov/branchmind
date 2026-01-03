#![forbid(unsafe_code)]

use super::super::super::super::StepRef;

pub(in crate::store) fn build_step_deleted_payload(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        step.step_id, step.path
    )
}

pub(in crate::store) fn build_step_defined_payload(
    task_id: &str,
    step: &StepRef,
    fields: &[&str],
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"step_id\":\"");
    out.push_str(&step.step_id);
    out.push_str("\",\"path\":\"");
    out.push_str(&step.path);
    out.push_str("\",\"fields\":[");
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(field);
        out.push('"');
    }
    out.push_str("]}");
    out
}

pub(in crate::store) fn build_step_noted_payload(
    task_id: &str,
    step: &StepRef,
    note_seq: i64,
) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"note_seq\":{note_seq}}}",
        step.step_id, step.path
    )
}

pub(in crate::store) fn build_step_noted_mirror_meta_json(
    task_id: &str,
    step: &StepRef,
    note_seq: i64,
    event_id: &str,
) -> String {
    format!(
        "{{\"source\":\"tasks_note\",\"task_id\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"note_seq\":{note_seq},\"event_id\":\"{event_id}\"}}",
        step.step_id, step.path
    )
}

pub(in crate::store) fn build_step_verified_payload(
    task_id: &str,
    step: &StepRef,
    criteria_confirmed: Option<bool>,
    tests_confirmed: Option<bool>,
    security_confirmed: Option<bool>,
    perf_confirmed: Option<bool>,
    docs_confirmed: Option<bool>,
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"step_id\":\"");
    out.push_str(&step.step_id);
    out.push_str("\",\"path\":\"");
    out.push_str(&step.path);
    out.push('"');
    if let Some(v) = criteria_confirmed {
        out.push_str(",\"criteria_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = tests_confirmed {
        out.push_str(",\"tests_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = security_confirmed {
        out.push_str(",\"security_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = perf_confirmed {
        out.push_str(",\"perf_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    if let Some(v) = docs_confirmed {
        out.push_str(",\"docs_confirmed\":");
        out.push_str(if v { "true" } else { "false" });
    }
    out.push('}');
    out
}

pub(in crate::store) fn build_step_done_payload(task_id: &str, step: &StepRef) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\"}}",
        step.step_id, step.path
    )
}

pub(in crate::store) fn build_step_reopened_payload(
    task_id: &str,
    step: &StepRef,
    force: bool,
) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"force\":{}}}",
        step.step_id,
        step.path,
        if force { "true" } else { "false" }
    )
}

pub(in crate::store) fn build_step_block_payload(
    task_id: &str,
    step: &StepRef,
    blocked: bool,
    reason: Option<&str>,
) -> String {
    let mut out = format!(
        "{{\"task\":\"{task_id}\",\"step_id\":\"{}\",\"path\":\"{}\",\"blocked\":{}}}",
        step.step_id,
        step.path,
        if blocked { "true" } else { "false" }
    );
    if let Some(reason) = reason {
        out.pop();
        out.push_str(",\"reason\":\"");
        out.push_str(reason);
        out.push_str("\"}");
    }
    out
}
