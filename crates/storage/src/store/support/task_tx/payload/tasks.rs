#![forbid(unsafe_code)]

use bm_core::model::TaskKind;

pub(in crate::store) fn build_task_deleted_payload(task_id: &str, kind: TaskKind) -> String {
    format!("{{\"task\":\"{task_id}\",\"kind\":\"{}\"}}", kind.as_str())
}
