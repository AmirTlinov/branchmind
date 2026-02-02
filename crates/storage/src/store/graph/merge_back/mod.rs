#![forbid(unsafe_code)]

mod edges;
mod nodes;
mod setup;

use super::*;
use bm_core::ids::WorkspaceId;

pub(super) struct MergeBackCtx<'a> {
    pub workspace: &'a str,
    pub from_branch: &'a str,
    pub into_branch: &'a str,
    pub doc: &'a str,
    pub now_ms: i64,
    pub dry_run: bool,
    pub base_sources: &'a [BranchSource],
    pub into_sources: &'a [BranchSource],
    pub preview_ctx: GraphConflictPreviewCtx<'a>,
    pub create_ctx: GraphConflictCreateCtx<'a>,
}

pub(super) struct MergeBackState {
    pub merged: usize,
    pub skipped: usize,
    pub conflicts_detected: usize,
    pub conflicts_created: usize,
    pub conflict_ids: Vec<String>,
    pub conflicts: Vec<GraphConflictDetail>,
    pub processed: usize,
    pub diff_summary: GraphMergeDiffSummary,
}

impl MergeBackState {
    fn new() -> Self {
        Self {
            merged: 0,
            skipped: 0,
            conflicts_detected: 0,
            conflicts_created: 0,
            conflict_ids: Vec::new(),
            conflicts: Vec::new(),
            processed: 0,
            diff_summary: GraphMergeDiffSummary {
                nodes_changed: 0,
                edges_changed: 0,
                node_fields_changed: 0,
                edge_fields_changed: 0,
            },
        }
    }

    fn reached_limit(&self, limit: i64) -> bool {
        self.merged as i64 + self.skipped as i64 + self.conflicts_detected as i64 >= limit
    }
}

impl SqliteStore {
    pub fn graph_merge_back(
        &mut self,
        workspace: &WorkspaceId,
        request: GraphMergeBackRequest,
    ) -> Result<GraphMergeResult, StoreError> {
        let GraphMergeBackRequest {
            from_branch,
            into_branch,
            doc,
            cursor,
            limit,
            dry_run,
        } = request;

        if from_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("from_branch must not be empty"));
        }
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_seq = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let scan_limit = (limit * 5).clamp(limit, 1000);
        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let setup::MergeBackSetup {
            base_cutoff_seq,
            base_sources,
            into_sources,
            preview_ctx,
            create_ctx,
        } = setup::prepare_merge_back_tx(
            &tx,
            workspace,
            &from_branch,
            &into_branch,
            &doc,
            dry_run,
            now_ms,
        )?;

        let candidates = graph_merge_candidates_tx(
            &tx,
            workspace.as_str(),
            &from_branch,
            &doc,
            base_cutoff_seq,
            before_seq,
            scan_limit + 1,
        )?;

        let ctx = MergeBackCtx {
            workspace: workspace.as_str(),
            from_branch: &from_branch,
            into_branch: &into_branch,
            doc: &doc,
            now_ms,
            dry_run,
            base_sources: &base_sources,
            into_sources: &into_sources,
            preview_ctx,
            create_ctx,
        };
        let mut state = MergeBackState::new();

        for candidate in candidates.iter().take(scan_limit as usize) {
            if state.reached_limit(limit) {
                break;
            }
            state.processed += 1;

            match candidate {
                GraphMergeCandidate::Node { theirs, .. } => {
                    nodes::apply_node_candidate_tx(&tx, &ctx, theirs, &mut state)?;
                }
                GraphMergeCandidate::Edge { theirs, .. } => {
                    edges::apply_edge_candidate_tx(&tx, &ctx, theirs, &mut state)?;
                }
            }
        }

        if !dry_run && state.merged > 0 {
            touch_document_tx(&tx, workspace.as_str(), &into_branch, &doc, now_ms)?;
        }

        let has_more = candidates.len() > state.processed;
        let next_cursor = if has_more {
            candidates
                .get(state.processed.saturating_sub(1))
                .map(|c| c.last_seq())
        } else {
            None
        };

        tx.commit()?;
        Ok(GraphMergeResult {
            merged: state.merged,
            skipped: state.skipped,
            conflicts_detected: state.conflicts_detected,
            conflicts_created: state.conflicts_created,
            conflict_ids: state.conflict_ids,
            conflicts: state.conflicts,
            diff_summary: state.diff_summary,
            count: state.processed,
            next_cursor,
            has_more,
        })
    }
}
