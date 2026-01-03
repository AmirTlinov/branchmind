#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

use super::queries::graph_query_or_empty;

#[derive(Clone, Debug)]
pub(super) struct MemoryDocTail {
    pub(super) entries: Vec<Value>,
    pub(super) next_cursor: Option<i64>,
    pub(super) has_more: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperMemoryLoadArgs {
    pub(super) notes_cursor: Option<i64>,
    pub(super) notes_limit: usize,
    pub(super) trace_cursor: Option<i64>,
    pub(super) trace_limit: usize,
    pub(super) cards_cursor: Option<i64>,
    pub(super) cards_limit: usize,
    pub(super) read_only: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperMemory {
    pub(super) notes: MemoryDocTail,
    pub(super) trace: MemoryDocTail,
    pub(super) cards: Vec<Value>,
    pub(super) cards_next_cursor: Option<i64>,
    pub(super) cards_has_more: bool,
    pub(super) cards_limit: usize,
    pub(super) cards_cursor: Option<i64>,
    pub(super) stats_by_type: std::collections::BTreeMap<String, u64>,
}

pub(super) fn load_resume_super_memory(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    reasoning: &bm_storage::ReasoningRefRow,
    args: ResumeSuperMemoryLoadArgs,
    reasoning_branch_missing: &mut bool,
) -> Result<ResumeSuperMemory, Value> {
    let ResumeSuperMemoryLoadArgs {
        notes_cursor,
        notes_limit,
        trace_cursor,
        trace_limit,
        cards_cursor,
        cards_limit,
        read_only,
    } = args;

    let notes_slice = match server.store.doc_show_tail(
        workspace,
        &reasoning.branch,
        &reasoning.notes_doc,
        notes_cursor,
        notes_limit,
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let trace_slice = match server.store.doc_show_tail(
        workspace,
        &reasoning.branch,
        &reasoning.trace_doc,
        trace_cursor,
        trace_limit,
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let bm_storage::DocSlice {
        entries: notes_entries_raw,
        next_cursor: notes_next_cursor,
        has_more: notes_has_more,
    } = notes_slice;
    let bm_storage::DocSlice {
        entries: trace_entries_raw,
        next_cursor: trace_next_cursor,
        has_more: trace_has_more,
    } = trace_slice;

    let notes_entries = doc_entries_to_json(notes_entries_raw);
    let trace_entries = doc_entries_to_json(trace_entries_raw);

    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    let cards_slice = graph_query_or_empty(
        server,
        workspace,
        &reasoning.branch,
        &reasoning.graph_doc,
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(types),
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: cards_cursor,
            limit: cards_limit,
            include_edges: false,
            edges_limit: 0,
        },
        read_only,
        reasoning_branch_missing,
    )?;

    let cards_next_cursor = cards_slice.next_cursor;
    let cards_has_more = cards_slice.has_more;
    let cards = graph_nodes_to_cards(cards_slice.nodes);

    let mut by_type = std::collections::BTreeMap::<String, u64>::new();
    for card in &cards {
        if let Some(ty) = card.get("type").and_then(|v| v.as_str()) {
            *by_type.entry(ty.to_string()).or_insert(0) += 1;
        }
    }

    Ok(ResumeSuperMemory {
        notes: MemoryDocTail {
            entries: notes_entries,
            next_cursor: notes_next_cursor,
            has_more: notes_has_more,
        },
        trace: MemoryDocTail {
            entries: trace_entries,
            next_cursor: trace_next_cursor,
            has_more: trace_has_more,
        },
        cards,
        cards_next_cursor,
        cards_has_more,
        cards_limit,
        cards_cursor,
        stats_by_type: by_type,
    })
}
