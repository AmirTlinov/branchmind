#![forbid(unsafe_code)]

use super::*;
use rusqlite::Transaction;

impl SqliteStore {
    pub(in crate::store) fn project_task_graph_contains_edge_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        from: &str,
        to: &str,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let source_event_id = format!("task_graph:{}:edge:{from}:contains:{to}", event.event_id());
        graph_upsert_edge_tx(
            tx,
            GraphEdgeUpsertTxArgs {
                workspace,
                branch: &reasoning.branch,
                doc: &reasoning.graph_doc,
                now_ms,
                from,
                rel: "contains",
                to,
                meta_json: None,
                source_event_id: &source_event_id,
            },
        )
    }

    pub(in crate::store) fn project_task_graph_task_node_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        task_id: &str,
        title: &str,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let node_id = task_graph_node_id(task_id);
        let meta_json = build_task_graph_meta_json(task_id);
        let source_event_id = format!("task_graph:{}:node:{node_id}", event.event_id());
        graph_upsert_node_tx(
            tx,
            GraphNodeUpsertTxArgs {
                workspace,
                branch: &reasoning.branch,
                doc: &reasoning.graph_doc,
                now_ms,
                node_id: &node_id,
                node_type: "task",
                title: Some(title),
                status: None,
                meta_json: Some(meta_json.as_str()),
                source_event_id: &source_event_id,
            },
        )
    }

    pub(in crate::store) fn project_task_graph_delete_node_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        node_id: &str,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let source_event_id = format!("task_graph:{}:node_delete:{node_id}", event.event_id());
        graph_delete_node_tx(
            tx,
            GraphNodeDeleteTxArgs {
                workspace,
                branch: &reasoning.branch,
                doc: &reasoning.graph_doc,
                now_ms,
                node_id,
                source_event_id: &source_event_id,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::store) fn project_task_graph_step_node_tx(
        tx: &Transaction<'_>,
        workspace: &str,
        reasoning: &ReasoningRefRow,
        event: &EventRow,
        task_id: &str,
        step: &StepRef,
        title: &str,
        completed: bool,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        ensure_document_tx(
            tx,
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            DocumentKind::Graph.as_str(),
            now_ms,
        )?;
        let node_id = step_graph_node_id(&step.step_id);
        let meta_json = build_step_graph_meta_json(task_id, step);
        let status = if completed {
            Some("done")
        } else {
            Some("open")
        };
        let source_event_id = format!("task_graph:{}:node:{node_id}", event.event_id());
        graph_upsert_node_tx(
            tx,
            GraphNodeUpsertTxArgs {
                workspace,
                branch: &reasoning.branch,
                doc: &reasoning.graph_doc,
                now_ms,
                node_id: &node_id,
                node_type: "step",
                title: Some(title),
                status,
                meta_json: Some(meta_json.as_str()),
                source_event_id: &source_event_id,
            },
        )
    }
}
