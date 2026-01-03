#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    fn build_context_health(
        &mut self,
        workspace: &WorkspaceId,
        target_id: &str,
        kind: TaskKind,
    ) -> Result<Value, StoreError> {
        let mut issues = Vec::new();
        let reasoning_ref = self.store.reasoning_ref_get(workspace, target_id, kind)?;
        let (reasoning, stored) = match reasoning_ref {
            Some(row) => (row, true),
            None => {
                let derived = ReasoningRef::for_entity(kind, target_id);
                (
                    bm_storage::ReasoningRefRow {
                        branch: derived.branch,
                        notes_doc: derived.notes_doc,
                        graph_doc: derived.graph_doc,
                        trace_doc: derived.trace_doc,
                    },
                    false,
                )
            }
        };

        if !stored {
            issues.push(json!({
                "severity": "warning",
                "code": "REASONING_REF_MISSING",
                "message": "reasoning refs are not persisted yet",
                "recovery": "Run tasks_resume_super with read_only=false or think_pipeline to seed reasoning refs."
            }));
        }

        let notes_has = self
            .store
            .doc_show_tail(workspace, &reasoning.branch, &reasoning.notes_doc, None, 1)
            .map(|slice| !slice.entries.is_empty())
            .unwrap_or(false);
        let trace_has = self
            .store
            .doc_show_tail(workspace, &reasoning.branch, &reasoning.trace_doc, None, 1)
            .map(|slice| !slice.entries.is_empty())
            .unwrap_or(false);
        let cards_has = match self.store.graph_query(
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 1,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(slice) => !slice.nodes.is_empty(),
            Err(StoreError::UnknownBranch) => {
                issues.push(json!({
                    "severity": "warning",
                    "code": "REASONING_BRANCH_MISSING",
                    "message": "reasoning branch is missing",
                    "recovery": "Seed reasoning via think_pipeline or switch read_only=false on resume tools."
                }));
                false
            }
            Err(StoreError::InvalidInput(msg)) => return Err(StoreError::InvalidInput(msg)),
            Err(err) => return Err(err),
        };

        if !notes_has && !trace_has && !cards_has {
            issues.push(json!({
                "severity": "warning",
                "code": "CONTEXT_EMPTY",
                "message": "notes/trace/graph are empty",
                "recovery": "Add a decision/evidence note or run think_pipeline to seed context."
            }));
        }

        if trace_has && !notes_has {
            issues.push(json!({
                "severity": "warning",
                "code": "TRACE_ONLY",
                "message": "trace has events but notes are empty",
                "recovery": "Summarize key decisions in notes to improve recall."
            }));
        }

        let status = if issues.is_empty() { "ok" } else { "warn" };

        Ok(json!({
            "status": status,
            "stats": {
                "notes_present": notes_has,
                "trace_present": trace_has,
                "cards_present": cards_has,
                "reasoning_ref": if stored { "stored" } else { "derived" }
            },
            "issues": issues
        }))
    }

    pub(crate) fn tool_tasks_lint(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let (target_id, kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };

        let mut issues = Vec::new();
        match kind {
            TaskKind::Plan => {
                let checklist = match self.store.plan_checklist_get(&workspace, &target_id) {
                    Ok(v) => v,
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let total = checklist.steps.len() as i64;
                if total == 0 {
                    issues.push(json!({
                        "severity": "warning",
                        "code": "NO_CHECKLIST",
                        "message": "plan checklist is empty",
                        "recovery": "Use tasks_plan to set checklist steps."
                    }));
                }
                if checklist.current < 0 || checklist.current > total {
                    issues.push(json!({
                        "severity": "error",
                        "code": "CHECKLIST_INDEX_OUT_OF_RANGE",
                        "message": format!("plan_current out of range: {}", checklist.current),
                        "recovery": "Use tasks_plan to set a valid current index."
                    }));
                }
            }
            TaskKind::Task => match self.store.task_steps_summary(&workspace, &target_id) {
                Ok(summary) => {
                    if summary.total_steps == 0 {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "NO_STEPS",
                            "message": "task has no steps",
                            "recovery": "Use tasks_decompose to add steps."
                        }));
                    }
                    if summary.missing_criteria > 0 {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "MISSING_CRITERIA",
                            "message": format!("missing criteria checkpoints: {}", summary.missing_criteria),
                            "recovery": "Fill success_criteria via tasks_define and confirm with tasks_verify."
                        }));
                    }
                    if summary.missing_tests > 0 {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "MISSING_TESTS",
                            "message": format!("missing tests checkpoints: {}", summary.missing_tests),
                            "recovery": "Set tests via tasks_define and confirm with tasks_verify."
                        }));
                    }
                    if summary.missing_security > 0 {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "MISSING_SECURITY",
                            "message": format!("missing security checkpoints: {}", summary.missing_security),
                            "recovery": "Confirm security checkpoint via tasks_verify."
                        }));
                    }
                    if summary.missing_perf > 0 {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "MISSING_PERF",
                            "message": format!("missing perf checkpoints: {}", summary.missing_perf),
                            "recovery": "Confirm perf checkpoint via tasks_verify."
                        }));
                    }
                    if summary.missing_docs > 0 {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "MISSING_DOCS",
                            "message": format!("missing docs checkpoints: {}", summary.missing_docs),
                            "recovery": "Confirm docs checkpoint via tasks_verify."
                        }));
                    }
                    if let Ok(blockers) = self.store.task_open_blockers(&workspace, &target_id, 1)
                        && !blockers.is_empty()
                    {
                        issues.push(json!({
                            "severity": "warning",
                            "code": "BLOCKED_STEPS",
                            "message": "task has blocked steps",
                            "recovery": "Use tasks_context/tasks_resume to locate blockers and clear via tasks_block."
                        }));
                    }
                }
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
        }

        let context_health = match self.build_context_health(&workspace, &target_id, kind) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let (errors, warnings) = issues.iter().fold((0, 0), |acc, item| {
            match item.get("severity").and_then(|v| v.as_str()) {
                Some("error") => (acc.0 + 1, acc.1),
                Some("warning") => (acc.0, acc.1 + 1),
                _ => acc,
            }
        });

        ai_ok(
            "lint",
            json!({
                "workspace": workspace.as_str(),
                "target": { "id": target_id, "kind": kind.as_str() },
                "summary": {
                    "errors": errors,
                    "warnings": warnings,
                    "total": errors + warnings
                },
                "issues": issues,
                "context_health": context_health
            }),
        )
    }
}
