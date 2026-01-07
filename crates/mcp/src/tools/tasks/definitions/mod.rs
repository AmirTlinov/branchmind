#![forbid(unsafe_code)]

use serde_json::Value;

mod batch;
mod bootstrap;
mod create;
mod history;
mod steps_control;
mod steps_leases;
mod steps_lifecycle;
mod steps_patch;
mod steps_task_ops;
mod views;

pub(crate) fn task_tool_definitions() -> Vec<Value> {
    let mut out = Vec::new();
    out.extend(create::create_definitions());
    out.extend(bootstrap::bootstrap_definitions());
    out.extend(steps_lifecycle::steps_lifecycle_definitions());
    out.extend(steps_control::steps_control_definitions());
    out.extend(steps_leases::steps_leases_definitions());
    out.extend(steps_task_ops::steps_task_ops_definitions());
    out.extend(steps_patch::steps_patch_definitions());
    out.extend(history::history_definitions());
    out.extend(batch::batch_definitions());
    out.extend(views::views_definitions());
    out
}
