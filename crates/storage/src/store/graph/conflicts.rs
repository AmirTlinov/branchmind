#![forbid(unsafe_code)]

use super::*;
use bm_core::ids::WorkspaceId;
use rusqlite::params;

impl SqliteStore {
    pub fn graph_conflicts_list(
        &mut self,
        workspace: &WorkspaceId,
        into_branch: &str,
        doc: &str,
        status: Option<&str>,
        cursor: Option<i64>,
        limit: usize,
    ) -> Result<(Vec<GraphConflictSummary>, Option<i64>, bool), StoreError> {
        if into_branch.trim().is_empty() {
            return Err(StoreError::InvalidInput("into_branch must not be empty"));
        }
        if doc.trim().is_empty() {
            return Err(StoreError::InvalidInput("doc must not be empty"));
        }

        let before_created_at = cursor.unwrap_or(i64::MAX);
        let limit = limit.clamp(1, 200) as i64;
        let status = status.unwrap_or("open");
        let tx = self.conn.transaction()?;

        if !branch_exists_tx(&tx, workspace.as_str(), into_branch)? {
            return Err(StoreError::UnknownBranch);
        }

        let mut out = Vec::new();
        {
            let mut stmt = tx.prepare(
                r#"
                SELECT conflict_id, kind, key, status, created_at_ms
                FROM graph_conflicts
                WHERE workspace=?1 AND into_branch=?2 AND doc=?3 AND status=?4 AND created_at_ms < ?5
                ORDER BY created_at_ms DESC
                LIMIT ?6
                "#,
            )?;

            let mut rows = stmt.query(params![
                workspace.as_str(),
                into_branch,
                doc,
                status,
                before_created_at,
                limit + 1
            ])?;

            while let Some(row) = rows.next()? {
                out.push(GraphConflictSummary {
                    conflict_id: row.get(0)?,
                    kind: row.get(1)?,
                    key: row.get(2)?,
                    status: row.get(3)?,
                    created_at_ms: row.get(4)?,
                });
            }
        }

        let has_more = out.len() as i64 > limit;
        if has_more {
            out.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            out.last().map(|c| c.created_at_ms)
        } else {
            None
        };

        tx.commit()?;
        Ok((out, next_cursor, has_more))
    }

    pub fn graph_conflict_show(
        &mut self,
        workspace: &WorkspaceId,
        conflict_id: &str,
    ) -> Result<GraphConflictDetail, StoreError> {
        validate_conflict_id(conflict_id)?;

        let tx = self.conn.transaction()?;
        let row = graph_conflict_detail_row_tx(&tx, workspace.as_str(), conflict_id)?
            .ok_or(StoreError::UnknownConflict)?;
        tx.commit()?;

        Ok(row.into_detail(conflict_id))
    }

