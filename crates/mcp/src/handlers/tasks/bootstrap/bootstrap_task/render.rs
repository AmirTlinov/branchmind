#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct TasksBootstrapRenderArgs<'a> {
    pub workspace: &'a WorkspaceId,
    pub parent_plan_id: String,
    pub plan_created: bool,
    pub task_id: String,
    pub revision: i64,
    pub steps: Vec<bm_storage::StepRef>,
    pub events: Vec<Value>,
    pub think_pipeline: Option<Value>,
}

pub(super) fn render_tasks_bootstrap_result(args: TasksBootstrapRenderArgs<'_>) -> Value {
    let TasksBootstrapRenderArgs {
        workspace,
        parent_plan_id,
        plan_created,
        task_id,
        revision,
        steps,
        events,
        think_pipeline,
    } = args;

    let mut result = json!({
        "workspace": workspace.as_str(),
        "plan": {
            "id": parent_plan_id,
            "qualified_id": format!("{}:{}", workspace.as_str(), parent_plan_id),
            "created": plan_created
        },
        "task": {
            "id": task_id,
            "qualified_id": format!("{}:{}", workspace.as_str(), task_id),
            "revision": revision
        },
        "steps": steps.iter().map(|s| json!({
            "step_id": s.step_id,
            "path": s.path
        })).collect::<Vec<_>>(),
        "events": events
    });
    if let Some(think_pipeline) = think_pipeline
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("think_pipeline".to_string(), think_pipeline);
    }

    result
}
