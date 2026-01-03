#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_plan(&mut self, args: Value) -> Value {
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
        let steps = match optional_string_array(args_obj, "steps") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let current = match optional_i64(args_obj, "current") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let doc = match optional_string(args_obj, "doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let advance = match optional_bool(args_obj, "advance") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let payload = json!({
            "steps": steps,
            "current": current,
            "doc": doc,
            "advance": advance
        });

        let result = self.store.plan_checklist_update(
            &workspace,
            bm_storage::PlanChecklistUpdateRequest {
                plan_id: plan_id.clone(),
                expected_revision,
                steps,
                current,
                doc,
                advance,
                event_type: "plan_updated".to_string(),
                event_payload_json: payload.to_string(),
            },
        );

        match result {
            Ok((revision, checklist, _event)) => {
                let plan = match self.store.get_plan(&workspace, &plan_id) {
                    Ok(Some(p)) => p,
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown plan id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let progress = format!("{}/{}", checklist.current, checklist.steps.len());
                ai_ok(
                    "plan",
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
                            "docs_confirmed": plan.docs_confirmed,
                            "plan_progress": progress
                        }
                    }),
                )
            }
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
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown plan id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
