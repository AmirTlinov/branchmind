#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) fn lease_error_suggestions(
    workspace: &bm_core::ids::WorkspaceId,
    task_id: &str,
    step_id: Option<&str>,
    path: Option<&bm_core::paths::StepPath>,
    agent_id: Option<&str>,
) -> Vec<Value> {
    let mut selector = serde_json::Map::new();
    selector.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );
    selector.insert("task".to_string(), Value::String(task_id.to_string()));
    if let Some(step_id) = step_id {
        selector.insert("step_id".to_string(), Value::String(step_id.to_string()));
    }
    if let Some(path) = path {
        selector.insert("path".to_string(), Value::String(path.to_string()));
    }

    let mut get = selector.clone();
    if let Some(agent_id) = agent_id {
        get.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
    }

    let mut claim_force = selector;
    if let Some(agent_id) = agent_id {
        claim_force.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
    }
    claim_force.insert("force".to_string(), Value::Bool(true));

    vec![
        suggest_call(
            "tasks_step_lease_get",
            "Inspect lease holder and expiry for this step.",
            "high",
            Value::Object(get),
        ),
        suggest_call(
            "tasks_step_lease_claim",
            "Take over the lease explicitly (force=true).",
            "medium",
            Value::Object(claim_force),
        ),
    ]
}

