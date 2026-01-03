#![forbid(unsafe_code)]

use super::super::super::StoreError;
use super::tags::encode_tags;
use super::types::{GraphEdgeVersionInsertTxArgs, GraphNodeVersionInsertTxArgs};
use rusqlite::{Transaction, params};

pub(in crate::store) fn insert_graph_node_version_tx(
    tx: &Transaction<'_>,
    args: GraphNodeVersionInsertTxArgs<'_>,
) -> Result<(), StoreError> {
    let GraphNodeVersionInsertTxArgs {
        workspace,
        branch,
        doc,
        seq,
        ts_ms,
        node_id,
        node_type,
        title,
        text,
        tags,
        status,
        meta_json,
        deleted,
    } = args;

    let tags = encode_tags(tags);
    tx.execute(
        r#"
        INSERT INTO graph_node_versions(
          workspace, branch, doc, seq, ts_ms, node_id, node_type, title, text, tags, status, meta_json, deleted
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            workspace,
            branch,
            doc,
            seq,
            ts_ms,
            node_id,
            node_type,
            title,
            text,
            tags,
            status,
            meta_json,
            if deleted { 1i64 } else { 0i64 }
        ],
    )?;
    Ok(())
}

pub(in crate::store) fn insert_graph_edge_version_tx(
    tx: &Transaction<'_>,
    args: GraphEdgeVersionInsertTxArgs<'_>,
) -> Result<(), StoreError> {
    let GraphEdgeVersionInsertTxArgs {
        workspace,
        branch,
        doc,
        seq,
        ts_ms,
        from_id,
        rel,
        to_id,
        meta_json,
        deleted,
    } = args;

    tx.execute(
        r#"
        INSERT INTO graph_edge_versions(
          workspace, branch, doc, seq, ts_ms, from_id, rel, to_id, meta_json, deleted
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            workspace,
            branch,
            doc,
            seq,
            ts_ms,
            from_id,
            rel,
            to_id,
            meta_json,
            if deleted { 1i64 } else { 0i64 }
        ],
    )?;
    Ok(())
}
