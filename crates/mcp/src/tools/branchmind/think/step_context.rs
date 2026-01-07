#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(crate) struct ResolvedStepContext {
    pub(crate) task_id: String,
    pub(crate) step: bm_storage::StepRef,
    pub(crate) step_tag: String,
}

pub(crate) fn resolve_step_context_from_args(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    args_obj: &serde_json::Map<String, Value>,
    step_raw: &str,
) -> Result<ResolvedStepContext, Value> {
    let step_raw = step_raw.trim();
    if step_raw.is_empty() {
        return Err(ai_error("INVALID_INPUT", "step must not be empty"));
    }

    // Step scoping is intentionally tied to TASK targets/focus. If a caller overrides the
    // reasoning scope manually, we cannot validate the step selector deterministically.
    let branch_override = optional_string(args_obj, "branch")?;
    let trace_doc_override = optional_string(args_obj, "trace_doc")?;
    let graph_doc_override = optional_string(args_obj, "graph_doc")?;
    let notes_doc_override = optional_string(args_obj, "notes_doc")?;
    let ref_override = optional_string(args_obj, "ref")?;
    let doc_override = optional_string(args_obj, "doc")?;

    let overrides_present = branch_override.is_some()
        || trace_doc_override.is_some()
        || graph_doc_override.is_some()
        || notes_doc_override.is_some()
        || ref_override.is_some()
        || doc_override.is_some();

    if overrides_present {
        return Err(ai_error(
            "INVALID_INPUT",
            "step cannot be used with explicit branch/doc overrides; use target/focus scope",
        ));
    }

    let explicit_target = args_obj
        .get("target")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let target_id = match explicit_target {
        Some(id) => id,
        None => match server.store.focus_get(workspace) {
            Ok(Some(focus)) => focus,
            Ok(None) => {
                return Err(ai_error(
                    "INVALID_INPUT",
                    "step requires a TASK target (or a workspace focus set to a TASK)",
                ));
            }
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        },
    };

    match parse_plan_or_task_kind(&target_id) {
        Some(TaskKind::Task) => {}
        Some(TaskKind::Plan) => {
            return Err(ai_error(
                "INVALID_INPUT",
                "step is only supported for TASK-* targets (plans have no steps table)",
            ));
        }
        None => {
            return Err(ai_error(
                "INVALID_INPUT",
                "target must start with TASK- when step is provided",
            ));
        }
    }

    let step = if step_raw.eq_ignore_ascii_case("focus") {
        let summary = match server.store.task_steps_summary(workspace, &target_id) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        let Some(first_open) = summary.first_open else {
            return Err(ai_error_with(
                "INVALID_INPUT",
                "step=\"focus\" requires at least one open step",
                Some(
                    "If the task has no steps, decompose it first. If all steps are closed, finish the task or pass an explicit step selector (STEP-... or StepPath).",
                ),
                vec![],
            ));
        };
        bm_storage::StepRef {
            step_id: first_open.step_id,
            path: first_open.path,
        }
    } else {
        let (step_id, step_path) = if step_raw.starts_with("STEP-") {
            (Some(step_raw.to_string()), None)
        } else {
            let path = StepPath::parse(step_raw).map_err(|_| {
                ai_error(
                    "INVALID_INPUT",
                    "step must be a STEP-... id or a StepPath like s:0 or s:0.s:1",
                )
            })?;
            (None, Some(path))
        };

        match server.store.step_resolve(
            workspace,
            &target_id,
            step_id.as_deref(),
            step_path.as_ref(),
        ) {
            Ok(v) => v,
            Err(StoreError::StepNotFound) => {
                return Err(ai_error("UNKNOWN_ID", "Unknown step selector"));
            }
            Err(StoreError::UnknownId) => {
                return Err(ai_error("UNKNOWN_ID", "Unknown task id"));
            }
            Err(StoreError::InvalidInput(msg)) => {
                return Err(ai_error("INVALID_INPUT", msg));
            }
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        }
    };

    Ok(ResolvedStepContext {
        task_id: target_id,
        step_tag: step_tag_for(&step.step_id),
        step,
    })
}

pub(super) fn apply_step_context_to_card(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    args_obj: &serde_json::Map<String, Value>,
    parsed: &mut ParsedThinkCard,
) -> Result<(), Value> {
    let step_raw = optional_string(args_obj, "step")?;
    let Some(step_raw) = step_raw else {
        return Ok(());
    };

    let ResolvedStepContext {
        task_id,
        step,
        step_tag,
    } = resolve_step_context_from_args(server, workspace, args_obj, &step_raw)?;

    // Attach a canonical tag for graph querying and an explicit meta.step reference for introspection.
    {
        use std::collections::BTreeSet;
        let mut tags = BTreeSet::<String>::new();
        tags.extend(parsed.tags.iter().cloned());
        tags.insert(step_tag);
        parsed.tags = tags.into_iter().collect();
    }

    if let Value::Object(obj) = &mut parsed.meta_value {
        obj.insert(
            "step".to_string(),
            json!({
                "task_id": task_id,
                "step_id": step.step_id,
                "path": step.path
            }),
        );
    }

    Ok(())
}
