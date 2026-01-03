#![forbid(unsafe_code)]

use super::super::*;
use super::{MergeBackCtx, MergeBackState};
use rusqlite::Transaction;

pub(super) fn apply_edge_candidate_tx(
    tx: &Transaction<'_>,
    ctx: &MergeBackCtx<'_>,
    theirs: &GraphEdgeRow,
    state: &mut MergeBackState,
) -> Result<(), StoreError> {
    let key = GraphEdgeKey {
        from: theirs.from.clone(),
        rel: theirs.rel.clone(),
        to: theirs.to.clone(),
    };
    let base = graph_edge_get_tx(tx, ctx.workspace, ctx.base_sources, ctx.doc, &key)?;
    let ours = graph_edge_get_tx(tx, ctx.workspace, ctx.into_sources, ctx.doc, &key)?;

    if graph_edge_semantic_eq(base.as_ref(), Some(theirs))
        || graph_edge_semantic_eq(ours.as_ref(), Some(theirs))
    {
        state.skipped += 1;
        return Ok(());
    }

    state.diff_summary.edges_changed += 1;
    state.diff_summary.edge_fields_changed += count_edge_field_changes(base.as_ref(), theirs);

    if graph_edge_semantic_eq(base.as_ref(), ours.as_ref()) {
        if ctx.dry_run {
            state.merged += 1;
            return Ok(());
        }

        let key_str = format!("{}|{}|{}", key.from, key.rel, key.to);
        let merge_key = format!(
            "graph_merge:{}:{}:edge:{}",
            ctx.from_branch, theirs.last_seq, key_str
        );
        let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
            from: key.from.clone(),
            rel: key.rel.clone(),
            to: key.to.clone(),
            meta_json: theirs.meta_json.clone(),
        });
        if let Some(seq) = insert_graph_doc_entry_tx(
            tx,
            ctx.workspace,
            ctx.into_branch,
            ctx.doc,
            ctx.now_ms,
            &op,
            Some(&merge_key),
        )?
        .1
        {
            let meta_json = merge_meta_json(
                theirs.meta_json.as_deref(),
                ctx.from_branch,
                theirs.last_seq,
                theirs.last_ts_ms,
            );
            insert_graph_edge_version_tx(
                tx,
                GraphEdgeVersionInsertTxArgs {
                    workspace: ctx.workspace,
                    branch: ctx.into_branch,
                    doc: ctx.doc,
                    seq,
                    ts_ms: ctx.now_ms,
                    from_id: &key.from,
                    rel: &key.rel,
                    to_id: &key.to,
                    meta_json: Some(meta_json.as_str()),
                    deleted: theirs.deleted,
                },
            )?;
            state.merged += 1;
        } else {
            state.skipped += 1;
        }
        return Ok(());
    }

    let preview = build_conflict_preview_edge(
        &ctx.preview_ctx,
        &key,
        base.as_ref(),
        Some(theirs),
        ours.as_ref(),
    );
    if !ctx.dry_run {
        let _ = graph_conflict_create_edge_tx(
            tx,
            &ctx.create_ctx,
            &key,
            base.as_ref(),
            Some(theirs),
            ours.as_ref(),
        )?;
    }

    state.conflicts_created += 1;
    state.conflict_ids.push(preview.conflict_id.clone());
    state.conflicts.push(preview);

    Ok(())
}
