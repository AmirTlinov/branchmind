#![forbid(unsafe_code)]

use super::*;
use serde_json::{Value, json};

fn bus_message_to_json(row: bm_storage::JobBusMessageRow) -> Value {
    json!({
        "seq": row.seq,
        "ts_ms": row.ts_ms,
        "thread_id": row.thread_id,
        "from_agent_id": row.from_agent_id,
        "from_job_id": row.from_job_id,
        "to_agent_id": row.to_agent_id,
        "kind": row.kind,
        "summary": row.summary,
        "refs": row.refs,
        "payload_json": row.payload_json,
        "idempotency_key": row.idempotency_key
    })
}

fn require_agent_id_or_default(
    server: &McpServer,
    args_obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, Value> {
    if let Some(v) = optional_agent_id(args_obj, key)? {
        return Ok(v);
    }
    if let Some(v) = server
        .default_agent_id
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Ok(v.to_string());
    }
    Err(ai_error(
        "INVALID_INPUT",
        &format!("{key} is required (or launch server with --agent-id)"),
    ))
}

impl McpServer {
    pub(crate) fn tool_tasks_jobs_mesh_publish(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if !self.jobs_mesh_v1_enabled {
            return ai_error_with(
                "NOT_ENABLED",
                "jobs mesh v1 is disabled",
                Some("Enable via BRANCHMIND_JOBS_MESH_V1=1 (or --jobs-mesh-v1)."),
                Vec::new(),
            );
        }

        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "thread_id",
                "kind",
                "summary",
                "refs",
                "plan_delta",
                "handoff",
                "idempotency_key",
                "from_agent_id",
                "from_job_id",
                "to_agent_id",
                "payload",
            ],
            "jobs.mesh.publish",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let thread_id = match require_string(args_obj, "thread_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let kind = match require_string(args_obj, "kind") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let summary = match require_string(args_obj, "summary") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let idempotency_key = match require_string(args_obj, "idempotency_key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from_agent_id = match require_agent_id_or_default(self, args_obj, "from_agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from_job_id = match optional_string(args_obj, "from_job_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to_agent_id = match optional_agent_id(args_obj, "to_agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        // CODE_REF validation (fail-closed on invalid format; drift => warning + keep message).
        let mut normalized_refs = Vec::<String>::new();
        for r in refs {
            match parse_code_ref(&r) {
                Ok(None) => normalized_refs.push(r),
                Ok(Some(code_ref)) => match validate_code_ref(&self.store, &workspace, &code_ref) {
                    Ok(v) => {
                        normalized_refs.push(v.normalized);
                        warnings.extend(v.warnings);
                    }
                    Err(resp) => return resp,
                },
                Err(resp) => return resp,
            }
        }

        // Optional structured payload (plan_delta / handoff / payload passthrough).
        let mut payload_obj = serde_json::Map::<String, Value>::new();
        if let Some(v) = args_obj.get("plan_delta").cloned().filter(|v| !v.is_null()) {
            payload_obj.insert("plan_delta".to_string(), v);
        }
        if let Some(v) = args_obj.get("handoff").cloned().filter(|v| !v.is_null()) {
            payload_obj.insert("handoff".to_string(), v);
        }
        if let Some(v) = args_obj.get("payload").cloned().filter(|v| !v.is_null()) {
            payload_obj.insert("payload".to_string(), v);
        }
        let payload_json = if payload_obj.is_empty() {
            None
        } else {
            serde_json::to_string(&Value::Object(payload_obj)).ok()
        };

        let published = match self.store.job_bus_publish(
            &workspace,
            bm_storage::JobBusPublishRequest {
                idempotency_key,
                thread_id,
                from_agent_id,
                from_job_id,
                to_agent_id,
                kind,
                summary,
                refs: normalized_refs,
                payload_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "deduped": published.deduped,
            "message": bus_message_to_json(published.message)
        });

        if warnings.is_empty() {
            ai_ok("tasks_jobs_mesh_publish", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_mesh_publish", result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_tasks_jobs_mesh_pull(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if !self.jobs_mesh_v1_enabled {
            return ai_error_with(
                "NOT_ENABLED",
                "jobs mesh v1 is disabled",
                Some("Enable via BRANCHMIND_JOBS_MESH_V1=1 (or --jobs-mesh-v1)."),
                Vec::new(),
            );
        }

        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "thread_id",
                "consumer_id",
                "after_seq",
                "limit",
                "max_chars",
                "fmt",
            ],
            "jobs.mesh.pull",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let thread_id = match require_string(args_obj, "thread_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let consumer_id = match require_agent_id_or_default(self, args_obj, "consumer_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let after_seq = match optional_i64(args_obj, "after_seq") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(50).clamp(1, 200),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let pulled = match self.store.job_bus_pull(
            &workspace,
            bm_storage::JobBusPullRequest {
                consumer_id: consumer_id.clone(),
                thread_id: thread_id.clone(),
                after_seq,
                limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let bm_storage::JobBusPullResult {
            messages,
            next_after_seq,
            has_more,
        } = pulled;
        let count = messages.len();

        let mut result = json!({
            "workspace": workspace.as_str(),
            "thread_id": thread_id,
            "consumer_id": consumer_id,
            "messages": messages.into_iter().map(bus_message_to_json).collect::<Vec<_>>(),
            "count": count,
            "next_after_seq": next_after_seq,
            "has_more": has_more,
            "truncated": false
        });

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, truncated) = enforce_graph_list_budget(&mut result, "messages", limit);
            set_truncated_flag(&mut result, truncated);
            let _used = attach_budget(&mut result, limit, truncated);

            warnings.extend(budget_warnings(truncated, false, clamped));
        }

        if warnings.is_empty() {
            ai_ok("tasks_jobs_mesh_pull", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_mesh_pull", result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_tasks_jobs_mesh_ack(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if !self.jobs_mesh_v1_enabled {
            return ai_error_with(
                "NOT_ENABLED",
                "jobs mesh v1 is disabled",
                Some("Enable via BRANCHMIND_JOBS_MESH_V1=1 (or --jobs-mesh-v1)."),
                Vec::new(),
            );
        }

        let unknown_warning = match check_unknown_args(
            args_obj,
            &["workspace", "thread_id", "consumer_id", "after_seq"],
            "jobs.mesh.ack",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let thread_id = match require_string(args_obj, "thread_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let consumer_id = match require_agent_id_or_default(self, args_obj, "consumer_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let after_seq = match optional_i64(args_obj, "after_seq") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let Some(after_seq) = after_seq else {
            return ai_error("INVALID_INPUT", "after_seq is required");
        };

        let acked = match self.store.job_bus_ack(
            &workspace,
            bm_storage::JobBusAckRequest {
                consumer_id,
                thread_id,
                after_seq,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let result = json!({
            "workspace": workspace.as_str(),
            "offset": {
                "consumer_id": acked.consumer_id,
                "thread_id": acked.thread_id,
                "after_seq": acked.after_seq,
                "updated_at_ms": acked.updated_at_ms
            }
        });

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);
        if warnings.is_empty() {
            ai_ok("tasks_jobs_mesh_ack", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_mesh_ack", result, warnings, Vec::new())
        }
    }

    pub(crate) fn tool_tasks_jobs_mesh_link(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if !self.jobs_mesh_v1_enabled {
            return ai_error_with(
                "NOT_ENABLED",
                "jobs mesh v1 is disabled",
                Some("Enable via BRANCHMIND_JOBS_MESH_V1=1 (or --jobs-mesh-v1)."),
                Vec::new(),
            );
        }

        let unknown_warning = match check_unknown_args(
            args_obj,
            &[
                "workspace",
                "from_thread",
                "to_thread",
                "kind",
                "summary",
                "refs",
                "idempotency_key",
                "from_agent_id",
            ],
            "jobs.mesh.link",
            self.jobs_unknown_args_fail_closed_enabled,
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let from_thread = match require_string(args_obj, "from_thread") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let to_thread = match require_string(args_obj, "to_thread") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let link_kind = match optional_string(args_obj, "kind") {
            Ok(v) => v.unwrap_or_else(|| "depends_on".to_string()),
            Err(resp) => return resp,
        };
        let summary = match optional_string(args_obj, "summary") {
            Ok(v) => v.unwrap_or_else(|| format!("{from_thread} -> {to_thread} ({link_kind})")),
            Err(resp) => return resp,
        };
        let idempotency_key = match require_string(args_obj, "idempotency_key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let from_agent_id = match require_agent_id_or_default(self, args_obj, "from_agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let refs = match optional_string_array(args_obj, "refs") {
            Ok(v) => v.unwrap_or_else(Vec::new),
            Err(resp) => return resp,
        };

        let mut payload = serde_json::Map::<String, Value>::new();
        payload.insert(
            "link".to_string(),
            json!({ "from_thread": from_thread, "to_thread": to_thread, "kind": link_kind }),
        );
        let payload_json = serde_json::to_string(&Value::Object(payload)).ok();

        // Store link edges in workspace/main (stable topology root).
        let published = match self.store.job_bus_publish(
            &workspace,
            bm_storage::JobBusPublishRequest {
                idempotency_key,
                thread_id: "workspace/main".to_string(),
                from_agent_id,
                from_job_id: None,
                to_agent_id: None,
                kind: "link".to_string(),
                summary,
                refs,
                payload_json,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut warnings = Vec::<Value>::new();
        push_warning_if(&mut warnings, unknown_warning);

        let result = json!({
            "workspace": workspace.as_str(),
            "deduped": published.deduped,
            "message": bus_message_to_json(published.message)
        });
        if warnings.is_empty() {
            ai_ok("tasks_jobs_mesh_link", result)
        } else {
            ai_ok_with_warnings("tasks_jobs_mesh_link", result, warnings, Vec::new())
        }
    }
}
