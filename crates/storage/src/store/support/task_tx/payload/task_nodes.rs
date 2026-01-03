#![forbid(unsafe_code)]

pub(in crate::store) fn build_task_node_added_payload(
    task_id: &str,
    node_id: &str,
    path: &str,
    parent_path: &str,
) -> String {
    format!(
        "{{\"task\":\"{task_id}\",\"node_id\":\"{node_id}\",\"path\":\"{path}\",\"parent_path\":\"{parent_path}\"}}"
    )
}

pub(in crate::store) fn build_task_node_defined_payload(
    task_id: &str,
    node_id: &str,
    path: &str,
    fields: &[&str],
) -> String {
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"node_id\":\"");
    out.push_str(node_id);
    out.push_str("\",\"path\":\"");
    out.push_str(path);
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

pub(in crate::store) fn build_task_node_deleted_payload(
    task_id: &str,
    node_id: &str,
    path: &str,
) -> String {
    format!("{{\"task\":\"{task_id}\",\"node_id\":\"{node_id}\",\"path\":\"{path}\"}}")
}
