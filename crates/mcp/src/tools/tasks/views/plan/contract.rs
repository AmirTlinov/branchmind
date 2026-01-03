#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_contract(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (plan_id, kind, _focus) = match resolve_target_id(&mut self.store, &workspace, args_obj)
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !matches!(kind, TaskKind::Plan) {
            return ai_error("INVALID_INPUT", "plan is required");
        }
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let clear = args_obj
            .get("clear")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let contract = match optional_string(args_obj, "current") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let contract_json = match optional_object_as_json_string(args_obj, "contract_data") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let contract_for_payload = contract.clone();
        let contract_json_for_payload = contract_json.clone();

        let next_contract = if clear { Some(None) } else { Some(contract) };
        let next_contract_json = if clear {
            Some(None)
        } else {
            Some(contract_json)
        };

        let payload = json!({
            "clear": clear,
            "contract": contract_for_payload,
            "contract_data": parse_json_or_null(contract_json_for_payload)
        });

        let result = self.store.edit_plan(
            &workspace,
            bm_storage::PlanEditRequest {
                id: plan_id.clone(),
                expected_revision,
                title: None,
                description: None,
                context: None,
                priority: None,
                tags: None,
                depends_on: None,
                contract: next_contract,
                contract_json: next_contract_json,
                event_type: "contract_updated".to_string(),
                event_payload_json: payload.to_string(),
            },
        );

        match result {
            Ok((revision, _event)) => {
                let plan = match self.store.get_plan(&workspace, &plan_id) {
                    Ok(Some(p)) => p,
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown plan id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                ai_ok(
                    "contract",
                    json!( {
                        "plan": {
                            "id": plan.id,
                            "kind": "plan",
                            "title": plan.title,
                            "revision": revision,
                            "contract_versions_count": 0,
                            "criteria_confirmed": plan.criteria_confirmed,
                            "tests_confirmed": plan.tests_confirmed,
                            "criteria_auto_confirmed": plan.criteria_auto_confirmed,
                            "tests_auto_confirmed": plan.tests_auto_confirmed,
                            "security_confirmed": plan.security_confirmed,
                            "perf_confirmed": plan.perf_confirmed,
                            "docs_confirmed": plan.docs_confirmed
                        }
                    }),
                )
            }
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown plan id"),
            Err(StoreError::RevisionMismatch { expected, actual }) => ai_error_with(
                "REVISION_MISMATCH",
                &format!("expected={expected} actual={actual}"),
                Some("Refresh the current revision and retry with expected_revision."),
                vec![suggest_call(
                    "tasks_context",
                    "Refresh current revisions for this workspace.",
                    "high",
                    json!({ "workspace": workspace.as_str() }),
                )],
            ),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
