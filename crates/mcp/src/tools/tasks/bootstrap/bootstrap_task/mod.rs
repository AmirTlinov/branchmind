#![forbid(unsafe_code)]
//! tasks_bootstrap (split-friendly orchestrator).

mod args;
mod plan;
mod render;
mod task;
mod think;

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_bootstrap(&mut self, args: Value) -> Value {
        let args = match args::parse_tasks_bootstrap_args(args) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let args::TasksBootstrapArgs {
            workspace,
            agent_id,
            plan_id,
            parent_id,
            plan_title,
            plan_template,
            task_title,
            description,
            steps,
            think,
            reasoning_mode,
        } = args;

        let mut events = Vec::new();

        let plan::ResolvedPlan {
            id: parent_plan_id,
            created: plan_created,
        } = match plan::resolve_or_create_parent_plan(
            self,
            &workspace,
            plan_id,
            parent_id,
            plan_title,
            plan_template,
            &mut events,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let task::CreatedTask {
            id: task_id,
            revision,
            steps: created_steps,
        } = match task::create_task_with_steps(task::CreateTaskWithStepsArgs {
            server: self,
            workspace: &workspace,
            parent_plan_id: &parent_plan_id,
            task_title,
            description,
            steps,
            agent_id: agent_id.clone(),
            events: &mut events,
        }) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut warnings = Vec::new();
        let mut revision = revision;

        if let Some(mode) = reasoning_mode
            && mode != "normal"
        {
            let payload = json!({
                "kind": "task",
                "patch": { "reasoning_mode": mode.clone() }
            })
            .to_string();
            match self.store.edit_task(
                &workspace,
                bm_storage::TaskEditRequest {
                    id: task_id.clone(),
                    expected_revision: None,
                    title: None,
                    description: None,
                    context: None,
                    priority: None,
                    domain: None,
                    reasoning_mode: Some(mode),
                    phase: None,
                    component: None,
                    assignee: None,
                    tags: None,
                    depends_on: None,
                    event_type: "task_edited".to_string(),
                    event_payload_json: payload,
                },
            ) {
                Ok((new_revision, event)) => {
                    revision = new_revision;
                    events.push(events_to_json(vec![event]).remove(0));
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        }

        let think_pipeline = match think::maybe_run_think_pipeline(
            self,
            &workspace,
            &task_id,
            agent_id.as_deref(),
            think,
            &mut warnings,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = render::render_tasks_bootstrap_result(render::TasksBootstrapRenderArgs {
            workspace: &workspace,
            parent_plan_id,
            plan_created,
            task_id,
            revision,
            steps: created_steps,
            events,
            think_pipeline,
        });

        if warnings.is_empty() {
            ai_ok("tasks_bootstrap", result)
        } else {
            ai_ok_with_warnings("tasks_bootstrap", result, warnings, Vec::new())
        }
    }
}
