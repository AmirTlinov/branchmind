#![forbid(unsafe_code)]

use super::super::*;
use super::resume::render_tasks_resume_lines;

pub(super) fn render_tasks_macro_start_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    render_tasks_resume_lines(
        toolset,
        "tasks_macro_start",
        args,
        response,
        &["result", "resume"],
        omit_workspace,
    )
}

pub(super) fn render_tasks_macro_delegate_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    render_tasks_resume_lines(
        toolset,
        "tasks_macro_delegate",
        args,
        response,
        &["result", "resume"],
        omit_workspace,
    )
}

pub(super) fn render_tasks_macro_close_step_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    render_tasks_resume_lines(
        toolset,
        "tasks_macro_close_step",
        args,
        response,
        &["result", "resume"],
        omit_workspace,
    )
}
