#![forbid(unsafe_code)]

use crate::store::{GraphConflictDetail, GraphEdgeRow, GraphNodeRow};

use super::super::types::{GraphConflictIdArgs, GraphConflictPreviewCtx, GraphEdgeKey};
use super::id::graph_conflict_id;

pub(in crate::store) fn build_conflict_preview_node(
    ctx: &GraphConflictPreviewCtx<'_>,
    key: &str,
    base: Option<&GraphNodeRow>,
    theirs: Option<&GraphNodeRow>,
    ours: Option<&GraphNodeRow>,
) -> GraphConflictDetail {
    let theirs_seq = theirs.map(|n| n.last_seq).unwrap_or(0);
    let ours_seq = ours.map(|n| n.last_seq).unwrap_or(0);
    let conflict_id = graph_conflict_id(GraphConflictIdArgs {
        workspace: ctx.workspace,
        from_branch: ctx.from_branch,
        into_branch: ctx.into_branch,
        doc: ctx.doc,
        kind: "node",
        key,
        base_cutoff_seq: ctx.base_cutoff_seq,
        theirs_seq,
        ours_seq,
    });

    GraphConflictDetail {
        conflict_id,
        kind: "node".to_string(),
        key: key.to_string(),
        from_branch: ctx.from_branch.to_string(),
        into_branch: ctx.into_branch.to_string(),
        doc: ctx.doc.to_string(),
        status: ctx.status.to_string(),
        created_at_ms: ctx.now_ms,
        resolved_at_ms: None,
        base_node: base.cloned(),
        theirs_node: theirs.cloned(),
        ours_node: ours.cloned(),
        base_edge: None,
        theirs_edge: None,
        ours_edge: None,
    }
}

pub(in crate::store) fn build_conflict_preview_edge(
    ctx: &GraphConflictPreviewCtx<'_>,
    key: &GraphEdgeKey,
    base: Option<&GraphEdgeRow>,
    theirs: Option<&GraphEdgeRow>,
    ours: Option<&GraphEdgeRow>,
) -> GraphConflictDetail {
    let theirs_seq = theirs.map(|n| n.last_seq).unwrap_or(0);
    let ours_seq = ours.map(|n| n.last_seq).unwrap_or(0);
    let key_str = format!("{}|{}|{}", key.from, key.rel, key.to);
    let conflict_id = graph_conflict_id(GraphConflictIdArgs {
        workspace: ctx.workspace,
        from_branch: ctx.from_branch,
        into_branch: ctx.into_branch,
        doc: ctx.doc,
        kind: "edge",
        key: &key_str,
        base_cutoff_seq: ctx.base_cutoff_seq,
        theirs_seq,
        ours_seq,
    });

    GraphConflictDetail {
        conflict_id,
        kind: "edge".to_string(),
        key: key_str,
        from_branch: ctx.from_branch.to_string(),
        into_branch: ctx.into_branch.to_string(),
        doc: ctx.doc.to_string(),
        status: ctx.status.to_string(),
        created_at_ms: ctx.now_ms,
        resolved_at_ms: None,
        base_node: None,
        theirs_node: None,
        ours_node: None,
        base_edge: base.cloned(),
        theirs_edge: theirs.cloned(),
        ours_edge: ours.cloned(),
    }
}
