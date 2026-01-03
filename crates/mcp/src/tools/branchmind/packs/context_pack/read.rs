#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use crate::*;
use serde_json::Value;

pub(super) struct ContextPackDocSlice {
    pub entries: Vec<Value>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

pub(super) struct ContextPackDocs {
    pub notes: ContextPackDocSlice,
    pub trace: ContextPackDocSlice,
}

pub(super) struct ContextPackGraphData {
    pub cards: Vec<Value>,
    pub decisions: Vec<Value>,
    pub evidence: Vec<Value>,
    pub blockers: Vec<Value>,
    pub by_type: BTreeMap<String, u64>,
    pub stats_by_type: BTreeMap<String, u64>,
}

pub(super) struct ContextPackTotals {
    pub notes_count: usize,
    pub trace_count: usize,
    pub cards_total: usize,
    pub decisions_total: usize,
    pub evidence_total: usize,
    pub blockers_total: usize,
}

pub(super) struct ContextPackRead {
    pub docs: ContextPackDocs,
    pub graph: ContextPackGraphData,
    pub totals: ContextPackTotals,
}

pub(super) struct ContextPackReadArgs<'a> {
    pub workspace: &'a WorkspaceId,
    pub scope: &'a ReasoningScope,
    pub notes_limit: usize,
    pub trace_limit: usize,
    pub limit_cards: usize,
    pub decisions_limit: usize,
    pub evidence_limit: usize,
    pub blockers_limit: usize,
}

pub(super) fn read_context_pack(
    server: &mut McpServer,
    args: ContextPackReadArgs<'_>,
) -> Result<ContextPackRead, Value> {
    let ContextPackReadArgs {
        workspace,
        scope,
        notes_limit,
        trace_limit,
        limit_cards,
        decisions_limit,
        evidence_limit,
        blockers_limit,
    } = args;

    let notes_slice = match server.store.doc_show_tail(
        workspace,
        &scope.branch,
        &scope.notes_doc,
        None,
        notes_limit,
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let trace_slice = match server.store.doc_show_tail(
        workspace,
        &scope.branch,
        &scope.trace_doc,
        None,
        trace_limit,
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let bm_storage::DocSlice {
        entries: notes_entries_rows,
        next_cursor: notes_next_cursor,
        has_more: notes_has_more,
    } = notes_slice;
    let bm_storage::DocSlice {
        entries: trace_entries_rows,
        next_cursor: trace_next_cursor,
        has_more: trace_has_more,
    } = trace_slice;

    let notes_entries = doc_entries_to_json(notes_entries_rows);
    let trace_entries = doc_entries_to_json(trace_entries_rows);

    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    let slice = match server.store.graph_query(
        workspace,
        &scope.branch,
        &scope.graph_doc,
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(types),
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: limit_cards,
            include_edges: false,
            edges_limit: 0,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::UnknownBranch) => return Err(unknown_branch_error(workspace)),
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let cards = graph_nodes_to_cards(slice.nodes);

    let mut decisions = Vec::new();
    if decisions_limit > 0 {
        let slice = match server.store.graph_query(
            workspace,
            &scope.branch,
            &scope.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["decision".to_string()]),
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: decisions_limit,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => return Err(unknown_branch_error(workspace)),
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        decisions = graph_nodes_to_signal_cards(slice.nodes);
    }

    let mut evidence = Vec::new();
    if evidence_limit > 0 {
        let slice = match server.store.graph_query(
            workspace,
            &scope.branch,
            &scope.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(vec!["evidence".to_string()]),
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: evidence_limit,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => return Err(unknown_branch_error(workspace)),
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        evidence = graph_nodes_to_signal_cards(slice.nodes);
    }

    let mut blockers = Vec::new();
    if blockers_limit > 0 {
        let slice = match server.store.graph_query(
            workspace,
            &scope.branch,
            &scope.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: None,
                status: None,
                tags_any: Some(vec!["blocker".to_string()]),
                tags_all: None,
                text: None,
                cursor: None,
                limit: blockers_limit,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => return Err(unknown_branch_error(workspace)),
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };
        blockers = graph_nodes_to_signal_cards(slice.nodes);
    }

    let notes_count = notes_entries.len();
    let trace_count = trace_entries.len();
    let cards_total = cards.len();
    let decisions_total = decisions.len();
    let evidence_total = evidence.len();
    let blockers_total = blockers.len();

    let mut by_type = BTreeMap::<String, u64>::new();
    for card in &cards {
        if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
            *by_type.entry(ty.to_string()).or_insert(0) += 1;
        }
    }
    let stats_by_type = by_type.clone();

    Ok(ContextPackRead {
        docs: ContextPackDocs {
            notes: ContextPackDocSlice {
                entries: notes_entries,
                next_cursor: notes_next_cursor,
                has_more: notes_has_more,
            },
            trace: ContextPackDocSlice {
                entries: trace_entries,
                next_cursor: trace_next_cursor,
                has_more: trace_has_more,
            },
        },
        graph: ContextPackGraphData {
            cards,
            decisions,
            evidence,
            blockers,
            by_type,
            stats_by_type,
        },
        totals: ContextPackTotals {
            notes_count,
            trace_count,
            cards_total,
            decisions_total,
            evidence_total,
            blockers_total,
        },
    })
}