impl McpServer {
    pub(crate) fn tool_tasks_step_lease_get(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, _kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (step_id, path) = match super::lifecycle::require_step_selector(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let result = self.store.step_lease_get(
            &workspace,
            bm_storage::StepLeaseGetRequest {
                task_id: task_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
            },
        );

        match result {
            Ok(out) => ai_ok(
                "step_lease_get",
                json!({
                    "task": task_id,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "lease": out.lease.map(|l| json!({
                        "holder_agent_id": l.holder_agent_id,
                        "acquired_seq": l.acquired_seq,
                        "expires_seq": l.expires_seq
                    })).unwrap_or(Value::Null),
                    "now_seq": out.now_seq
                }),
            ),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_step_lease_claim(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, _kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (step_id, path) = match super::lifecycle::require_step_selector(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(agent_id) = agent_id else {
            return ai_error("INVALID_INPUT", "agent_id is required");
        };

        let ttl_seq = match optional_i64(args_obj, "ttl_seq") {
            Ok(v) => v.unwrap_or(0),
            Err(resp) => return resp,
        };
        let force = match optional_bool(args_obj, "force") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let result = self.store.step_lease_claim(
            &workspace,
            bm_storage::StepLeaseClaimRequest {
                task_id: task_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                agent_id: agent_id.clone(),
                ttl_seq,
                force,
            },
        );

        match result {
            Ok(out) => ai_ok(
                "step_lease_claim",
                json!({
                    "task": task_id,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "lease": out.lease.map(|l| json!({
                        "holder_agent_id": l.holder_agent_id,
                        "acquired_seq": l.acquired_seq,
                        "expires_seq": l.expires_seq
                    })).unwrap_or(Value::Null),
                    "event": out.event.map(|e| json!({
                        "event_id": e.event_id(),
                        "ts": ts_ms_to_rfc3339(e.ts_ms),
                        "ts_ms": e.ts_ms,
                        "task_id": e.task_id,
                        "path": e.path,
                        "type": e.event_type,
                        "payload": parse_json_or_string(&e.payload_json)
                    })).unwrap_or(Value::Null),
                    "now_seq": out.now_seq
                }),
            ),
            Err(StoreError::StepLeaseHeld {
                step_id: leased_step_id,
                holder_agent_id,
                now_seq,
                expires_seq,
            }) => ai_error_with(
                "STEP_LEASE_HELD",
                &format!(
                    "step is leased by {holder_agent_id} (step_id={leased_step_id}, now_seq={now_seq}, expires_seq={expires_seq})"
                ),
                Some(
                    "Ask the holder to release the lease, wait for expiry, or take over explicitly (force=true).",
                ),
                lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id.as_deref(),
                    path.as_ref(),
                    Some(&agent_id),
                ),
            ),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_step_lease_renew(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, _kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (step_id, path) = match super::lifecycle::require_step_selector(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(agent_id) = agent_id else {
            return ai_error("INVALID_INPUT", "agent_id is required");
        };

        let ttl_seq = match optional_i64(args_obj, "ttl_seq") {
            Ok(v) => v.unwrap_or(0),
            Err(resp) => return resp,
        };

        let result = self.store.step_lease_renew(
            &workspace,
            bm_storage::StepLeaseRenewRequest {
                task_id: task_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                agent_id: agent_id.clone(),
                ttl_seq,
            },
        );

        match result {
            Ok(out) => ai_ok(
                "step_lease_renew",
                json!({
                    "task": task_id,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "lease": out.lease.map(|l| json!({
                        "holder_agent_id": l.holder_agent_id,
                        "acquired_seq": l.acquired_seq,
                        "expires_seq": l.expires_seq
                    })).unwrap_or(Value::Null),
                    "event": out.event.map(|e| json!({
                        "event_id": e.event_id(),
                        "ts": ts_ms_to_rfc3339(e.ts_ms),
                        "ts_ms": e.ts_ms,
                        "task_id": e.task_id,
                        "path": e.path,
                        "type": e.event_type,
                        "payload": parse_json_or_string(&e.payload_json)
                    })).unwrap_or(Value::Null),
                    "now_seq": out.now_seq
                }),
            ),
            Err(StoreError::StepLeaseNotHeld {
                step_id: leased_step_id,
                holder_agent_id,
            }) => ai_error_with(
                "STEP_LEASE_NOT_HELD",
                &match holder_agent_id {
                    None => format!("no active lease for step_id={leased_step_id}"),
                    Some(holder) => {
                        format!("lease for step_id={leased_step_id} is held by {holder}")
                    }
                },
                Some("Claim the lease (or ask the holder to release) before renewing."),
                lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id.as_deref(),
                    path.as_ref(),
                    Some(&agent_id),
                ),
            ),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }

    pub(crate) fn tool_tasks_step_lease_release(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, _kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let (step_id, path) = match super::lifecycle::require_step_selector(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(agent_id) = agent_id else {
            return ai_error("INVALID_INPUT", "agent_id is required");
        };

        let result = self.store.step_lease_release(
            &workspace,
            bm_storage::StepLeaseReleaseRequest {
                task_id: task_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                agent_id: agent_id.clone(),
            },
        );

        match result {
            Ok(out) => ai_ok(
                "step_lease_release",
                json!({
                    "task": task_id,
                    "step": { "step_id": out.step.step_id, "path": out.step.path },
                    "lease": Value::Null,
                    "event": out.event.map(|e| json!({
                        "event_id": e.event_id(),
                        "ts": ts_ms_to_rfc3339(e.ts_ms),
                        "ts_ms": e.ts_ms,
                        "task_id": e.task_id,
                        "path": e.path,
                        "type": e.event_type,
                        "payload": parse_json_or_string(&e.payload_json)
                    })).unwrap_or(Value::Null),
                    "now_seq": out.now_seq
                }),
            ),
            Err(StoreError::StepLeaseNotHeld {
                step_id: leased_step_id,
                holder_agent_id,
            }) => ai_error_with(
                "STEP_LEASE_NOT_HELD",
                &match holder_agent_id {
                    None => format!("no active lease for step_id={leased_step_id}"),
                    Some(holder) => {
                        format!("lease for step_id={leased_step_id} is held by {holder}")
                    }
                },
                Some(
                    "Ask the holder to release the lease (or take over explicitly) before proceeding.",
                ),
                lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id.as_deref(),
                    path.as_ref(),
                    Some(&agent_id),
                ),
            ),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
