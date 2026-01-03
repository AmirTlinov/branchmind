#![forbid(unsafe_code)]

use super::super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::Transaction;

pub(super) struct MergeBackSetup<'a> {
    pub base_cutoff_seq: i64,
    pub base_sources: Vec<BranchSource>,
    pub into_sources: Vec<BranchSource>,
    pub preview_ctx: GraphConflictPreviewCtx<'a>,
    pub create_ctx: GraphConflictCreateCtx<'a>,
}

pub(super) fn prepare_merge_back_tx<'a>(
    tx: &Transaction<'_>,
    workspace: &'a WorkspaceId,
    from_branch: &'a str,
    into_branch: &'a str,
    doc: &'a str,
    dry_run: bool,
    now_ms: i64,
) -> Result<MergeBackSetup<'a>, StoreError> {
    if !branch_exists_tx(tx, workspace.as_str(), from_branch)?
        || !branch_exists_tx(tx, workspace.as_str(), into_branch)?
    {
        return Err(StoreError::UnknownBranch);
    }

    let Some((base_branch, base_cutoff_seq)) =
        branch_base_info_tx(tx, workspace.as_str(), from_branch)?
    else {
        return Err(StoreError::MergeNotSupported);
    };
    if base_branch != into_branch {
        return Err(StoreError::MergeNotSupported);
    }

    if !dry_run {
        ensure_workspace_tx(tx, workspace, now_ms)?;
        ensure_document_tx(
            tx,
            workspace.as_str(),
            into_branch,
            doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
    }

    let base_sources = base_sources_for_branch_tx(tx, workspace.as_str(), from_branch)?;
    let into_sources = branch_sources_tx(tx, workspace.as_str(), into_branch)?;

    let conflict_status = if dry_run { "preview" } else { "open" };
    let preview_ctx = GraphConflictPreviewCtx {
        workspace: workspace.as_str(),
        from_branch,
        into_branch,
        doc,
        base_cutoff_seq,
        now_ms,
        status: conflict_status,
    };
    let create_ctx = GraphConflictCreateCtx {
        workspace: workspace.as_str(),
        from_branch,
        into_branch,
        doc,
        base_cutoff_seq,
        now_ms,
    };

    Ok(MergeBackSetup {
        base_cutoff_seq,
        base_sources,
        into_sources,
        preview_ctx,
        create_ctx,
    })
}
