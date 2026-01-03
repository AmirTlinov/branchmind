#![forbid(unsafe_code)]

use super::super::super::*;
use rusqlite::Transaction;

pub(super) struct GraphUpsertArgs<'a> {
    pub(super) workspace: &'a str,
    pub(super) branch: &'a str,
    pub(super) graph_doc: &'a str,
    pub(super) card_id: &'a str,
    pub(super) card_type: &'a str,
    pub(super) card: &'a ThinkCardInput,
    pub(super) tags: &'a [String],
    pub(super) supports: &'a [String],
    pub(super) blocks: &'a [String],
    pub(super) now_ms: i64,
}

pub(super) struct GraphUpsertResult {
    pub(super) nodes_upserted: usize,
    pub(super) edges_upserted: usize,
    pub(super) last_seq: Option<i64>,
}

pub(super) fn upsert_graph_semantics_tx(
    tx: &Transaction<'_>,
    args: GraphUpsertArgs<'_>,
) -> Result<GraphUpsertResult, StoreError> {
    let GraphUpsertArgs {
        workspace,
        branch,
        graph_doc,
        card_id,
        card_type,
        card,
        tags,
        supports,
        blocks,
        now_ms,
    } = args;

    // Graph: idempotent semantic upserts for node + support/block edges.
    ensure_document_tx(
        tx,
        workspace,
        branch,
        graph_doc,
        DocumentKind::Graph.as_str(),
        now_ms,
    )?;

    let sources = branch_sources_tx(tx, workspace, branch)?;

    let mut nodes_upserted = 0usize;
    let mut edges_upserted = 0usize;
    let mut last_seq: Option<i64> = None;
    let mut touched_graph = false;

    let existing_node = graph_node_get_tx(tx, workspace, &sources, graph_doc, card_id)?;
    let candidate_node = GraphNodeRow {
        id: card_id.to_string(),
        node_type: card_type.to_string(),
        title: card.title.clone(),
        text: card.text.clone(),
        tags: tags.to_vec(),
        status: card.status.clone(),
        meta_json: card.meta_json.clone(),
        deleted: false,
        last_seq: 0,
        last_ts_ms: 0,
    };

    if !graph_node_semantic_eq(existing_node.as_ref(), Some(&candidate_node)) {
        let op = GraphOp::NodeUpsert(GraphNodeUpsert {
            id: candidate_node.id.clone(),
            node_type: candidate_node.node_type.clone(),
            title: candidate_node.title.clone(),
            text: candidate_node.text.clone(),
            tags: tags.to_vec(),
            status: candidate_node.status.clone(),
            meta_json: candidate_node.meta_json.clone(),
        });
        let dedup = format!("think_card:{card_id}:node");
        let (_payload, seq_opt) =
            insert_graph_doc_entry_tx(tx, workspace, branch, graph_doc, now_ms, &op, Some(&dedup))?;
        let Some(seq) = seq_opt else {
            return Err(StoreError::InvalidInput(
                "dedup prevented node write (card_id collision)",
            ));
        };
        insert_graph_node_version_tx(
            tx,
            GraphNodeVersionInsertTxArgs {
                workspace,
                branch,
                doc: graph_doc,
                seq,
                ts_ms: now_ms,
                node_id: &candidate_node.id,
                node_type: Some(&candidate_node.node_type),
                title: candidate_node.title.as_deref(),
                text: candidate_node.text.as_deref(),
                tags,
                status: candidate_node.status.as_deref(),
                meta_json: candidate_node.meta_json.as_deref(),
                deleted: false,
            },
        )?;
        nodes_upserted += 1;
        last_seq = Some(seq);
        touched_graph = true;
    }

    for to_id in supports.iter() {
        let updated = upsert_edge_if_needed_tx(
            tx,
            EdgeUpsertArgs {
                workspace,
                branch,
                graph_doc,
                sources: &sources,
                from_id: card_id,
                rel: "supports",
                to_id,
                now_ms,
            },
        )?;
        if let Some(seq) = updated {
            edges_upserted += 1;
            last_seq = Some(seq);
            touched_graph = true;
        }
    }
    for to_id in blocks.iter() {
        let updated = upsert_edge_if_needed_tx(
            tx,
            EdgeUpsertArgs {
                workspace,
                branch,
                graph_doc,
                sources: &sources,
                from_id: card_id,
                rel: "blocks",
                to_id,
                now_ms,
            },
        )?;
        if let Some(seq) = updated {
            edges_upserted += 1;
            last_seq = Some(seq);
            touched_graph = true;
        }
    }

    if touched_graph {
        touch_document_tx(tx, workspace, branch, graph_doc, now_ms)?;
    }

    Ok(GraphUpsertResult {
        nodes_upserted,
        edges_upserted,
        last_seq,
    })
}

struct EdgeUpsertArgs<'a> {
    workspace: &'a str,
    branch: &'a str,
    graph_doc: &'a str,
    sources: &'a [BranchSource],
    from_id: &'a str,
    rel: &'a str,
    to_id: &'a str,
    now_ms: i64,
}

fn upsert_edge_if_needed_tx(
    tx: &Transaction<'_>,
    args: EdgeUpsertArgs<'_>,
) -> Result<Option<i64>, StoreError> {
    let EdgeUpsertArgs {
        workspace,
        branch,
        graph_doc,
        sources,
        from_id,
        rel,
        to_id,
        now_ms,
    } = args;

    validate_graph_rel(rel)?;
    validate_graph_node_id(to_id)?;

    let key = GraphEdgeKey {
        from: from_id.to_string(),
        rel: rel.to_string(),
        to: to_id.to_string(),
    };
    let existing = graph_edge_get_tx(tx, workspace, sources, graph_doc, &key)?;
    let candidate = GraphEdgeRow {
        from: key.from.clone(),
        rel: key.rel.clone(),
        to: key.to.clone(),
        meta_json: None,
        deleted: false,
        last_seq: 0,
        last_ts_ms: 0,
    };
    if graph_edge_semantic_eq(existing.as_ref(), Some(&candidate)) {
        return Ok(None);
    }

    let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
        from: key.from.clone(),
        rel: key.rel.clone(),
        to: key.to.clone(),
        meta_json: None,
    });
    let dedup = format!("think_card:{from_id}:edge:{rel}:{to_id}");
    let (_payload, seq_opt) =
        insert_graph_doc_entry_tx(tx, workspace, branch, graph_doc, now_ms, &op, Some(&dedup))?;
    let Some(seq) = seq_opt else {
        return Err(StoreError::InvalidInput(
            "dedup prevented edge write (card_id collision)",
        ));
    };
    insert_graph_edge_version_tx(
        tx,
        GraphEdgeVersionInsertTxArgs {
            workspace,
            branch,
            doc: graph_doc,
            seq,
            ts_ms: now_ms,
            from_id: &key.from,
            rel: &key.rel,
            to_id: &key.to,
            meta_json: None,
            deleted: false,
        },
    )?;
    Ok(Some(seq))
}
