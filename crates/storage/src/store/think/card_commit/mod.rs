#![forbid(unsafe_code)]

mod graph;
mod input;
mod trace;

use super::super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn think_card_commit(
        &mut self,
        workspace: &WorkspaceId,
        request: ThinkCardCommitRequest,
    ) -> Result<ThinkCardCommitResult, StoreError> {
        let validated = input::validate(request)?;
        let now_ms = now_ms();

        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !branch_exists_tx(&tx, workspace.as_str(), validated.branch.as_str())? {
            return Err(StoreError::UnknownBranch);
        }

        let (inserted, trace_seq) = trace::insert_trace_entry_if_needed_tx(
            &tx,
            workspace.as_str(),
            validated.branch.as_str(),
            validated.trace_doc.as_str(),
            &validated.card,
            validated.card_id.as_str(),
            now_ms,
        )?;

        let graph_result = graph::upsert_graph_semantics_tx(
            &tx,
            graph::GraphUpsertArgs {
                workspace: workspace.as_str(),
                branch: validated.branch.as_str(),
                graph_doc: validated.graph_doc.as_str(),
                card_id: validated.card_id.as_str(),
                card_type: validated.card_type.as_str(),
                card: &validated.card,
                tags: &validated.tags,
                supports: &validated.supports,
                blocks: &validated.blocks,
                now_ms,
            },
        )?;

        // Meaning map: keep a cheap, queryable index from anchors → cards across graphs.
        // Best-effort: invalid anchor tags are ignored (no hard failure).
        let _ = crate::store::anchor_links::upsert_anchor_links_for_card_tx(
            &tx,
            workspace.as_str(),
            crate::store::anchor_links::UpsertAnchorLinksForCardTxArgs {
                branch: validated.branch.as_str(),
                graph_doc: validated.graph_doc.as_str(),
                card_id: validated.card_id.as_str(),
                card_type: validated.card_type.as_str(),
                tags: &validated.tags,
                now_ms,
            },
        )?;

        // Knowledge key index: (anchor_id, key) → card_id, updated_at_ms.
        //
        // Guard: keep think_card_commit idempotent — only touch indexes when the graph was
        // semantically updated (node/edges changed).
        if graph_result.last_seq.is_some() {
            crate::store::knowledge_keys::upsert_knowledge_keys_for_card_tx(
                &tx,
                workspace.as_str(),
                crate::store::knowledge_keys::UpsertKnowledgeKeysForCardTxArgs {
                    card_id: validated.card_id.as_str(),
                    card_type: validated.card_type.as_str(),
                    tags: &validated.tags,
                    now_ms,
                },
            )?;
        }

        tx.commit()?;

        Ok(ThinkCardCommitResult {
            inserted,
            nodes_upserted: graph_result.nodes_upserted,
            edges_upserted: graph_result.edges_upserted,
            trace_seq,
            last_seq: graph_result.last_seq,
        })
    }
}
