#![forbid(unsafe_code)]

use super::super::*;
use super::resume::render_tasks_resume_lines;

pub(super) fn render_tasks_snapshot_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    // tasks_snapshot returns the resume_super payload directly.
    // Navigation guarantee (BM-L1): a stable `ref=` handle is embedded in the state line.
    render_tasks_resume_lines(
        toolset,
        "tasks_snapshot",
        args,
        response,
        &["result"],
        omit_workspace,
    )
}
