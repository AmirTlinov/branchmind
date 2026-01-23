#![forbid(unsafe_code)]

pub(super) fn is_portal_tool(name: &str) -> bool {
    matches!(
        name,
        "status"
            | "macro_branch_note"
            | "anchors_list"
            | "anchor_snapshot"
            | "macro_anchor_note"
            | "anchors_export"
            | "workspace_use"
            | "workspace_reset"
            | "tasks_macro_start"
            | "tasks_macro_delegate"
            | "tasks_macro_close_step"
            | "tasks_snapshot"
    )
}
