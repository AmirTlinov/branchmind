#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

use super::ANCHORS_GRAPH_DOC;

fn is_anchor_tag_any(tag: &str, anchor_ids: &[String]) -> bool {
    let tag = tag.trim();
    if tag.is_empty() {
        return false;
    }
    anchor_ids
        .iter()
        .any(|id| tag.eq_ignore_ascii_case(id.as_str()))
}

fn card_type(card: &Value) -> &str {
    card.get("type").and_then(|v| v.as_str()).unwrap_or("note")
}

fn card_ts(card: &Value) -> i64 {
    card.get("last_ts_ms").and_then(|v| v.as_i64()).unwrap_or(0)
}

fn card_id(card: &Value) -> &str {
    card.get("id").and_then(|v| v.as_str()).unwrap_or("")
}

fn card_has_tag(card: &Value, tag: &str) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };
    tags.iter().any(|t| {
        t.as_str()
            .map(|s| s.eq_ignore_ascii_case(tag))
            .unwrap_or(false)
    })
}

fn is_canon_by_type(card: &Value) -> bool {
    matches!(card_type(card), "decision" | "evidence" | "test")
}

fn is_canon_by_visibility(card: &Value) -> bool {
    card_has_tag(card, VIS_TAG_CANON)
}

fn is_draft_by_visibility(card: &Value) -> bool {
    let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
        return false;
    };

    let mut has_canon = false;
    let mut explicit_draft = false;
    let mut legacy_lane = false;

    for tag in tags {
        let Some(tag) = tag.as_str() else {
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        if tag == VIS_TAG_CANON {
            has_canon = true;
        }
        if tag == VIS_TAG_DRAFT {
            explicit_draft = true;
        }
        if tag.starts_with(LANE_TAG_AGENT_PREFIX) {
            legacy_lane = true;
        }
    }

    explicit_draft || (legacy_lane && !has_canon)
}

fn type_priority(t: &str) -> u8 {
    match t {
        "decision" => 0,
        "evidence" => 1,
        "test" => 2,
        "hypothesis" => 3,
        "question" => 4,
        "update" => 5,
        "note" => 6,
        "frame" => 7,
        _ => 9,
    }
}

