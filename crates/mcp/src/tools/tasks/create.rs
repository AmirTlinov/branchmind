#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_create(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let title = match require_string(args_obj, "title") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let parent = args_obj
            .get("parent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let kind = parse_kind(
            args_obj.get("kind").and_then(|v| v.as_str()),
            parent.is_some(),
        );

        let description = args_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let contract = args_obj
            .get("contract")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let contract_json = args_obj.get("contract_data").map(|v| v.to_string());

        let steps_value = args_obj.get("steps").cloned().unwrap_or(Value::Null);
        let mut steps = Vec::new();
        if !steps_value.is_null() {
            let Some(steps_array) = steps_value.as_array() else {
                return ai_error("INVALID_INPUT", "steps must be an array");
            };
            if steps_array.is_empty() {
                return ai_error("INVALID_INPUT", "steps must not be empty");
            }
            if kind != TaskKind::Task {
                return ai_error("INVALID_INPUT", "steps are only supported for task create");
            }
            for step in steps_array {
                let Some(obj) = step.as_object() else {
                    return ai_error("INVALID_INPUT", "steps[] items must be objects");
                };
                let title = match require_string(obj, "title") {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                let criteria_value = obj.get("success_criteria").cloned().unwrap_or(Value::Null);
                let Some(criteria_array) = criteria_value.as_array() else {
                    return ai_error("INVALID_INPUT", "steps[].success_criteria must be an array");
                };
                if criteria_array.is_empty() {
                    return ai_error(
                        "INVALID_INPUT",
                        "steps[].success_criteria must not be empty",
                    );
                }
                let mut success_criteria = Vec::with_capacity(criteria_array.len());
                for item in criteria_array {
                    let Some(s) = item.as_str() else {
                        return ai_error(
                            "INVALID_INPUT",
                            "steps[].success_criteria items must be strings",
                        );
                    };
                    success_criteria.push(s.to_string());
                }
                let success_criteria = match normalize_required_string_list(
                    success_criteria,
                    "steps[].success_criteria",
                ) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                let tests = match optional_string_array(obj, "tests") {
                    Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
                    Err(resp) => return resp,
                };
                let blockers = match optional_string_array(obj, "blockers") {
                    Ok(v) => normalize_optional_string_list(v).unwrap_or_default(),
                    Err(resp) => return resp,
                };
                steps.push((title, success_criteria, tests, blockers));
            }
        }

        let event_type = match kind {
            TaskKind::Plan => "plan_created",
            TaskKind::Task => "task_created",
        }
        .to_string();

        let event_payload_json = json!({
            "kind": kind.as_str(),
            "title": title.clone(),
            "parent": parent.clone(),
        })
        .to_string();

        let created = self.store.create(
            &workspace,
            bm_storage::TaskCreateRequest {
                kind,
                title,
                parent_plan_id: parent.clone(),
                description,
                contract,
                contract_json,
                event_type: event_type.clone(),
                event_payload_json,
            },
        );

        match created {
            Ok((id, revision, event)) => {
                if steps.is_empty() {
                    return ai_ok(
                        "create",
                        json!( {
                            "id": id,
                            "kind": kind.as_str(),
                            "revision": revision,
                            "event": {
                                "event_id": event.event_id(),
                                "ts": ts_ms_to_rfc3339(event.ts_ms),
                                "ts_ms": event.ts_ms,
                                "task_id": event.task_id,
                                "path": event.path,
                                "type": event.event_type,
                                "payload": parse_json_or_string(&event.payload_json)
                            }
                        }),
                    );
                }

                let mut events = vec![events_to_json(vec![event]).remove(0)];
                let decompose_steps = steps
                    .iter()
                    .map(
                        |(title, success_criteria, _tests, _blockers)| bm_storage::NewStep {
                            title: title.clone(),
                            success_criteria: success_criteria.clone(),
                        },
                    )
                    .collect::<Vec<_>>();
                let decompose = match self.store.steps_decompose(
                    &workspace,
                    &id,
                    Some(revision),
                    None,
                    decompose_steps,
                ) {
                    Ok(v) => v,
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                events.push(events_to_json(vec![decompose.event]).remove(0));

                let mut current_revision = decompose.task_revision;
                for ((_, _criteria, tests, blockers), step_ref) in
                    steps.iter().zip(decompose.steps.iter())
                {
                    if tests.is_empty() && blockers.is_empty() {
                        continue;
                    }
                    let defined = match self.store.step_define(
                        &workspace,
                        bm_storage::StepDefineRequest {
                            task_id: id.clone(),
                            expected_revision: Some(current_revision),
                            agent_id: agent_id.clone(),
                            selector: bm_storage::StepSelector {
                                step_id: Some(step_ref.step_id.clone()),
                                path: None,
                            },
                            patch: bm_storage::StepPatch {
                                title: None,
                                success_criteria: None,
                                tests: Some(tests.clone()),
                                blockers: Some(blockers.clone()),
                                proof_tests_mode: None,
                                proof_security_mode: None,
                                proof_perf_mode: None,
                                proof_docs_mode: None,
                            },
                        },
                    ) {
                        Ok(v) => v,
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };
                    current_revision = defined.task_revision;
                    events.push(events_to_json(vec![defined.event]).remove(0));
                }

                let steps_out = decompose
                    .steps
                    .into_iter()
                    .map(|s| json!({ "step_id": s.step_id, "path": s.path }))
                    .collect::<Vec<_>>();

                ai_ok(
                    "create",
                    json!( {
                        "id": id,
                        "qualified_id": format!("{}:{}", workspace.as_str(), id),
                        "kind": kind.as_str(),
                        "revision": current_revision,
                        "event": {
                            "event_id": events[0]["event_id"].clone(),
                            "ts": events[0]["ts"].clone(),
                            "ts_ms": events[0]["ts_ms"].clone(),
                            "task_id": events[0]["task_id"].clone(),
                            "path": events[0]["path"].clone(),
                            "type": events[0]["type"].clone(),
                            "payload": events[0]["payload"].clone()
                        },
                        "steps": steps_out,
                        "events": events
                    }),
                )
            }
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
