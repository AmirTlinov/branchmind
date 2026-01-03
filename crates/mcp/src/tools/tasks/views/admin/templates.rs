#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_templates_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let templates = built_in_task_templates()
            .into_iter()
            .map(|template| {
                let mut out = json!({
                    "id": template.id,
                    "kind": template.kind.as_str(),
                    "title": template.title,
                    "description": template.description
                });
                if template.kind == TaskKind::Plan {
                    if let Some(obj) = out.as_object_mut() {
                        obj.insert("plan_steps".to_string(), json!(template.plan_steps));
                    }
                } else if let Some(obj) = out.as_object_mut() {
                    obj.insert(
                        "steps".to_string(),
                        Value::Array(
                            template
                                .task_steps
                                .into_iter()
                                .map(|step| {
                                    json!({
                                        "title": step.title,
                                        "success_criteria": step.success_criteria,
                                        "tests": step.tests,
                                        "blockers": step.blockers
                                    })
                                })
                                .collect(),
                        ),
                    );
                }
                out
            })
            .collect::<Vec<_>>();

        ai_ok(
            "templates_list",
            json!({
                "workspace": workspace.as_str(),
                "templates": templates
            }),
        )
    }
}
