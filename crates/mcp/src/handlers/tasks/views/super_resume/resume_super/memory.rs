#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

use super::args::ResumeSuperView;
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
    pub(super) focus_step_tag: Option<String>,
    pub(super) focus_task_id: Option<String>,
    pub(super) focus_step_path: Option<String>,
    pub(super) view: ResumeSuperView,
    pub(super) read_only: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ResumeSuperMemory {
    pub(super) notes: MemoryDocTail,
    pub(super) trace: MemoryDocTail,
    pub(super) cards: Vec<Value>,
    pub(super) edges: Vec<Value>,
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
        focus_step_tag,
        focus_task_id,
        focus_step_path,
        view,
        read_only,
    } = args;

    let all_lanes = matches!(view, ResumeSuperView::Audit);

    let notes_slice = if notes_limit == 0 {
        bm_storage::DocSlice {
            entries: Vec::new(),
            next_cursor: None,
            has_more: false,
        }
    } else {
        match server.store.doc_show_tail(
            workspace,
            &reasoning.branch,
            &reasoning.notes_doc,
            notes_cursor,
            notes_limit,
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        }
    };
    let trace_slice = if trace_limit == 0 {
        bm_storage::DocSlice {
            entries: Vec::new(),
            next_cursor: None,
            has_more: false,
        }
    } else {
        match server.store.doc_show_tail(
            workspace,
            &reasoning.branch,
            &reasoning.trace_doc,
            trace_cursor,
            trace_limit,
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        }
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

    let mut notes_entries = doc_entries_to_json(notes_entries_raw);
    let mut trace_entries = doc_entries_to_json(trace_entries_raw);

    // Step-aware focus: in smart views, minimize trace noise by keeping only entries scoped to
    // the current open step (notes via meta.step; events via task_id+path).
    if matches!(view, ResumeSuperView::Smart | ResumeSuperView::FocusOnly)
        && let (Some(focus_task_id), Some(focus_step_path)) =
            (focus_task_id.as_deref(), focus_step_path.as_deref())
    {
        trace_entries.retain(|entry| {
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
                _ => true,
            }
        });
    }

    // Meaning-mode visibility: drafts are hidden by default outside the focused step.
    if !all_lanes {
        let focus_task_id = focus_task_id.as_deref();
        let focus_step_path = focus_step_path.as_deref();

        let mut keep_note = |entry: &Value| {
            if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                return true;
            }
            let meta = entry.get("meta").unwrap_or(&Value::Null);
            if let (Some(task_id), Some(step_path)) = (focus_task_id, focus_step_path)
                && step_meta_matches(meta, task_id, step_path)
            {
                return true;
            }
            !meta_is_draft(meta)
        };

        notes_entries.retain(&mut keep_note);
        trace_entries.retain(&mut keep_note);
    }

    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();

    let warm_archive = matches!(view, ResumeSuperView::Explore);

    let (cards, edges, cards_next_cursor, cards_has_more) = match view {
        ResumeSuperView::Full => {
            let slice = graph_query_or_empty(
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
                    include_edges: true,
                    edges_limit: 200,
                },
                read_only,
                reasoning_branch_missing,
            )?;
            (
                graph_nodes_to_cards(slice.nodes),
                graph_edges_to_json(slice.edges),
                slice.next_cursor,
                slice.has_more,
            )
        }
        ResumeSuperView::FocusOnly
        | ResumeSuperView::Smart
        | ResumeSuperView::Explore
        | ResumeSuperView::Audit => {
            let slice = fetch_relevance_first_cards(
                server,
                RelevanceFirstCardsArgs {
                    workspace,
                    branch: reasoning.branch.as_str(),
                    graph_doc: reasoning.graph_doc.as_str(),
                    cursor: cards_cursor,
                    cards_limit,
                    focus_step_tag: focus_step_tag.as_deref(),
                    warm_archive,
                    all_lanes,
                    read_only,
                    reasoning_branch_missing,
                },
            )?;
            (slice.cards, slice.edges, slice.next_cursor, slice.has_more)
        }
    };

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
        edges,
        cards_next_cursor,
        cards_has_more,
        cards_limit,
        cards_cursor,
        stats_by_type: by_type,
    })
}

