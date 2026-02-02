#![forbid(unsafe_code)]

use crate::ops::{Action, ActionPriority, Envelope, OpError, OpResponse, ToolName};
use serde_json::{Value, json};

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
        .unwrap_or(false);

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

    #[derive(Clone, Debug)]
    struct Entry {
        anchor_id: String,
        key: String,
        card_id: String,
        created_at_ms: i64,
        content_hash: u64,
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

    #[derive(Clone, Debug)]
    struct DuplicateGroup {
        anchor_id: String,
        content_hash: u64,
        keys: Vec<String>,
        card_ids: Vec<String>,
        recommended_key: String,
    }

    let mut issues = Vec::<Value>::new();
    let mut duplicate_groups = Vec::<DuplicateGroup>::new();

    // 1) High-confidence duplicates: same normalized content, same anchor, different keys.
    let mut by_anchor_hash = std::collections::BTreeMap::<(String, u64), Vec<Entry>>::new();
    for entry in entries.iter().cloned() {
        by_anchor_hash
            .entry((entry.anchor_id.clone(), entry.content_hash))
            .or_default()
            .push(entry);
    }
    for ((anchor_id, content_hash), mut group) in by_anchor_hash {
        group.sort_by(|a, b| {
            a.created_at_ms
                .cmp(&b.created_at_ms)
                .then_with(|| a.key.cmp(&b.key))
                .then_with(|| a.card_id.cmp(&b.card_id))
        });
        let mut keys = group
            .iter()
            .map(|e| e.key.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if keys.len() < 2 {
            continue;
        }
        keys.sort();
        let mut card_ids = group
            .iter()
            .map(|e| e.card_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        card_ids.sort();

        let recommended = group.first().expect("non-empty group");
        let recommended_key = recommended.key.clone();
        let recommended_card_id = recommended.card_id.clone();

        issues.push(json!({
            "severity": "warning",
            "code": "KNOWLEDGE_DUPLICATE_CONTENT_SAME_ANCHOR",
            "message": format!(
                "Duplicate knowledge content under one anchor: {} has multiple keys with identical content.",
                anchor_id
            ),
            "evidence": {
                "anchor_id": anchor_id,
                "keys": keys,
                "card_ids": card_ids,
                "content_hash": format!("{content_hash:016x}"),
                "recommended_key": recommended_key,
                "recommended_card_id": recommended_card_id
            }
        }));

        duplicate_groups.push(DuplicateGroup {
            anchor_id,
            content_hash,
            keys,
            card_ids,
            recommended_key,
        });
    }

    // 2) Duplicate content for the same key across anchors (often “shared knowledge”).
    let mut by_key_hash = std::collections::BTreeMap::<(String, u64), Vec<Entry>>::new();
    for entry in entries.iter().cloned() {
        by_key_hash
            .entry((entry.key.clone(), entry.content_hash))
            .or_default()
            .push(entry);
    }
    for ((key, content_hash), group) in by_key_hash {
        let anchors = group
            .iter()
            .map(|e| e.anchor_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let anchor_count = anchors.len();
        if anchor_count < 2 {
            continue;
        }
        let card_ids = group
            .iter()
            .map(|e| e.card_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let anchors_sample = anchors.iter().take(12).cloned().collect::<Vec<_>>();
        let card_ids_sample = card_ids.iter().take(12).cloned().collect::<Vec<_>>();
        issues.push(json!({
            "severity": "info",
            "code": "KNOWLEDGE_DUPLICATE_CONTENT_SAME_KEY_ACROSS_ANCHORS",
            "message": format!(
                "Key is reused across anchors with identical content: k:{} appears in {} anchors.",
                key,
                anchor_count
            ),
            "evidence": {
                "key": key,
                "anchor_count": anchor_count,
                "anchors_sample": anchors_sample,
                "card_ids_sample": card_ids_sample,
                "content_hash": format!("{content_hash:016x}")
            }
        }));
    }

    // 3) Potentially too-generic keys: reused across anchors with multiple distinct content variants.
    let mut key_stats = std::collections::BTreeMap::<
        String,
        (
            std::collections::BTreeSet<String>,
            std::collections::BTreeSet<u64>,
        ),
    >::new();
    for entry in entries.iter() {
        let slot = key_stats.entry(entry.key.clone()).or_insert_with(|| {
            (
                std::collections::BTreeSet::new(),
                std::collections::BTreeSet::new(),
            )
        });
        slot.0.insert(entry.anchor_id.clone());
        slot.1.insert(entry.content_hash);
    }
    for (key, (anchors, variants)) in key_stats.iter() {
        let anchor_count = anchors.len();
        let variant_count = variants.len();
        if anchor_count < 2 || variant_count < 2 {
            continue;
        }
        let anchors_sample = anchors.iter().take(12).cloned().collect::<Vec<_>>();
        issues.push(json!({
            "severity": "info",
            "code": "KNOWLEDGE_KEY_OVERLOADED_ACROSS_ANCHORS",
            "message": format!(
                "Key may be overloaded (reused with different content): k:{} has {} anchors and {} variants.",
                key,
                anchor_count,
                variant_count
            ),
            "evidence": {
                "key": key,
                "anchor_count": anchor_count,
                "variant_count": variant_count,
                "anchors_sample": anchors_sample
            }
        }));
    }

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

    // Actions: open helpers for the top duplicate groups (bounded).
    duplicate_groups.sort_by(|a, b| {
        b.keys
            .len()
            .cmp(&a.keys.len())
            .then_with(|| a.anchor_id.cmp(&b.anchor_id))
            .then_with(|| a.content_hash.cmp(&b.content_hash))
    });
    for group in duplicate_groups.into_iter().take(5) {
        let ids_limit = group.card_ids.len().clamp(1, 50);
        resp.actions.push(Action {
            action_id: format!(
                "knowledge.lint.duplicate.open::{}::{:016x}",
                group.anchor_id, group.content_hash
            ),
            priority: ActionPriority::High,
            tool: ToolName::GraphOps.as_str().to_string(),
            args: json!({
                "op": "call",
                "cmd": "graph.query",
                "args": {
                    "workspace": workspace.as_str(),
                    "branch": super::KB_BRANCH,
                    "doc": super::KB_GRAPH_DOC,
                    "ids": group.card_ids,
                    "types": ["knowledge"],
                    "limit": ids_limit,
                    "include_edges": false,
                    "edges_limit": 0
                },
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: format!(
                "Открыть дубль-набор для консолидации: {} → k:{} ({} keys).",
                group.anchor_id,
                group.recommended_key,
                group.keys.len()
            ),
            risk: "Низкий".to_string(),
        });
    }

    // Actions: open top potentially-overloaded keys (info only, bounded).
    let mut overloaded = key_stats
        .into_iter()
        .filter_map(|(key, (anchors, variants))| {
            if anchors.len() < 2 || variants.len() < 2 {
                return None;
            }
            Some((anchors.len(), variants.len(), key))
        })
        .collect::<Vec<_>>();
    overloaded.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    for (_anchor_count, _variants, key) in overloaded.into_iter().take(3) {
        resp.actions.push(Action {
            action_id: format!("knowledge.lint.key.open::{key}"),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "op": "call",
                "cmd": "think.knowledge.query",
                "args": { "key": key, "limit": 20 },
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: format!(
                "Открыть k:{key} across anchors (проверить перегруженность/консолидацию)."
            ),
            risk: "Низкий".to_string(),
        });
    }

    resp
}