impl McpServer {
    pub(crate) fn tool_branchmind_anchor_snapshot(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let anchor_id = match require_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let include_drafts = match optional_bool(args_obj, "include_drafts") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let tasks_limit = match optional_usize(args_obj, "tasks_limit") {
            Ok(v) => v.unwrap_or(10).clamp(0, 50),
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20).clamp(1, 50),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut warnings = Vec::<Value>::new();

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let resolved = match self.store.anchor_resolve_id(&workspace, &anchor_id) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let Some(resolved_anchor_id) = resolved else {
            return ai_error_with(
                "UNKNOWN_ID",
                "Unknown anchor id",
                Some("Create it via macro_anchor_note (provide title+kind) and retry."),
                vec![suggest_call(
                    "macro_anchor_note",
                    "Create anchor + attach a first canonical note.",
                    "high",
                    json!({
                        "workspace": workspace.as_str(),
                        "anchor": anchor_id,
                        "title": "Core",
                        "kind": "component",
                        "content": "Anchor registry note (what it is, invariants, risks).",
                        "visibility": "canon",
                        "pin": true
                    }),
                )],
            );
        };
        if !anchor_id
            .trim()
            .eq_ignore_ascii_case(resolved_anchor_id.as_str())
        {
            warnings.push(warning(
                "ANCHOR_ALIAS_RESOLVED",
                "anchor id resolved via alias mapping",
                "Use the canonical anchor id for new work; history is included automatically.",
            ));
        }

        let anchor = match self.store.anchor_get(
            &workspace,
            bm_storage::AnchorGetRequest {
                id: resolved_anchor_id,
            },
        ) {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("STORE_ERROR", "anchor_resolve returned missing anchor"),
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let checkout = match require_checkout_branch(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let query_limit = if include_drafts {
            limit
        } else {
            limit.saturating_mul(4).clamp(1, 200)
        };

        // Primary source: anchor_links index (cross-graph).
        // Fallback: anchors-graph tag scan (backwards compatibility).
        let mut cards = Vec::<Value>::new();

        let mut anchor_ids = vec![anchor.id.clone()];
        anchor_ids.extend(anchor.aliases.clone());

        // Task lens: derive recent tasks touching this anchor from anchor_links.
        let (tasks, tasks_has_more) = if tasks_limit == 0 {
            (Vec::<Value>::new(), false)
        } else {
            let tasks = match self.store.anchor_tasks_list_any(
                &workspace,
                bm_storage::AnchorTasksListAnyRequest {
                    anchor_ids: anchor_ids.clone(),
                    limit: tasks_limit,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let has_more = tasks.has_more;
            let items = tasks
                .tasks
                .into_iter()
                .map(|t| {
                    json!({
                        "task": t.task_id,
                        "title": t.title,
                        "status": t.status,
                        "last_ts_ms": t.last_ts_ms
                    })
                })
                .collect::<Vec<_>>();
            (items, has_more)
        };
        let links = match self.store.anchor_links_list_any(
            &workspace,
            bm_storage::AnchorLinksListAnyRequest {
                anchor_ids: anchor_ids.clone(),
                limit: query_limit,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let links_has_more = links.has_more;
        let links_total = links.links.len();

        if !links.links.is_empty() {
            #[derive(Clone, Debug)]
            struct GroupKey {
                branch: String,
                graph_doc: String,
            }

            let mut groups =
                std::collections::BTreeMap::<(String, String), (i64, Vec<String>)>::new();
            for link in &links.links {
                let key = (link.branch.clone(), link.graph_doc.clone());
                let entry = groups.entry(key).or_insert((link.last_ts_ms, Vec::new()));
                // Preserve max ts for ordering across groups.
                entry.0 = entry.0.max(link.last_ts_ms);
                entry.1.push(link.card_id.clone());
            }

            let mut group_list = groups
                .into_iter()
                .map(|((branch, graph_doc), (max_ts_ms, ids))| {
                    (max_ts_ms, GroupKey { branch, graph_doc }, ids)
                })
                .collect::<Vec<_>>();
            group_list.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.branch.cmp(&b.1.branch))
                    .then_with(|| a.1.graph_doc.cmp(&b.1.graph_doc))
            });

            let mut seen = std::collections::BTreeSet::<String>::new();
            for (_max_ts, key, ids) in group_list {
                if cards.len() >= query_limit {
                    break;
                }

                // `graph_query` is bounded; ids are already capped by query_limit.
                let slice = match self.store.graph_query(
                    &workspace,
                    &key.branch,
                    &key.graph_doc,
                    bm_storage::GraphQueryRequest {
                        ids: Some(ids),
                        types: Some(
                            bm_core::think::SUPPORTED_THINK_CARD_TYPES
                                .iter()
                                .map(|v| v.to_string())
                                .collect(),
                        ),
                        status: None,
                        tags_any: None,
                        tags_all: None,
                        text: None,
                        cursor: None,
                        limit: query_limit,
                        include_edges: false,
                        edges_limit: 0,
                    },
                ) {
                    Ok(v) => v,
                    Err(StoreError::UnknownBranch) => continue,
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };

                for card in graph_nodes_to_cards(slice.nodes) {
                    let id = card_id(&card).to_string();
                    if id.is_empty() {
                        continue;
                    }
                    if seen.insert(id) {
                        cards.push(card);
                    }
                }
            }
        }

        if cards.is_empty() {
            // Backwards compatibility: older workspaces may have anchor-tagged cards without an index.
            let slice = match self.store.graph_query(
                &workspace,
                &checkout,
                ANCHORS_GRAPH_DOC,
                bm_storage::GraphQueryRequest {
                    ids: None,
                    types: Some(
                        bm_core::think::SUPPORTED_THINK_CARD_TYPES
                            .iter()
                            .map(|v| v.to_string())
                            .collect(),
                    ),
                    status: None,
                    tags_any: None,
                    tags_all: Some(vec![anchor.id.clone()]),
                    text: None,
                    cursor: None,
                    limit: query_limit,
                    include_edges: false,
                    edges_limit: 0,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::UnknownBranch) => {
                    // If the checkout is missing, it indicates an inconsistent workspace state.
                    return ai_error("STORE_ERROR", "checkout branch does not exist");
                }
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            cards = graph_nodes_to_cards(slice.nodes);
        }

        // Ensure the returned slice is actually anchor-scoped (regardless of how it was collected).
        cards.retain(|card| {
            let Some(tags) = card.get("tags").and_then(|v| v.as_array()) else {
                return false;
            };
            tags.iter()
                .filter_map(|t| t.as_str())
                .any(|t| is_anchor_tag_any(t, &anchor_ids))
        });

        if !include_drafts {
            cards.retain(|card| {
                if card_has_tag(card, PIN_TAG) {
                    return true;
                }
                if is_draft_by_visibility(card) {
                    return false;
                }
                is_canon_by_visibility(card) || is_canon_by_type(card)
            });
        }

        cards.sort_by(|a, b| {
            let a_pinned = card_has_tag(a, PIN_TAG);
            let b_pinned = card_has_tag(b, PIN_TAG);
            b_pinned
                .cmp(&a_pinned)
                .then_with(|| type_priority(card_type(a)).cmp(&type_priority(card_type(b))))
                .then_with(|| card_ts(b).cmp(&card_ts(a)))
                .then_with(|| card_id(a).cmp(card_id(b)))
        });
        cards.truncate(limit);

        let mut result = json!({
            "workspace": workspace.as_str(),
            "anchor": {
                "id": anchor.id,
                "title": anchor.title,
                "kind": anchor.kind,
                "status": anchor.status,
                "description": anchor.description,
                "refs": anchor.refs,
                "aliases": anchor.aliases,
                "parent_id": anchor.parent_id,
                "depends_on": anchor.depends_on,
                "created_at_ms": anchor.created_at_ms,
                "updated_at_ms": anchor.updated_at_ms
            },
            "scope": {
                "branch": checkout,
                "graph_doc": ANCHORS_GRAPH_DOC,
                "mode": "links+registry"
            },
            "stats": {
                "links_count": links_total,
                "links_has_more": links_has_more,
                "tasks_count": tasks.len(),
                "tasks_has_more": tasks_has_more
            },
            "tasks": tasks,
            "cards": cards,
            "count": cards.len(),
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, truncated_tasks) = enforce_graph_list_budget(&mut result, "tasks", limit);
            let (_used, truncated_cards) = enforce_graph_list_budget(&mut result, "cards", limit);
            let truncated_any = truncated_tasks || truncated_cards;
            if let Some(obj) = result.as_object_mut() {
                if let Some(cards) = obj.get("cards").and_then(|v| v.as_array()) {
                    obj.insert(
                        "count".to_string(),
                        Value::Number(serde_json::Number::from(cards.len() as u64)),
                    );
                }
                let tasks_len = obj
                    .get("tasks")
                    .and_then(|v| v.as_array())
                    .map(|tasks| tasks.len());
                if let Some(stats) = obj.get_mut("stats").and_then(|v| v.as_object_mut()) {
                    if let Some(tasks_len) = tasks_len {
                        stats.insert(
                            "tasks_count".to_string(),
                            Value::Number(serde_json::Number::from(tasks_len as u64)),
                        );
                    }
                    if truncated_tasks {
                        // If tasks were dropped due to max_chars, we *do* have more to show.
                        stats.insert("tasks_has_more".to_string(), Value::Bool(true));
                    }
                }
            }
            set_truncated_flag(&mut result, truncated_any);
            let _ = attach_budget(&mut result, limit, truncated_any);
            let mut out_warnings = warnings;
            out_warnings.extend(budget_warnings(truncated_any, false, clamped));
            if out_warnings.is_empty() {
                ai_ok("anchor_snapshot", result)
            } else {
                ai_ok_with_warnings("anchor_snapshot", result, out_warnings, Vec::new())
            }
        } else if warnings.is_empty() {
            ai_ok("anchor_snapshot", result)
        } else {
            ai_ok_with_warnings("anchor_snapshot", result, warnings, Vec::new())
        }
    }
}
