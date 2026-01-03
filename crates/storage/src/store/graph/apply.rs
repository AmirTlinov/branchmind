#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;

impl SqliteStore {
    pub fn graph_apply_ops(
        &mut self,
        workspace: &WorkspaceId,
        branch: &str,
        doc: &str,
        ops: Vec<GraphOp>,
    ) -> Result<GraphApplyResult, StoreError> {
        if branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }
        if ops.is_empty() {
            return Err(StoreError::InvalidInput("ops must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;
        ensure_workspace_tx(&tx, workspace, now_ms)?;

        if !branch_exists_tx(&tx, workspace.as_str(), branch)? {
            return Err(StoreError::UnknownBranch);
        }

        ensure_document_tx(
            &tx,
            workspace.as_str(),
            branch,
            doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;

        let mut nodes_upserted = 0usize;
        let mut nodes_deleted = 0usize;
        let mut edges_upserted = 0usize;
        let mut edges_deleted = 0usize;
        let mut last_seq = 0i64;

        for op in ops {
            let (content, seq_opt) =
                insert_graph_doc_entry_tx(&tx, workspace.as_str(), branch, doc, now_ms, &op, None)?;
            let Some(seq) = seq_opt else {
                // No dedup key was provided, so this should be unreachable.
                return Err(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows));
            };
            last_seq = seq;

            match op {
                GraphOp::NodeUpsert(upsert) => {
                    validate_graph_node_id(&upsert.id)?;
                    validate_graph_type(&upsert.node_type)?;
                    let tags = normalize_tags(&upsert.tags)?;
                    insert_graph_node_version_tx(
                        &tx,
                        GraphNodeVersionInsertTxArgs {
                            workspace: workspace.as_str(),
                            branch,
                            doc,
                            seq,
                            ts_ms: now_ms,
                            node_id: &upsert.id,
                            node_type: Some(&upsert.node_type),
                            title: upsert.title.as_deref(),
                            text: upsert.text.as_deref(),
                            tags: &tags,
                            status: upsert.status.as_deref(),
                            meta_json: upsert.meta_json.as_deref(),
                            deleted: false,
                        },
                    )?;
                    nodes_upserted += 1;
                }
                GraphOp::NodeDelete { id } => {
                    validate_graph_node_id(&id)?;
                    let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
                    let Some(existing) =
                        graph_node_get_tx(&tx, workspace.as_str(), &sources, doc, &id)?
                    else {
                        return Err(StoreError::InvalidInput("node not found"));
                    };
                    if existing.deleted {
                        return Err(StoreError::InvalidInput("node already deleted"));
                    }

                    insert_graph_node_version_tx(
                        &tx,
                        GraphNodeVersionInsertTxArgs {
                            workspace: workspace.as_str(),
                            branch,
                            doc,
                            seq,
                            ts_ms: now_ms,
                            node_id: &id,
                            node_type: Some(existing.node_type.as_str()),
                            title: existing.title.as_deref(),
                            text: existing.text.as_deref(),
                            tags: &existing.tags,
                            status: existing.status.as_deref(),
                            meta_json: existing.meta_json.as_deref(),
                            deleted: true,
                        },
                    )?;
                    nodes_deleted += 1;

                    // Cascade-delete edges connected to this node in the current effective view.
                    let edge_keys =
                        graph_edge_keys_for_node_tx(&tx, workspace.as_str(), &sources, doc, &id)?;
                    for key in edge_keys {
                        insert_graph_edge_version_tx(
                            &tx,
                            GraphEdgeVersionInsertTxArgs {
                                workspace: workspace.as_str(),
                                branch,
                                doc,
                                seq,
                                ts_ms: now_ms,
                                from_id: &key.from,
                                rel: &key.rel,
                                to_id: &key.to,
                                meta_json: None,
                                deleted: true,
                            },
                        )?;
                        edges_deleted += 1;
                    }
                }
                GraphOp::EdgeUpsert(upsert) => {
                    validate_graph_node_id(&upsert.from)?;
                    validate_graph_node_id(&upsert.to)?;
                    validate_graph_rel(&upsert.rel)?;

                    // Require endpoints to exist in the effective view (avoid dangling edges).
                    let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
                    let Some(from_node) =
                        graph_node_get_tx(&tx, workspace.as_str(), &sources, doc, &upsert.from)?
                    else {
                        return Err(StoreError::InvalidInput("edge.from node not found"));
                    };
                    if from_node.deleted {
                        return Err(StoreError::InvalidInput("edge.from node is deleted"));
                    }
                    let Some(to_node) =
                        graph_node_get_tx(&tx, workspace.as_str(), &sources, doc, &upsert.to)?
                    else {
                        return Err(StoreError::InvalidInput("edge.to node not found"));
                    };
                    if to_node.deleted {
                        return Err(StoreError::InvalidInput("edge.to node is deleted"));
                    }

                    insert_graph_edge_version_tx(
                        &tx,
                        GraphEdgeVersionInsertTxArgs {
                            workspace: workspace.as_str(),
                            branch,
                            doc,
                            seq,
                            ts_ms: now_ms,
                            from_id: &upsert.from,
                            rel: &upsert.rel,
                            to_id: &upsert.to,
                            meta_json: upsert.meta_json.as_deref(),
                            deleted: false,
                        },
                    )?;
                    edges_upserted += 1;
                }
                GraphOp::EdgeDelete { from, rel, to } => {
                    validate_graph_node_id(&from)?;
                    validate_graph_node_id(&to)?;
                    validate_graph_rel(&rel)?;

                    let sources = branch_sources_tx(&tx, workspace.as_str(), branch)?;
                    let key = GraphEdgeKey {
                        from: from.clone(),
                        rel: rel.clone(),
                        to: to.clone(),
                    };
                    let Some(existing) =
                        graph_edge_get_tx(&tx, workspace.as_str(), &sources, doc, &key)?
                    else {
                        return Err(StoreError::InvalidInput("edge not found"));
                    };
                    if existing.deleted {
                        return Err(StoreError::InvalidInput("edge already deleted"));
                    }

                    insert_graph_edge_version_tx(
                        &tx,
                        GraphEdgeVersionInsertTxArgs {
                            workspace: workspace.as_str(),
                            branch,
                            doc,
                            seq,
                            ts_ms: now_ms,
                            from_id: &from,
                            rel: &rel,
                            to_id: &to,
                            meta_json: existing.meta_json.as_deref(),
                            deleted: true,
                        },
                    )?;
                    edges_deleted += 1;
                }
            }

            let _ = content;
        }

        touch_document_tx(&tx, workspace.as_str(), branch, doc, now_ms)?;
        tx.commit()?;

        Ok(GraphApplyResult {
            nodes_upserted,
            nodes_deleted,
            edges_upserted,
            edges_deleted,
            last_seq,
            last_ts_ms: now_ms,
        })
    }
}
