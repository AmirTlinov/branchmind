#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct ContextPackBridgeContext {
    pub notes_count: usize,
    pub trace_count: usize,
    pub card_count: usize,
    pub decisions_total: usize,
    pub evidence_total: usize,
    pub blockers_total: usize,
}

impl McpServer {
    pub(super) fn maybe_build_context_pack_bridge(
        &mut self,
        workspace: &WorkspaceId,
        requested_target_present: bool,
        scope_branch: &str,
        ctx: ContextPackBridgeContext,
    ) -> (Option<Value>, Option<Value>) {
        let ContextPackBridgeContext {
            notes_count,
            trace_count,
            card_count,
            decisions_total,
            evidence_total,
            blockers_total,
        } = ctx;

        let mut bridge: Option<Value> = None;
        let mut bridge_warning: Option<Value> = None;
        if requested_target_present
            && notes_count == 0
            && trace_count == 0
            && card_count == 0
            && decisions_total == 0
            && evidence_total == 0
            && blockers_total == 0
            && let Ok(Some(checkout_branch)) = self.store.branch_checkout_get(workspace)
            && checkout_branch != scope_branch
        {
            let docs = self.store.doc_list(workspace, &checkout_branch);
            if let Ok(docs) = docs {
                let notes_doc = Self::latest_doc_for_kind(
                    &docs,
                    bm_storage::DocumentKind::Notes,
                    DEFAULT_NOTES_DOC,
                );
                let trace_doc = Self::latest_doc_for_kind(
                    &docs,
                    bm_storage::DocumentKind::Trace,
                    DEFAULT_TRACE_DOC,
                );
                let graph_doc = Self::latest_doc_for_kind(
                    &docs,
                    bm_storage::DocumentKind::Graph,
                    DEFAULT_GRAPH_DOC,
                );

                let notes_has = self
                    .store
                    .doc_show_tail(workspace, &checkout_branch, &notes_doc, None, 1)
                    .map(|slice| !slice.entries.is_empty())
                    .unwrap_or(false);
                let trace_has = self
                    .store
                    .doc_show_tail(workspace, &checkout_branch, &trace_doc, None, 1)
                    .map(|slice| !slice.entries.is_empty())
                    .unwrap_or(false);
                let graph_has = self
                    .store
                    .graph_query(
                        workspace,
                        &checkout_branch,
                        &graph_doc,
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
                    )
                    .map(|slice| !slice.nodes.is_empty())
                    .unwrap_or(false);

                if notes_has || trace_has || graph_has {
                    bridge = Some(json!({
                        "checkout": checkout_branch,
                        "docs": {
                            "notes": notes_doc,
                            "trace": trace_doc,
                            "graph": graph_doc
                        },
                        "has": {
                            "notes": notes_has,
                            "trace": trace_has,
                            "graph": graph_has
                        }
                    }));
                    bridge_warning = Some(warning(
                        "CONTEXT_EMPTY_FOR_TARGET",
                        "Target reasoning scope is empty; checkout branch has recent context.",
                        "Call context_pack with ref=<checkout> or seed reasoning via think_pipeline for the target.",
                    ));
                }
            }
        }

        (bridge, bridge_warning)
    }
}
