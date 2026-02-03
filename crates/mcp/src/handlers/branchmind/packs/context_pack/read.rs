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
    pub agent_id: Option<&'a str>,
    pub all_lanes: bool,
    pub warm_archive: bool,
    pub notes_limit: usize,
    pub trace_limit: usize,
    pub limit_cards: usize,
    pub decisions_limit: usize,
    pub evidence_limit: usize,
    pub blockers_limit: usize,
    pub focus_step_tag: Option<&'a str>,
    pub focus_task_id: Option<&'a str>,
    pub focus_step_path: Option<&'a str>,
    pub read_only: bool,
}

fn doc_show_tail_or_error(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
    cursor: Option<i64>,
    limit: usize,
) -> Result<bm_storage::DocSlice, Value> {
    match server
        .store
        .doc_show_tail(workspace, branch, doc, cursor, limit)
    {
        Ok(v) => Ok(v),
        Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
    }
}

fn scan_step_scoped_doc_tail(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
    want_limit: usize,
    max_pages: usize,
    mut keep: impl FnMut(&Value) -> bool,
) -> Result<(Vec<Value>, Option<i64>, bool), Value> {
    if want_limit == 0 {
        return Ok((Vec::new(), None, false));
    }
    let want_limit = want_limit.min(200);

    let mut cursor: Option<i64> = None;
    let mut out_desc = Vec::<Value>::new();
    let mut last_next_cursor = None;
    let mut last_has_more = false;

    // Deterministic, bounded scan: walk backwards in fixed pages and keep only matching entries.
    // Storage clamps `limit` to 200, so we ask for the max page size to minimize round-trips.
    for _ in 0..max_pages.max(1) {
        let slice = doc_show_tail_or_error(server, workspace, branch, doc, cursor, 200)?;
        last_next_cursor = slice.next_cursor;
        last_has_more = slice.has_more;

        let entries = doc_entries_to_json(slice.entries);
        for entry in entries.iter().rev() {
            if keep(entry) {
                out_desc.push(entry.clone());
                if out_desc.len() >= want_limit {
                    break;
                }
            }
        }

        if out_desc.len() >= want_limit {
            break;
        }
        if !last_has_more {
            break;
        }
        cursor = last_next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let mut has_more = last_has_more;
    if out_desc.len() >= want_limit {
        has_more = true;
    }
    out_desc.truncate(want_limit);
    out_desc.reverse();
    Ok((out_desc, last_next_cursor, has_more))
}

pub(super) fn read_context_pack(
    server: &mut McpServer,
    args: ContextPackReadArgs<'_>,
) -> Result<ContextPackRead, Value> {
    let ContextPackReadArgs {
        workspace,
        scope,
        agent_id,
        all_lanes,
        warm_archive,
        notes_limit,
        trace_limit,
        limit_cards,
        decisions_limit,
        evidence_limit,
        blockers_limit,
        focus_step_tag,
        focus_task_id,
        focus_step_path,
        read_only,
    } = args;

    let tags_all = focus_step_tag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| vec![s.to_string()]);

    let (notes_entries, notes_next_cursor, notes_has_more) = if notes_limit == 0 {
        (Vec::new(), None, false)
    } else if let (Some(focus_task_id), Some(focus_step_path)) = (focus_task_id, focus_step_path) {
        let (entries, next_cursor, has_more) = scan_step_scoped_doc_tail(
            server,
            workspace,
            scope.branch.as_str(),
            scope.notes_doc.as_str(),
            notes_limit,
            3,
            |entry| {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    return false;
                }
                let meta = entry.get("meta").unwrap_or(&Value::Null);
                step_meta_matches(meta, focus_task_id, focus_step_path)
            },
        )?;
        (entries, next_cursor, has_more)
    } else {
        let slice = doc_show_tail_or_error(
            server,
            workspace,
            &scope.branch,
            &scope.notes_doc,
            None,
            notes_limit,
        )?;
        let mut entries = doc_entries_to_json(slice.entries);
        if !all_lanes {
            entries.retain(|entry| {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    return true;
                }
                let meta = entry.get("meta").unwrap_or(&Value::Null);
                !meta_is_draft(meta)
            });
        }
        (entries, slice.next_cursor, slice.has_more)
    };

    let (trace_entries, trace_next_cursor, trace_has_more) = if trace_limit == 0 {
        (Vec::new(), None, false)
    } else if let (Some(focus_task_id), Some(focus_step_path)) = (focus_task_id, focus_step_path) {
        let (entries, next_cursor, has_more) = scan_step_scoped_doc_tail(
            server,
            workspace,
            scope.branch.as_str(),
            scope.trace_doc.as_str(),
            trace_limit,
            3,
            |entry| {
                let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                match kind {
                    "event" => {
                        let task_id = entry.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                        if task_id != focus_task_id {
                            return false;
                        }
                        let Some(path) = entry.get("path").and_then(|v| v.as_str()) else {
                            return false;
                        };
                        step_path_matches(focus_step_path, path)
                    }
                    "note" => {
                        let meta = entry.get("meta").unwrap_or(&Value::Null);
                        step_meta_matches(meta, focus_task_id, focus_step_path)
                    }
                    _ => false,
                }
            },
        )?;
        (entries, next_cursor, has_more)
    } else {
        let slice = doc_show_tail_or_error(
            server,
            workspace,
            &scope.branch,
            &scope.trace_doc,
            None,
            trace_limit,
        )?;
        let mut entries = doc_entries_to_json(slice.entries);
        if !all_lanes {
            entries.retain(|entry| {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    return true;
                }
                let meta = entry.get("meta").unwrap_or(&Value::Null);
                !meta_is_draft(meta)
            });
        }
        (entries, slice.next_cursor, slice.has_more)
    };

    let cards = match fetch_relevance_first_cards(
        server,
        RelevanceFirstCardsArgs {
            workspace,
            branch: scope.branch.as_str(),
            graph_doc: scope.graph_doc.as_str(),
            cards_limit: limit_cards,
            focus_step_tag,
            agent_id,
            warm_archive,
            all_lanes,
            read_only,
        },
    ) {
        Ok(v) => v.cards,
        Err(resp) => return Err(resp),
    };

    let mut graph_query_or_empty =
        |request: bm_storage::GraphQueryRequest| -> Result<bm_storage::GraphQuerySlice, Value> {
            match server
                .store
                .graph_query(workspace, &scope.branch, &scope.graph_doc, request)
            {
                Ok(v) => Ok(v),
                Err(StoreError::UnknownBranch) => {
                    if read_only {
                        Ok(bm_storage::GraphQuerySlice {
                            nodes: Vec::new(),
                            edges: Vec::new(),
                            next_cursor: None,
                            has_more: false,
                        })
                    } else {
                        Err(unknown_branch_error(workspace))
                    }
                }
                Err(StoreError::InvalidInput(msg)) => Err(ai_error("INVALID_INPUT", msg)),
                Err(err) => Err(ai_error("STORE_ERROR", &format_store_error(err))),
            }
        };

    let mut decisions = Vec::new();
    if decisions_limit > 0 {
        let limit = decisions_limit;
        let slice = graph_query_or_empty(bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["decision".to_string()]),
            status: None,
            tags_any: None,
            tags_all: tags_all.clone(),
            text: None,
            cursor: None,
            limit,
            include_edges: false,
            edges_limit: 0,
        })?;
        decisions = graph_nodes_to_signal_cards(slice.nodes);
        if !all_lanes {
            decisions.retain(|card| card_value_visibility_allows(card, false, focus_step_tag));
        }
        decisions.truncate(decisions_limit);
    }

    let mut evidence = Vec::new();
    if evidence_limit > 0 {
        let limit = evidence_limit;
        let slice = graph_query_or_empty(bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["evidence".to_string()]),
            status: None,
            tags_any: None,
            tags_all: tags_all.clone(),
            text: None,
            cursor: None,
            limit,
            include_edges: false,
            edges_limit: 0,
        })?;
        evidence = graph_nodes_to_signal_cards(slice.nodes);
        if !all_lanes {
            evidence.retain(|card| card_value_visibility_allows(card, false, focus_step_tag));
        }
        evidence.truncate(evidence_limit);
    }

    let mut blockers = Vec::new();
    if blockers_limit > 0 {
        let limit = blockers_limit;
        let slice = graph_query_or_empty(bm_storage::GraphQueryRequest {
            ids: None,
            types: None,
            status: None,
            tags_any: Some(vec!["blocker".to_string()]),
            tags_all,
            text: None,
            cursor: None,
            limit,
            include_edges: false,
            edges_limit: 0,
        })?;
        blockers = graph_nodes_to_signal_cards(slice.nodes);
        if !all_lanes {
            blockers.retain(|card| card_value_visibility_allows(card, false, focus_step_tag));
        }
        blockers.truncate(blockers_limit);
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
