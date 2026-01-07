#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_evidence_capture(&mut self, args: Value) -> Value {
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
        let expected_revision = match optional_i64(args_obj, "expected_revision") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let agent_id = match optional_agent_id(args_obj, "agent_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let step_id = match optional_string(args_obj, "step_id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let path = match optional_step_path(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let items_value = if args_obj.contains_key("items") {
            args_obj.get("items").cloned().unwrap_or(Value::Null)
        } else {
            args_obj.get("artifacts").cloned().unwrap_or(Value::Null)
        };
        let items = if items_value.is_null() {
            Vec::new()
        } else {
            let Some(arr) = items_value.as_array() else {
                return ai_error("INVALID_INPUT", "items must be an array");
            };
            arr.clone()
        };

        if items.len() > 20 {
            return ai_error("INVALID_INPUT", "items exceeds max_items=20");
        }

        let mut artifacts = Vec::new();
        for item in items {
            let Some(item_obj) = item.as_object() else {
                return ai_error("INVALID_INPUT", "items entries must be objects");
            };
            let kind = match require_string(item_obj, "kind") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let command = item_obj
                .get("command")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let stdout = item_obj
                .get("stdout")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let stderr = item_obj
                .get("stderr")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let exit_code = item_obj.get("exit_code").and_then(|v| v.as_i64());
            let diff = item_obj
                .get("diff")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let content = item_obj
                .get("content")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let url = item_obj
                .get("url")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let external_uri = item_obj
                .get("external_uri")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let meta_json = item_obj.get("meta").map(|v| v.to_string());

            let mut size = 0usize;
            for text in [
                command.as_deref(),
                stdout.as_deref(),
                stderr.as_deref(),
                diff.as_deref(),
                content.as_deref(),
                url.as_deref(),
                external_uri.as_deref(),
                meta_json.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                size = size.saturating_add(text.len());
            }
            if size > 256000 {
                return ai_error(
                    "INVALID_INPUT",
                    "artifact exceeds max_artifact_bytes=256000",
                );
            }

            artifacts.push(bm_storage::EvidenceArtifactInput {
                kind,
                command,
                stdout,
                stderr,
                exit_code,
                diff,
                content,
                url,
                external_uri,
                meta_json,
            });
        }

        let checks_value = args_obj.get("checks").cloned().unwrap_or(Value::Null);
        let checks = if checks_value.is_null() {
            Vec::new()
        } else {
            let Some(arr) = checks_value.as_array() else {
                return ai_error("INVALID_INPUT", "checks must be an array of strings");
            };
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let Some(s) = item.as_str() else {
                    return ai_error("INVALID_INPUT", "checks must be an array of strings");
                };
                out.push(s.to_string());
            }
            out
        };

        let attachments_value = args_obj.get("attachments").cloned().unwrap_or(Value::Null);
        let attachments = if attachments_value.is_null() {
            Vec::new()
        } else {
            let Some(arr) = attachments_value.as_array() else {
                return ai_error("INVALID_INPUT", "attachments must be an array of strings");
            };
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let Some(s) = item.as_str() else {
                    return ai_error("INVALID_INPUT", "attachments must be an array of strings");
                };
                out.push(s.to_string());
            }
            out
        };

        let checkpoint_value = args_obj.get("checkpoint").cloned().unwrap_or(Value::Null);
        let mut checkpoints = if checkpoint_value.is_null() {
            Vec::new()
        } else if let Some(s) = checkpoint_value.as_str() {
            vec![s.to_string()]
        } else if let Some(arr) = checkpoint_value.as_array() {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let Some(s) = item.as_str() else {
                    return ai_error(
                        "INVALID_INPUT",
                        "checkpoint must be a string or array of strings",
                    );
                };
                out.push(s.to_string());
            }
            out
        } else {
            return ai_error(
                "INVALID_INPUT",
                "checkpoint must be a string or array of strings",
            );
        };
        checkpoints.retain(|s| !s.trim().is_empty());
        checkpoints.sort();
        checkpoints.dedup();
        for checkpoint in checkpoints.iter() {
            if !matches!(
                checkpoint.as_str(),
                "criteria" | "tests" | "security" | "perf" | "docs"
            ) {
                return ai_error(
                    "INVALID_INPUT",
                    "checkpoint must be one of: criteria, tests, security, perf, docs",
                );
            }
        }

        let result = self.store.evidence_capture(
            &workspace,
            bm_storage::EvidenceCaptureRequest {
                task_id: task_id.clone(),
                expected_revision,
                agent_id: agent_id.clone(),
                selector: bm_storage::StepSelector {
                    step_id: step_id.clone(),
                    path: path.clone(),
                },
                artifacts,
                checks,
                attachments,
                checkpoints,
            },
        );

        match result {
            Ok(out) => ai_ok(
                "evidence_capture",
                json!({
                    "task": task_id,
                    "revision": out.revision,
                    "step": out.step.map(|s| json!({ "step_id": s.step_id, "path": s.path })).unwrap_or(Value::Null),
                    "event": {
                        "event_id": out.event.event_id(),
                        "ts": ts_ms_to_rfc3339(out.event.ts_ms),
                        "ts_ms": out.event.ts_ms,
                        "task_id": out.event.task_id,
                        "path": out.event.path,
                        "type": out.event.event_type,
                        "payload": parse_json_or_string(&out.event.payload_json)
                    }
                }),
            ),
            Err(StoreError::StepNotFound) => ai_error("UNKNOWN_ID", "Step not found"),
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
                Some("Ask the holder to release, wait for expiry, or take over explicitly."),
                super::super::lease::lease_error_suggestions(
                    &workspace,
                    &task_id,
                    step_id.as_deref(),
                    path.as_ref(),
                    agent_id.as_deref(),
                ),
            ),
            Err(StoreError::UnknownId) => ai_error("UNKNOWN_ID", "Unknown id"),
            Err(StoreError::InvalidInput(msg)) => ai_error("INVALID_INPUT", msg),
            Err(err) => ai_error("STORE_ERROR", &format_store_error(err)),
        }
    }
}
