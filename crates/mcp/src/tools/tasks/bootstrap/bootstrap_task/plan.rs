#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct ResolvedPlan {
    pub id: String,
    pub created: bool,
}

pub(super) fn resolve_or_create_parent_plan(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    plan_id: Option<String>,
    parent_id: Option<String>,
    plan_title: Option<String>,
    plan_template: Option<String>,
    events: &mut Vec<Value>,
) -> Result<ResolvedPlan, Value> {
    match (plan_id.or(parent_id), plan_title) {
        (Some(id), None) => {
            if !id.starts_with("PLAN-") {
                return Err(ai_error("INVALID_INPUT", "plan must start with PLAN-"));
            }
            match server.store.get_plan(workspace, &id) {
                Ok(Some(_)) => Ok(ResolvedPlan { id, created: false }),
                Ok(None) => Err(ai_error("UNKNOWN_ID", "Unknown plan id")),
                Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
            }
        }
        (None, Some(title)) => {
            let payload = json!({
                "kind": "plan",
                "title": title
            })
            .to_string();
            let (id, revision, event) = match server.store.create(
                workspace,
                bm_storage::TaskCreateRequest {
                    kind: TaskKind::Plan,
                    title,
                    parent_plan_id: None,
                    description: None,
                    contract: None,
                    contract_json: None,
                    event_type: "plan_created".to_string(),
                    event_payload_json: payload,
                },
            ) {
                Ok(v) => v,
                Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
            };

            events.push(events_to_json(vec![event]).remove(0));

            if let Some(template_id) = plan_template {
                let template = match find_task_template(&template_id, TaskKind::Plan) {
                    Some(v) => v,
                    None => {
                        return Err(ai_error_with(
                            "UNKNOWN_ID",
                            "Unknown plan_template id",
                            Some(
                                "Use built-in plan templates: basic-plan or principal-plan. (Use tasks_templates_list.)",
                            ),
                            vec![suggest_call(
                                "tasks_templates_list",
                                "List available templates.",
                                "high",
                                json!({ "workspace": workspace.as_str() }),
                            )],
                        ));
                    }
                };
                if template.plan_steps.is_empty() {
                    return Err(ai_error("INVALID_INPUT", "plan_template has no plan steps"));
                }

                let plan_steps = template.plan_steps.clone();
                let (_checklist_revision, _checklist, checklist_event) =
                    match server.store.plan_checklist_update(
                        workspace,
                        bm_storage::PlanChecklistUpdateRequest {
                            plan_id: id.clone(),
                            expected_revision: Some(revision),
                            steps: Some(plan_steps.clone()),
                            current: Some(0),
                            doc: None,
                            advance: false,
                            event_type: "plan_updated".to_string(),
                            event_payload_json: json!({
                                "steps": plan_steps,
                                "current": 0,
                                "template": template_id
                            })
                            .to_string(),
                        },
                    ) {
                        Ok(v) => v,
                        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
                    };
                events.push(events_to_json(vec![checklist_event]).remove(0));
            }
            Ok(ResolvedPlan { id, created: true })
        }
        (None, None) => Err(ai_error(
            "INVALID_INPUT",
            "plan or plan_title is required to bootstrap a task",
        )),
        (Some(_), Some(_)) => Err(ai_error(
            "INVALID_INPUT",
            "provide plan or plan_title, not both",
        )),
    }
}
