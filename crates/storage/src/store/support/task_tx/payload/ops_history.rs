#![forbid(unsafe_code)]

pub(in crate::store) fn build_undo_redo_payload(
    op_seq: i64,
    intent: &str,
    task_id: Option<&str>,
    path: Option<&str>,
    undo: bool,
) -> String {
    let mut out = String::new();
    out.push_str("{\"op_seq\":");
    out.push_str(&op_seq.to_string());
    out.push_str(",\"intent\":\"");
    out.push_str(intent);
    out.push_str("\",\"undo\":");
    out.push_str(if undo { "true" } else { "false" });
    if let Some(task_id) = task_id {
        out.push_str(",\"task\":\"");
        out.push_str(task_id);
        out.push('"');
    }
    if let Some(path) = path {
        out.push_str(",\"path\":\"");
        out.push_str(path);
        out.push('"');
    }
    out.push('}');
    out
}
