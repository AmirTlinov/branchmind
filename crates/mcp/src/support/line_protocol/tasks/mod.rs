#![forbid(unsafe_code)]

mod jobs;
mod macros;
mod resume;
mod snapshot;

use crate::Toolset;
use serde_json::Value;

pub(super) fn render_tasks_macro_start_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    macros::render_tasks_macro_start_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_macro_delegate_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    macros::render_tasks_macro_delegate_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_macro_close_step_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    macros::render_tasks_macro_close_step_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_snapshot_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    snapshot::render_tasks_snapshot_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_jobs_list_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    jobs::render_tasks_jobs_list_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_jobs_radar_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    jobs::render_tasks_jobs_radar_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_jobs_open_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    jobs::render_tasks_jobs_open_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_jobs_tail_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    jobs::render_tasks_jobs_tail_lines(args, response, toolset, omit_workspace)
}

pub(super) fn render_tasks_jobs_message_lines(
    args: &Value,
    response: &Value,
    toolset: Toolset,
    omit_workspace: bool,
) -> String {
    jobs::render_tasks_jobs_message_lines(args, response, toolset, omit_workspace)
}
