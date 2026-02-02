#![forbid(unsafe_code)]

use crate::ops::{Envelope, OpError, OpResponse};
use serde_json::{Value, json};

mod actions;
mod analysis;
mod model;

use model::Entry;

pub(super) fn handle(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let Some(ws) = env.workspace.as_deref() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "workspace is required".to_string(),
                recovery: Some(
                    "Call workspace op=use first (or configure default workspace).".to_string(),
                ),
            },
        );
    };
    let workspace = match crate::WorkspaceId::try_new(ws.to_string()) {
        Ok(v) => v,
        Err(_) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "workspace: expected WorkspaceId".to_string(),
                    recovery: Some("Use workspace like my-workspace".to_string()),
                },
            );
        }
    };

    let Some(args_obj) = env.args.as_object() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={...}".to_string()),
            },
        );
    };

    let limit = args_obj
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(50)
        .clamp(0, 200);
    if limit == 0 {
        return OpResponse::success(
            env.cmd.clone(),
            json!({
                "workspace": workspace.as_str(),
                "branch": super::KB_BRANCH,
                "graph_doc": super::KB_GRAPH_DOC,
                "stats": { "keys_scanned": 0, "has_more": false, "anchors": 0, "keys": 0, "cards_resolved": 0, "issues_total": 0 },
                "issues": [],
                "truncated": false
            }),
        );
    }
    let include_drafts = args_obj
        .get("include_drafts")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let mut anchor_ids = Vec::<String>::new();
    if let Some(anchor_value) = args_obj.get("anchor") {
        match anchor_value {
            Value::String(s) => anchor_ids.push(s.to_string()),
            Value::Array(arr) => {
                for item in arr {
                    let Some(s) = item.as_str() else {
                        return OpResponse::error(
                            env.cmd.clone(),
                            OpError {
                                code: "INVALID_INPUT".to_string(),
                                message: "anchor must be a string or array of strings".to_string(),
                                recovery: Some(
                                    "Use anchor:\"core\" or anchor:[\"core\",\"storage\"]"
                                        .to_string(),
                                ),
                            },
                        );
                    };
                    anchor_ids.push(s.to_string());
                }
            }
            _ => {
                return OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: "anchor must be a string or array of strings".to_string(),
                        recovery: Some(
                            "Use anchor:\"core\" or anchor:[\"core\",\"storage\"]".to_string(),
                        ),
                    },
                );
            }
        }
    }

    let mut normalized_anchors = Vec::<String>::new();
    for raw in anchor_ids {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let candidate = if raw.starts_with(crate::ANCHOR_TAG_PREFIX) {
            raw.to_string()
        } else {
            format!("{}{}", crate::ANCHOR_TAG_PREFIX, raw)
        };
        let Some(normalized) = crate::normalize_anchor_id_tag(&candidate) else {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "anchor must be a valid slug (a:<slug>)".to_string(),
                    recovery: Some("Use anchor like: core | a:core | storage-sqlite".to_string()),
                },
            );
        };
        let resolved = match server.store.anchor_resolve_id(&workspace, &normalized) {
            Ok(Some(v)) => v,
            Ok(None) => normalized,
            Err(bm_storage::StoreError::InvalidInput(msg)) => {
                return OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: msg.to_string(),
                        recovery: None,
                    },
                );
            }
            Err(err) => {
                return OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INTERNAL_ERROR".to_string(),
                        message: format!("store error: {err}"),
                        recovery: None,
                    },
                );
            }
        };
        normalized_anchors.push(resolved);
    }
    normalized_anchors.sort();
    normalized_anchors.dedup();

    let keys = match server.store.knowledge_keys_list_any(
        &workspace,
        bm_storage::KnowledgeKeysListAnyRequest {
            anchor_ids: normalized_anchors,
            limit,
        },
    ) {
        Ok(v) => v,
        Err(bm_storage::StoreError::InvalidInput(msg)) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: msg.to_string(),
                    recovery: None,
                },
            );
        }
        Err(err) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("store error: {err}"),
                    recovery: None,
                },
            );
        }
    };

    let mut anchors_seen = std::collections::BTreeSet::<String>::new();
    let mut keys_seen = std::collections::BTreeSet::<String>::new();
    let mut card_ids = Vec::<String>::new();
    for row in keys.items.iter() {
        anchors_seen.insert(row.anchor_id.clone());
        keys_seen.insert(row.key.clone());
        card_ids.push(row.card_id.clone());
    }
    card_ids.sort();
    card_ids.dedup();

    if card_ids.is_empty() {
        return OpResponse::success(
            env.cmd.clone(),
            json!({
                "workspace": workspace.as_str(),
                "branch": super::KB_BRANCH,
                "graph_doc": super::KB_GRAPH_DOC,
                "stats": {
                    "keys_scanned": keys.items.len(),
                    "has_more": keys.has_more,
                    "anchors": anchors_seen.len(),
                    "keys": keys_seen.len(),
                    "cards_resolved": 0,
                    "issues_total": 0
                },
                "issues": [],
                "truncated": false
            }),
        );
    }

    let slice = match server.store.graph_query(
        &workspace,
        super::KB_BRANCH,
        super::KB_GRAPH_DOC,
        bm_storage::GraphQueryRequest {
            ids: Some(card_ids.clone()),
            types: Some(vec!["knowledge".to_string()]),
            status: None,
            tags_any: None,
            tags_all: None,
            text: None,
            cursor: None,
            limit: card_ids.len().clamp(1, 200),
            include_edges: false,
            edges_limit: 0,
        },
    ) {
        Ok(v) => v,
        Err(bm_storage::StoreError::UnknownBranch) => {
            let mut resp = OpResponse::success(
                env.cmd.clone(),
                json!({
                    "workspace": workspace.as_str(),
                    "branch": super::KB_BRANCH,
                    "graph_doc": super::KB_GRAPH_DOC,
                    "stats": {
                        "keys_scanned": keys.items.len(),
                        "has_more": keys.has_more,
                        "anchors": anchors_seen.len(),
                        "keys": keys_seen.len(),
                        "cards_resolved": 0,
                        "issues_total": 0
                    },
                    "issues": [],
                    "truncated": false
                }),
            );
            resp.warnings.push(crate::warning(
                "KNOWLEDGE_BASE_MISSING",
                "Knowledge base branch is missing",
                "Create knowledge via think.knowledge.upsert (it will auto-create kb/main), then retry lint.",
            ));
            return resp;
        }
        Err(bm_storage::StoreError::InvalidInput(msg)) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: msg.to_string(),
                    recovery: None,
                },
            );
        }
        Err(err) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("store error: {err}"),
                    recovery: None,
                },
            );
        }
    };

    let cards_all = crate::graph_nodes_to_cards(slice.nodes);
    let mut cards_by_id = std::collections::HashMap::<String, Value>::new();
    for card in cards_all {
        let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        cards_by_id.insert(id.to_string(), card);
    }

    let mut visible_card_ids = std::collections::BTreeSet::<String>::new();
    for (id, card) in cards_by_id.iter() {
        if crate::card_value_visibility_allows(card, include_drafts, None) {
            visible_card_ids.insert(id.clone());
        }
    }

    let mut missing_cards = 0usize;
    let mut invisible_cards = 0usize;
    let mut entries = Vec::<Entry>::new();
    for row in keys.items.iter() {
        let Some(card) = cards_by_id.get(row.card_id.as_str()) else {
            missing_cards += 1;
            continue;
        };
        if !visible_card_ids.contains(row.card_id.as_str()) {
            invisible_cards += 1;
            continue;
        }
        let title = card.get("title").and_then(|v| v.as_str());
        let text = card.get("text").and_then(|v| v.as_str());
        let claim = super::normalized_claim(title, text);
        if claim.is_empty() {
            continue;
        }
        let content_hash = super::fnv1a64(&claim);
        entries.push(Entry {
            anchor_id: row.anchor_id.clone(),
            key: row.key.clone(),
            card_id: row.card_id.clone(),
            created_at_ms: row.created_at_ms,
            content_hash,
        });
    }

    let (mut issues, duplicate_groups) = analysis::analyze_duplicate_content_same_anchor(&entries);
    let (same_key_issues, mut cross_groups) =
        analysis::analyze_duplicate_content_same_key_across_anchors(&entries);
    issues.extend(same_key_issues);
    let (cross_issues, cross_groups_multi) =
        analysis::analyze_duplicate_content_across_anchors_multiple_keys(&entries);
    issues.extend(cross_issues);
    cross_groups.extend(cross_groups_multi);
    let overloaded = analysis::analyze_overloaded_keys(&entries);
    issues.extend(overloaded.issues);

    // Deterministic ordering (severity first is handled by stable sort below).
    issues.sort_by(|a, b| {
        let a_sev = a.get("severity").and_then(|v| v.as_str()).unwrap_or("info");
        let b_sev = b.get("severity").and_then(|v| v.as_str()).unwrap_or("info");
        let sev_rank = |sev: &str| if sev == "warning" { 0 } else { 1 };
        sev_rank(a_sev)
            .cmp(&sev_rank(b_sev))
            .then_with(|| {
                let a_code = a.get("code").and_then(|v| v.as_str()).unwrap_or("");
                let b_code = b.get("code").and_then(|v| v.as_str()).unwrap_or("");
                a_code.cmp(b_code)
            })
            .then_with(|| a.to_string().cmp(&b.to_string()))
    });

    let issues_total = issues.len();
    let mut result = json!({
        "workspace": workspace.as_str(),
        "branch": super::KB_BRANCH,
        "graph_doc": super::KB_GRAPH_DOC,
        "stats": {
            "keys_scanned": keys.items.len(),
            "has_more": keys.has_more,
            "anchors": anchors_seen.len(),
            "keys": keys_seen.len(),
            "cards_resolved": visible_card_ids.len(),
            "rows_missing_cards": missing_cards,
            "rows_invisible_cards": invisible_cards,
            "issues_total": issues_total
        },
        "issues": issues,
        "truncated": false
    });

    // Budget discipline: keep result bounded (cards are not embedded, only structured evidence).
    let max_chars = args_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let mut warnings = Vec::new();
    if let Some(max_chars) = max_chars {
        let (max_chars, clamped) = crate::clamp_budget_max(max_chars);
        let (_used, truncated) = crate::enforce_graph_list_budget(&mut result, "issues", max_chars);
        crate::set_truncated_flag(&mut result, truncated);
        warnings = crate::budget_warnings(truncated, false, clamped);
    }

    let mut resp = OpResponse::success(env.cmd.clone(), result);
    resp.warnings.extend(warnings);

    actions::push_duplicate_group_actions(
        &mut resp,
        workspace.as_str(),
        super::KB_BRANCH,
        super::KB_GRAPH_DOC,
        duplicate_groups,
    );
    actions::push_cross_duplicate_group_actions(
        &mut resp,
        workspace.as_str(),
        super::KB_BRANCH,
        super::KB_GRAPH_DOC,
        cross_groups,
    );
    actions::push_overloaded_outliers_actions(
        &mut resp,
        workspace.as_str(),
        super::KB_BRANCH,
        super::KB_GRAPH_DOC,
        overloaded.outliers,
    );
    actions::push_overloaded_key_open_actions(
        &mut resp,
        workspace.as_str(),
        overloaded.overloaded_keys,
    );

    resp
}