    pub fn graph_conflict_resolve(
        &mut self,
        workspace: &WorkspaceId,
        conflict_id: &str,
        resolution: &str,
    ) -> Result<GraphConflictResolveResult, StoreError> {
        validate_conflict_id(conflict_id)?;
        if resolution.trim().is_empty() {
            return Err(StoreError::InvalidInput("resolution must not be empty"));
        }

        let now_ms = now_ms();
        let tx = self.conn.transaction()?;

        let row = graph_conflict_detail_row_tx(&tx, workspace.as_str(), conflict_id)?
            .ok_or(StoreError::UnknownConflict)?;
        let detail = row.into_detail(conflict_id);
        if detail.status != "open" {
            return Err(StoreError::ConflictAlreadyResolved);
        }

        match resolution {
            "use_into" => {
                tx.execute(
                    "UPDATE graph_conflicts SET status='resolved', resolution=?3, resolved_at_ms=?4 WHERE workspace=?1 AND conflict_id=?2",
                    params![workspace.as_str(), conflict_id, resolution, now_ms],
                )?;
                tx.commit()?;
                return Ok(GraphConflictResolveResult {
                    conflict_id: conflict_id.to_string(),
                    status: "resolved".to_string(),
                    applied: false,
                    applied_seq: None,
                });
            }
            "use_from" => {}
            _ => {
                return Err(StoreError::InvalidInput(
                    "resolution must be use_from|use_into",
                ));
            }
        }

        ensure_workspace_tx(&tx, workspace, now_ms)?;
        ensure_document_tx(
            &tx,
            workspace.as_str(),
            &detail.into_branch,
            &detail.doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;

        let source_event_id = format!("graph_conflict_resolve:{conflict_id}");

        let (applied, applied_seq) = match detail.kind.as_str() {
            "node" => {
                let Some(theirs) = detail.theirs_node.as_ref() else {
                    return Err(StoreError::InvalidInput(
                        "conflict has no theirs node snapshot",
                    ));
                };
                let op = GraphOp::NodeUpsert(GraphNodeUpsert {
                    id: theirs.id.clone(),
                    node_type: theirs.node_type.clone(),
                    title: theirs.title.clone(),
                    text: theirs.text.clone(),
                    tags: theirs.tags.clone(),
                    status: theirs.status.clone(),
                    meta_json: theirs.meta_json.clone(),
                });
                let inserted = insert_graph_doc_entry_tx(
                    &tx,
                    workspace.as_str(),
                    &detail.into_branch,
                    &detail.doc,
                    now_ms,
                    &op,
                    Some(&source_event_id),
                )?;
                match inserted.1 {
                    None => (false, None),
                    Some(seq) => {
                        let meta_json = merge_meta_json(
                            theirs.meta_json.as_deref(),
                            &detail.from_branch,
                            theirs.last_seq,
                            theirs.last_ts_ms,
                        );
                        insert_graph_node_version_tx(
                            &tx,
                            GraphNodeVersionInsertTxArgs {
                                workspace: workspace.as_str(),
                                branch: &detail.into_branch,
                                doc: &detail.doc,
                                seq,
                                ts_ms: now_ms,
                                node_id: &theirs.id,
                                node_type: Some(theirs.node_type.as_str()),
                                title: theirs.title.as_deref(),
                                text: theirs.text.as_deref(),
                                tags: &theirs.tags,
                                status: theirs.status.as_deref(),
                                meta_json: Some(meta_json.as_str()),
                                deleted: theirs.deleted,
                            },
                        )?;
                        touch_document_tx(
                            &tx,
                            workspace.as_str(),
                            &detail.into_branch,
                            &detail.doc,
                            now_ms,
                        )?;
                        (true, Some(seq))
                    }
                }
            }
            "edge" => {
                let Some(theirs) = detail.theirs_edge.as_ref() else {
                    return Err(StoreError::InvalidInput(
                        "conflict has no theirs edge snapshot",
                    ));
                };
                let op = GraphOp::EdgeUpsert(GraphEdgeUpsert {
                    from: theirs.from.clone(),
                    rel: theirs.rel.clone(),
                    to: theirs.to.clone(),
                    meta_json: theirs.meta_json.clone(),
                });
                let inserted = insert_graph_doc_entry_tx(
                    &tx,
                    workspace.as_str(),
                    &detail.into_branch,
                    &detail.doc,
                    now_ms,
                    &op,
                    Some(&source_event_id),
                )?;
                match inserted.1 {
                    None => (false, None),
                    Some(seq) => {
                        let meta_json = merge_meta_json(
                            theirs.meta_json.as_deref(),
                            &detail.from_branch,
                            theirs.last_seq,
                            theirs.last_ts_ms,
                        );
                        insert_graph_edge_version_tx(
                            &tx,
                            GraphEdgeVersionInsertTxArgs {
                                workspace: workspace.as_str(),
                                branch: &detail.into_branch,
                                doc: &detail.doc,
                                seq,
                                ts_ms: now_ms,
                                from_id: &theirs.from,
                                rel: &theirs.rel,
                                to_id: &theirs.to,
                                meta_json: Some(meta_json.as_str()),
                                deleted: theirs.deleted,
                            },
                        )?;
                        touch_document_tx(
                            &tx,
                            workspace.as_str(),
                            &detail.into_branch,
                            &detail.doc,
                            now_ms,
                        )?;
                        (true, Some(seq))
                    }
                }
            }
            _ => return Err(StoreError::InvalidInput("unknown conflict kind")),
        };

        tx.execute(
            "UPDATE graph_conflicts SET status='resolved', resolution=?3, resolved_at_ms=?4 WHERE workspace=?1 AND conflict_id=?2",
            params![workspace.as_str(), conflict_id, resolution, now_ms],
        )?;

        tx.commit()?;
        Ok(GraphConflictResolveResult {
            conflict_id: conflict_id.to_string(),
            status: "resolved".to_string(),
            applied,
            applied_seq,
        })
    }
}
