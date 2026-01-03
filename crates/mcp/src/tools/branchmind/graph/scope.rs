#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

impl McpServer {
    pub(in super::super) fn resolve_reasoning_scope(
        &mut self,
        workspace: &WorkspaceId,
        input: ReasoningScopeInput,
    ) -> Result<ReasoningScope, Value> {
        self.resolve_reasoning_scope_with_options(workspace, input, false)
    }

    pub(in super::super) fn resolve_reasoning_scope_with_options(
        &mut self,
        workspace: &WorkspaceId,
        input: ReasoningScopeInput,
        read_only: bool,
    ) -> Result<ReasoningScope, Value> {
        let overrides_present = input.branch.is_some()
            || input.notes_doc.is_some()
            || input.graph_doc.is_some()
            || input.trace_doc.is_some();

        let mut input = input;

        // DX rule: if no explicit scope is provided, reuse the workspace focus as a default target.
        // This makes daily usage much cheaper (set focus once, then omit repetitive target fields).
        if input.target.is_none() && !overrides_present {
            let focus = match self.store.focus_get(workspace) {
                Ok(v) => v,
                Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
            };
            input.target = focus;
        }

        if input.target.is_some() && overrides_present {
            return Err(ai_error(
                "INVALID_INPUT",
                "provide either target or explicit branch/doc overrides, not both",
            ));
        }

        match input.target {
            Some(target_id) => {
                let kind = match parse_plan_or_task_kind(&target_id) {
                    Some(v) => v,
                    None => {
                        return Err(ai_error(
                            "INVALID_INPUT",
                            "target must start with PLAN- or TASK-",
                        ));
                    }
                };
                let reasoning = if read_only {
                    match self.store.reasoning_ref_get(workspace, &target_id, kind) {
                        Ok(Some(r)) => r,
                        Ok(None) => {
                            let derived = ReasoningRef::for_entity(kind, &target_id);
                            bm_storage::ReasoningRefRow {
                                branch: derived.branch,
                                notes_doc: derived.notes_doc,
                                graph_doc: derived.graph_doc,
                                trace_doc: derived.trace_doc,
                            }
                        }
                        Err(StoreError::UnknownId) => {
                            return Err(ai_error("UNKNOWN_ID", "Unknown target id"));
                        }
                        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
                    }
                } else {
                    match self.store.ensure_reasoning_ref(workspace, &target_id, kind) {
                        Ok(r) => r,
                        Err(StoreError::UnknownId) => {
                            return Err(ai_error("UNKNOWN_ID", "Unknown target id"));
                        }
                        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
                    }
                };
                Ok(ReasoningScope {
                    branch: reasoning.branch,
                    notes_doc: reasoning.notes_doc,
                    graph_doc: reasoning.graph_doc,
                    trace_doc: reasoning.trace_doc,
                })
            }
            None => {
                let branch = match input.branch {
                    Some(branch) => branch,
                    None => require_checkout_branch(&mut self.store, workspace)?,
                };
                if !self
                    .store
                    .branch_exists(workspace, &branch)
                    .unwrap_or(false)
                {
                    return Err(unknown_branch_error(workspace));
                }
                let notes_doc = input
                    .notes_doc
                    .unwrap_or_else(|| DEFAULT_NOTES_DOC.to_string());
                let graph_doc = input
                    .graph_doc
                    .unwrap_or_else(|| DEFAULT_GRAPH_DOC.to_string());
                let trace_doc = input
                    .trace_doc
                    .unwrap_or_else(|| DEFAULT_TRACE_DOC.to_string());
                Ok(ReasoningScope {
                    branch,
                    notes_doc,
                    graph_doc,
                    trace_doc,
                })
            }
        }
    }

    pub(in super::super) fn latest_doc_for_kind(
        docs: &[bm_storage::DocumentRow],
        kind: bm_storage::DocumentKind,
        fallback: &str,
    ) -> String {
        docs.iter()
            .find(|doc| doc.kind == kind)
            .map(|doc| doc.doc.clone())
            .unwrap_or_else(|| fallback.to_string())
    }
}
