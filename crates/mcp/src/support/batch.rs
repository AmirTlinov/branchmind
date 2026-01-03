#![forbid(unsafe_code)]

use serde_json::Value;

pub(crate) fn batch_tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "tasks_create"
            | "tasks_decompose"
            | "tasks_define"
            | "tasks_note"
            | "tasks_verify"
            | "tasks_done"
            | "tasks_close_step"
            | "tasks_block"
            | "tasks_progress"
            | "tasks_edit"
            | "tasks_patch"
            | "tasks_delete"
            | "tasks_task_add"
            | "tasks_task_define"
            | "tasks_task_delete"
            | "tasks_evidence_capture"
            | "tasks_plan"
            | "tasks_contract"
            | "tasks_complete"
    )
}

pub(crate) fn batch_tool_undoable(name: &str) -> bool {
    matches!(
        name,
        "tasks_patch" | "tasks_task_define" | "tasks_progress" | "tasks_block"
    )
}

pub(crate) fn batch_target_id(args: &serde_json::Map<String, Value>) -> Option<String> {
    args.get("task")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .or_else(|| {
            args.get("plan")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string())
        })
}
