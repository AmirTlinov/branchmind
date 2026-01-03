#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_scaffold(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let template_id = match require_string(args_obj, "template") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let title = match require_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let description = match optional_string(args_obj, "description") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let parent = args_obj
            .get("parent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let plan_title = match optional_string(args_obj, "plan_title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if parent.is_some() && plan_title.is_some() {
            return ai_error("INVALID_INPUT", "provide parent or plan_title, not both");
        }

        let kind_override = args_obj.get("kind").and_then(|v| v.as_str());
        let inferred_kind = match kind_override {
            Some(kind) => parse_kind(Some(kind), parent.is_some() || plan_title.is_some()),
            None => find_task_template_any(&template_id)
                .map(|t| t.kind)
                .unwrap_or_else(|| parse_kind(None, parent.is_some() || plan_title.is_some())),
        };

        let template = match find_task_template(&template_id, inferred_kind) {
            Some(v) => v,
            None => {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown template id",
                    Some(
                        "Use built-in template ids: basic-task, principal-task, basic-plan, principal-plan. (Use tasks_templates_list.)",
                    ),
                    vec![suggest_call(
                        "tasks_templates_list",
                        "List available templates.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                );
            }
        };

        match inferred_kind {
            TaskKind::Plan => {
                if template.plan_steps.is_empty() {
                    return ai_error("INVALID_INPUT", "template has no plan steps");
                }
                let payload = json!({ "kind": "plan", "title": title }).to_string();
                let (plan_id, revision, event) = match self.store.create(
                    &workspace,
                    bm_storage::TaskCreateRequest {
                        kind: TaskKind::Plan,
                        title,
                        parent_plan_id: None,
                        description,
                        contract: None,
                        contract_json: None,
                        event_type: "plan_created".to_string(),
                        event_payload_json: payload,
                    },
                ) {
                    Ok(v) => v,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let create_event_json = events_to_json(vec![event]).remove(0);
                let plan_steps = template.plan_steps.clone();
                let (checklist_revision, checklist, checklist_event) =
                    match self.store.plan_checklist_update(
                        &workspace,
                        bm_storage::PlanChecklistUpdateRequest {
                            plan_id: plan_id.clone(),
                            expected_revision: Some(revision),
                            steps: Some(plan_steps.clone()),
                            current: Some(0),
                            doc: None,
                            advance: false,
                            event_type: "plan_updated".to_string(),
                            event_payload_json: json!({ "steps": plan_steps, "current": 0 })
                                .to_string(),
                        },
                    ) {
                        Ok(v) => v,
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };

                let events = vec![
                    create_event_json.clone(),
                    events_to_json(vec![checklist_event]).remove(0),
                ];

                ai_ok(
                    "tasks_scaffold",
                    json!({
                        "workspace": workspace.as_str(),
                        "template": { "id": template.id, "kind": template.kind.as_str() },
                        "plan": { "id": plan_id, "revision": checklist_revision },
                        "checklist": {
                            "steps": checklist.steps,
                            "current": checklist.current
                        },
                        "events": events,
                        "event": {
                            "event_id": create_event_json["event_id"].clone(),
                            "ts": create_event_json["ts"].clone(),
                            "ts_ms": create_event_json["ts_ms"].clone(),
                            "task_id": create_event_json["task_id"].clone(),
                            "path": create_event_json["path"].clone(),
                            "type": create_event_json["type"].clone(),
                            "payload": create_event_json["payload"].clone()
                        }
                    }),
                )
            }
            TaskKind::Task => {
                if template.task_steps.is_empty() {
                    return ai_error("INVALID_INPUT", "template has no task steps");
                }
                let parent_plan_id = match (parent, plan_title) {
                    (Some(id), None) => {
                        if !id.starts_with("PLAN-") {
                            return ai_error("INVALID_INPUT", "parent must start with PLAN-");
                        }
                        match self.store.get_plan(&workspace, &id) {
                            Ok(Some(_)) => id,
                            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown plan id"),
                            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                        }
                    }
                    (None, Some(plan_title)) => {
                        let payload = json!({ "kind": "plan", "title": plan_title }).to_string();
                        let (plan_id, _revision, _event) = match self.store.create(
                            &workspace,
                            bm_storage::TaskCreateRequest {
                                kind: TaskKind::Plan,
                                title: plan_title,
                                parent_plan_id: None,
                                description: None,
                                contract: None,
                                contract_json: None,
                                event_type: "plan_created".to_string(),
                                event_payload_json: payload,
                            },
                        ) {
                            Ok(v) => v,
                            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                        };
                        plan_id
                    }
                    (None, None) => {
                        return ai_error(
                            "INVALID_INPUT",
                            "parent or plan_title is required for task scaffold",
                        );
                    }
                    (Some(_), Some(_)) => {
                        return ai_error("INVALID_INPUT", "provide parent or plan_title, not both");
                    }
                };

                let payload = json!({
                    "kind": "task",
                    "title": title,
                    "parent": parent_plan_id
                })
                .to_string();
                let (task_id, _revision, create_event) = match self.store.create(
                    &workspace,
                    bm_storage::TaskCreateRequest {
                        kind: TaskKind::Task,
                        title,
                        parent_plan_id: Some(parent_plan_id.clone()),
                        description,
                        contract: None,
                        contract_json: None,
                        event_type: "task_created".to_string(),
                        event_payload_json: payload,
                    },
                ) {
                    Ok(v) => v,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };

                let decompose_steps = template
                    .task_steps
                    .iter()
                    .map(|step| bm_storage::NewStep {
                        title: step.title.clone(),
                        success_criteria: step.success_criteria.clone(),
                    })
                    .collect::<Vec<_>>();
                let decompose = match self.store.steps_decompose(
                    &workspace,
                    &task_id,
                    None,
                    None,
                    decompose_steps,
                ) {
                    Ok(v) => v,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };

                let mut events = vec![
                    events_to_json(vec![create_event]).remove(0),
                    events_to_json(vec![decompose.event]).remove(0),
                ];

                let mut revision = decompose.task_revision;
                for (step, step_ref) in template.task_steps.iter().zip(decompose.steps.iter()) {
                    if step.tests.is_empty() && step.blockers.is_empty() {
                        continue;
                    }
                    let defined = match self.store.step_define(
                        &workspace,
                        bm_storage::StepDefineRequest {
                            task_id: task_id.clone(),
                            expected_revision: Some(revision),
                            selector: bm_storage::StepSelector {
                                step_id: Some(step_ref.step_id.clone()),
                                path: None,
                            },
                            patch: bm_storage::StepPatch {
                                title: None,
                                success_criteria: None,
                                tests: Some(step.tests.clone()),
                                blockers: Some(step.blockers.clone()),
                                proof_tests_mode: (step.proof_tests_mode
                                    != bm_storage::ProofMode::Off)
                                    .then_some(step.proof_tests_mode),
                                proof_security_mode: (step.proof_security_mode
                                    != bm_storage::ProofMode::Off)
                                    .then_some(step.proof_security_mode),
                                proof_perf_mode: (step.proof_perf_mode
                                    != bm_storage::ProofMode::Off)
                                    .then_some(step.proof_perf_mode),
                                proof_docs_mode: (step.proof_docs_mode
                                    != bm_storage::ProofMode::Off)
                                    .then_some(step.proof_docs_mode),
                            },
                        },
                    ) {
                        Ok(v) => v,
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    revision = defined.task_revision;
                    events.push(events_to_json(vec![defined.event]).remove(0));
                }

                let steps_out = decompose
                    .steps
                    .into_iter()
                    .map(|s| json!({ "step_id": s.step_id, "path": s.path }))
                    .collect::<Vec<_>>();

                ai_ok(
                    "tasks_scaffold",
                    json!({
                        "workspace": workspace.as_str(),
                        "template": { "id": template.id, "kind": template.kind.as_str() },
                        "plan": { "id": parent_plan_id },
                        "task": { "id": task_id, "revision": revision },
                        "steps": steps_out,
                        "events": events
                    }),
                )
            }
        }
    }
}
