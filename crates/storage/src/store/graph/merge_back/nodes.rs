#![forbid(unsafe_code)]

use super::super::*;
use super::{MergeBackCtx, MergeBackState};
use rusqlite::Transaction;

pub(super) fn apply_node_candidate_tx(
    tx: &Transaction<'_>,
    ctx: &MergeBackCtx<'_>,
    theirs: &GraphNodeRow,
    state: &mut MergeBackState,
) -> Result<(), StoreError> {
    let key = theirs.id.clone();
    let base = graph_node_get_tx(tx, ctx.workspace, ctx.base_sources, ctx.doc, &key)?;
    let ours = graph_node_get_tx(tx, ctx.workspace, ctx.into_sources, ctx.doc, &key)?;

    if graph_node_semantic_eq(base.as_ref(), Some(theirs))
        || graph_node_semantic_eq(ours.as_ref(), Some(theirs))
    {
        state.skipped += 1;
        return Ok(());
    }

    state.diff_summary.nodes_changed += 1;
    state.diff_summary.node_fields_changed += count_node_field_changes(base.as_ref(), theirs);

    if graph_node_semantic_eq(base.as_ref(), ours.as_ref()) {
        if ctx.dry_run {
            state.merged += 1;
            return Ok(());
        }

        let merge_key = format!(
            "graph_merge:{}:{}:node:{}",
            ctx.from_branch, theirs.last_seq, key
        );
        let op = GraphOp::NodeUpsert(GraphNodeUpsert {
            id: key.clone(),
            node_type: theirs.node_type.clone(),
            title: theirs.title.clone(),
            text: theirs.text.clone(),
            tags: theirs.tags.clone(),
            status: theirs.status.clone(),
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
            insert_graph_node_version_tx(
                tx,
                GraphNodeVersionInsertTxArgs {
                    workspace: ctx.workspace,
                    branch: ctx.into_branch,
                    doc: ctx.doc,
                    seq,
                    ts_ms: ctx.now_ms,
                    node_id: &key,
                    node_type: Some(theirs.node_type.as_str()),
                    title: theirs.title.as_deref(),
                    text: theirs.text.as_deref(),
                    tags: &theirs.tags,
                    status: theirs.status.as_deref(),
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

    // Diverged: create conflict.
    let preview = build_conflict_preview_node(
        &ctx.preview_ctx,
        &key,
        base.as_ref(),
        Some(theirs),
        ours.as_ref(),
    );
    if !ctx.dry_run {
        let _ = graph_conflict_create_node_tx(
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
