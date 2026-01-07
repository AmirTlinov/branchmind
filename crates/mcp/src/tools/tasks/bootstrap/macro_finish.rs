#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_tasks_macro_finish(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let handoff_max_chars = match optional_usize(args_obj, "handoff_max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let final_note = match optional_string(args_obj, "final_note") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if args_obj.contains_key("final_note")
            && final_note.as_ref().is_some_and(|v| v.trim().is_empty())
        {
            return ai_error("INVALID_INPUT", "final_note must not be empty");
        }
        let status = args_obj
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("DONE")
            .trim()
            .to_string();

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (target_id, kind, focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        if kind != TaskKind::Task {
            let omit_workspace = self.default_workspace.as_deref() == Some(workspace.as_str());

            let mut snapshot_params = serde_json::Map::new();
            if !omit_workspace {
                snapshot_params.insert(
                    "workspace".to_string(),
                    Value::String(workspace.as_str().to_string()),
                );
            }
            snapshot_params.insert("plan".to_string(), Value::String(target_id));

            return ai_error_with(
                "INVALID_INPUT",
                "macro_finish requires a task target",
                Some(
                    "You are targeting a plan. Use tasks_macro_start to create a task under the plan (portal).",
                ),
                vec![
                    suggest_call(
                        "tasks_macro_start",
                        "Start a task under this plan (portal).",
                        "high",
                        json!({ "workspace": workspace.as_str(), "plan": args_obj.get("plan").cloned().unwrap_or(Value::Null), "task_title": "New task" }),
                    ),
                    suggest_call(
                        "tasks_snapshot",
                        "Open a snapshot to confirm context (portal).",
                        "medium",
                        Value::Object(snapshot_params),
                    ),
                ],
            );
        }
        let task_id = target_id;

        // Guard: macro_finish is only safe when steps are completed (for DONE).
        if status == "DONE" {
            match self.store.task_steps_summary(&workspace, &task_id) {
                Ok(summary) => {
                    if summary.open_steps > 0 {
                        let omit_workspace =
                            self.default_workspace.as_deref() == Some(workspace.as_str());

                        let need_task = focus
                            .as_deref()
                            .map(|focused| focused != task_id)
                            .unwrap_or(true);

                        let mut close_params = serde_json::Map::new();
                        if !omit_workspace {
                            close_params.insert(
                                "workspace".to_string(),
                                Value::String(workspace.as_str().to_string()),
                            );
                        }
                        if need_task {
                            close_params.insert("task".to_string(), Value::String(task_id.clone()));
                        }
                        close_params
                            .insert("checkpoints".to_string(), Value::String("gate".to_string()));

                        let mut snap_params = serde_json::Map::new();
                        if !omit_workspace {
                            snap_params.insert(
                                "workspace".to_string(),
                                Value::String(workspace.as_str().to_string()),
                            );
                        }
                        if need_task {
                            snap_params.insert("task".to_string(), Value::String(task_id.clone()));
                        }

                        return ai_error_with(
                            "INVALID_INPUT",
                            "steps not completed",
                            Some("Close the remaining open steps first (checkpoint gated)."),
                            vec![
                                suggest_call(
                                    "tasks_macro_close_step",
                                    "Close the next open step (portal).",
                                    "high",
                                    Value::Object(close_params),
                                ),
                                suggest_call(
                                    "tasks_snapshot",
                                    "Show current focus/step and next actions (portal).",
                                    "medium",
                                    Value::Object(snap_params),
                                ),
                            ],
                        );
                    }
                }
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown task id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
        }

        let mut warnings = Vec::new();

        // Idempotency: avoid emitting duplicate completion events when already in the target status.
        let already_in_status = match self.store.get_task(&workspace, &task_id) {
            Ok(Some(task)) => task.status == status,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut completion_result: Option<Value> = None;
        if !already_in_status {
            let complete = self.tool_tasks_complete(args.clone());
            if !complete
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return complete;
            }
            if let Some(w) = complete.get("warnings").and_then(|v| v.as_array()) {
                warnings.extend(w.clone());
            }
            completion_result = complete.get("result").cloned();
        } else {
            let code = if status == "DONE" {
                "ALREADY_DONE"
            } else {
                "ALREADY_STATUS"
            };
            warnings.push(warning(
                code,
                &format!("task is already {status}"),
                "No completion event was emitted.",
            ));
        }

        let mut final_note_entry = Value::Null;
        if let Some(final_note) = final_note.clone() {
            let reasoning_ref =
                match self
                    .store
                    .ensure_reasoning_ref(&workspace, &task_id, TaskKind::Task)
                {
                    Ok(r) => r,
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown task id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };

            let title = "Final note".to_string();
            let format = "text".to_string();
            let meta_json = json!({
                "source": "tasks_macro_finish",
                "task_id": task_id.clone(),
                "status": status.clone(),
                "kind": "final_note"
            })
            .to_string();

            let entry = match self.store.doc_append_note(
                &workspace,
                bm_storage::DocAppendRequest {
                    branch: reasoning_ref.branch,
                    doc: reasoning_ref.notes_doc,
                    title: Some(title),
                    format: Some(format),
                    meta_json: Some(meta_json),
                    content: final_note.trim().to_string(),
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            final_note_entry = json!({
                "seq": entry.seq,
                "ts": ts_ms_to_rfc3339(entry.ts_ms),
                "ts_ms": entry.ts_ms,
                "branch": entry.branch,
                "doc": entry.doc
            });
        }

        let mut handoff_args = serde_json::Map::new();
        handoff_args.insert(
            "workspace".to_string(),
            Value::String(workspace.as_str().to_string()),
        );
        handoff_args.insert("task".to_string(), Value::String(task_id.clone()));
        handoff_args.insert("read_only".to_string(), Value::Bool(true));
        if let Some(max_chars) = handoff_max_chars {
            handoff_args.insert(
                "max_chars".to_string(),
                Value::Number(serde_json::Number::from(max_chars as u64)),
            );
        }

        let handoff = self.tool_tasks_handoff(Value::Object(handoff_args));
        if !handoff
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return handoff;
        }

        if let Some(w) = handoff.get("warnings").and_then(|v| v.as_array()) {
            warnings.extend(w.clone());
        }

        let result = json!({
            "task": task_id,
            "status": completion_result
                .as_ref()
                .and_then(|v| v.get("task"))
                .and_then(|v| v.get("status"))
                .cloned()
                .unwrap_or(Value::String(status)),
            "complete": completion_result.unwrap_or(Value::Null),
            "final_note": final_note_entry,
            "handoff": handoff.get("result").cloned().unwrap_or(Value::Null)
        });

        if warnings.is_empty() {
            ai_ok("tasks_macro_finish", result)
        } else {
            ai_ok_with_warnings("tasks_macro_finish", result, warnings, Vec::new())
        }
    }
}