struct RelevanceFirstCardsArgs<'a> {
    workspace: &'a WorkspaceId,
    branch: &'a str,
    graph_doc: &'a str,
    cursor: Option<i64>,
    cards_limit: usize,
    focus_step_tag: Option<&'a str>,
    warm_archive: bool,
    all_lanes: bool,
    read_only: bool,
    reasoning_branch_missing: &'a mut bool,
}

struct RelevanceFirstCardsSlice {
    cards: Vec<Value>,
    edges: Vec<Value>,
    next_cursor: Option<i64>,
    has_more: bool,
}

fn fetch_relevance_first_cards(
    server: &mut McpServer,
    args: RelevanceFirstCardsArgs<'_>,
) -> Result<RelevanceFirstCardsSlice, Value> {
    let RelevanceFirstCardsArgs {
        workspace,
        branch,
        graph_doc,
        cursor,
        cards_limit,
        focus_step_tag,
        warm_archive,
        all_lanes,
        read_only,
        reasoning_branch_missing,
    } = args;
    if cards_limit == 0 {
        return Ok(RelevanceFirstCardsSlice {
            cards: Vec::new(),
            edges: Vec::new(),
            next_cursor: None,
            has_more: false,
        });
    }

    let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
    let types = supported.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    let recent_types = types
        .iter()
        .filter(|t| !matches!(t.as_str(), "note"))
        .cloned()
        .collect::<Vec<_>>();
    let include_drafts = all_lanes;

    // 1) Priority candidates (pins + open frontier).
    let pins_limit = cards_limit.min(8);
    let mut ordered_ids = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();

    let pins_slice = graph_query_or_empty(
        server,
        workspace,
        branch,
        graph_doc,
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(types.clone()),
            status: None,
            tags_any: None,
            tags_all: Some(vec![PIN_TAG.to_string()]),
            text: None,
            cursor: None,
            limit: pins_limit,
            include_edges: false,
            edges_limit: 0,
        },
        read_only,
        reasoning_branch_missing,
    )?;
    for node in &pins_slice.nodes {
        if seen.len() >= cards_limit {
            break;
        }
        if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
            continue;
        }
        if seen.insert(node.id.clone()) {
            ordered_ids.push(node.id.clone());
        }
    }

    // Step-scoped cards (when focus is on a task with a first open step).
    if let Some(step_tag) = focus_step_tag {
        let tags_all = Some(vec![step_tag.to_string()]);

        let step_open_limit = cards_limit.clamp(1, 6);
        let step_open_slice = graph_query_or_empty(
            server,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(types.clone()),
                status: Some("open".to_string()),
                tags_any: None,
                tags_all: tags_all.clone(),
                text: None,
                cursor: None,
                limit: step_open_limit,
                include_edges: false,
                edges_limit: 0,
            },
            read_only,
            reasoning_branch_missing,
        )?;
        for node in &step_open_slice.nodes {
            if seen.len() >= cards_limit {
                break;
            }
            if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                continue;
            }
            if seen.insert(node.id.clone()) {
                ordered_ids.push(node.id.clone());
            }
        }

        if seen.len() < cards_limit {
            let step_any_limit = cards_limit.clamp(1, 4);
            let step_any_slice = graph_query_or_empty(
                server,
                workspace,
                branch,
                graph_doc,
                bm_storage::GraphQueryRequest {
                    ids: None,
                    types: Some(recent_types.clone()),
                    status: None,
                    tags_any: None,
                    tags_all,
                    text: None,
                    cursor: None,
                    limit: step_any_limit,
                    include_edges: false,
                    edges_limit: 0,
                },
                read_only,
                reasoning_branch_missing,
            )?;
            for node in &step_any_slice.nodes {
                if seen.len() >= cards_limit {
                    break;
                }
                if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                    continue;
                }
                if seen.insert(node.id.clone()) {
                    ordered_ids.push(node.id.clone());
                }
            }
        }
    }

    // Open core types (most agents care about the frontier more than the archive).
    let open_each_limit = cards_limit.clamp(1, 6);
    for req in [
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["hypothesis".to_string()]),
            status: Some("open".to_string()),
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: open_each_limit,
            include_edges: false,
            edges_limit: 0,
        },
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["question".to_string()]),
            status: Some("open".to_string()),
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: open_each_limit,
            include_edges: false,
            edges_limit: 0,
        },
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["question".to_string()]),
            status: Some("open".to_string()),
            tags_any: Some(vec!["subgoal".to_string()]),
            tags_all: None,
            text: None,
            cursor: None,
            limit: open_each_limit.min(4),
            include_edges: false,
            edges_limit: 0,
        },
        bm_storage::GraphQueryRequest {
            ids: None,
            types: Some(vec!["test".to_string()]),
            status: Some("open".to_string()),
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: open_each_limit,
            include_edges: false,
            edges_limit: 0,
        },
    ] {
        if seen.len() >= cards_limit {
            break;
        }
        let slice = graph_query_or_empty(
            server,
            workspace,
            branch,
            graph_doc,
            req,
            read_only,
            reasoning_branch_missing,
        )?;
        for node in &slice.nodes {
            if seen.len() >= cards_limit {
                break;
            }
            if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                continue;
            }
            if seen.insert(node.id.clone()) {
                ordered_ids.push(node.id.clone());
            }
        }
    }

    // 2) Fill remaining capacity with recent cards.
    let mut next_cursor = None;
    let mut has_more = false;
    if seen.len() < cards_limit {
        let remaining = cards_limit.saturating_sub(seen.len());
        let padding = remaining.min(8);
        let recent_limit = remaining.saturating_add(padding).max(1);
        let recent_status = if warm_archive {
            None
        } else {
            // Cold archive default: avoid pulling closed/resolved history into relevance-first views.
            Some("open".to_string())
        };
        let recent_slice = graph_query_or_empty(
            server,
            workspace,
            branch,
            graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: Some(recent_types),
                status: recent_status,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor,
                limit: recent_limit,
                include_edges: false,
                edges_limit: 0,
            },
            read_only,
            reasoning_branch_missing,
        )?;

        let mut last_included_seq: Option<i64> = None;
        let mut last_included_index: Option<usize> = None;
        for (idx, node) in recent_slice.nodes.iter().enumerate() {
            if seen.len() >= cards_limit {
                break;
            }
            if !tags_visibility_allows(&node.tags, include_drafts, focus_step_tag) {
                continue;
            }
            if !seen.insert(node.id.clone()) {
                continue;
            }
            ordered_ids.push(node.id.clone());
            last_included_seq = Some(node.last_seq);
            last_included_index = Some(idx);
        }

        // Cursor semantics: only advance past what we actually included (so paging doesn't skip).
        if let Some(seq) = last_included_seq {
            next_cursor = Some(seq);
            let idx = last_included_index.unwrap_or(0);
            has_more = idx + 1 < recent_slice.nodes.len() || recent_slice.has_more;
        } else {
            // No new unique cards were added from the recent slice (likely duplicates). Fall back
            // to the store cursor to allow progress when paging through older history.
            next_cursor = recent_slice.next_cursor;
            has_more = recent_slice.has_more;
        }
    }

    if ordered_ids.is_empty() {
        return Ok(RelevanceFirstCardsSlice {
            cards: Vec::new(),
            edges: Vec::new(),
            next_cursor,
            has_more,
        });
    }

    // 3) Materialize a connected subgraph for the selected ids.
    let graph_slice = graph_query_or_empty(
        server,
        workspace,
        branch,
        graph_doc,
        bm_storage::GraphQueryRequest {
            ids: Some(ordered_ids.clone()),
            types: None,
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: ordered_ids.len().max(1),
            include_edges: true,
            edges_limit: (ordered_ids.len().saturating_mul(6)).min(200),
        },
        read_only,
        reasoning_branch_missing,
    )?;

    let cards = reorder_cards_by_id(graph_nodes_to_cards(graph_slice.nodes), &ordered_ids);
    let edges = graph_edges_to_json(graph_slice.edges);

    Ok(RelevanceFirstCardsSlice {
        cards,
        edges,
        next_cursor,
        has_more,
    })
}

fn reorder_cards_by_id(mut cards: Vec<Value>, ordered_ids: &[String]) -> Vec<Value> {
    let mut by_id = std::collections::BTreeMap::<String, Value>::new();
    for card in cards.drain(..) {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        by_id.insert(id.to_string(), card);
    }

    let mut out = Vec::new();
    for id in ordered_ids {
        if let Some(card) = by_id.remove(id) {
            out.push(card);
        }
    }
    // Any remaining cards (unexpected) are appended deterministically.
    out.extend(by_id.into_values());
    out
}
