#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, Envelope, OpError, OpResponse, ToolName, name_to_cmd_segments,
};
use serde_json::{Value, json};
use std::fmt::Write as _;

pub(crate) const KB_BRANCH: &str = "kb/main";
pub(crate) const KB_GRAPH_DOC: &str = "kb-graph";
pub(crate) const KB_TRACE_DOC: &str = "kb-trace";

pub(crate) fn handle_knowledge_upsert(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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
                recovery: None,
            },
        );
    };
    let card_value = args_obj.get("card").cloned().unwrap_or(Value::Null);
    let parsed = match crate::parse_think_card(&workspace, card_value.clone()) {
        Ok(v) => v,
        Err(resp) => {
            return crate::ops::handler_to_op_response(&env.cmd, Some(workspace.as_str()), resp);
        }
    };

    let anchor = args_obj
        .get("anchor")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let mut key = args_obj
        .get("key")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let key_mode = args_obj
        .get("key_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("explicit")
        .trim()
        .to_ascii_lowercase();
    if !matches!(key_mode.as_str(), "explicit" | "auto") {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "key_mode must be explicit|auto".to_string(),
                recovery: Some("Use key_mode=\"explicit\" or key_mode=\"auto\".".to_string()),
            },
        );
    }
    let lint_mode = args_obj
        .get("lint_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("manual")
        .trim()
        .to_ascii_lowercase();
    if !matches!(lint_mode.as_str(), "manual" | "auto") {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "lint_mode must be manual|auto".to_string(),
                recovery: Some("Use lint_mode=\"manual\" or lint_mode=\"auto\".".to_string()),
            },
        );
    }

    if key.is_none() && key_mode == "auto" {
        if anchor.as_deref().is_none() {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "key_mode=auto requires anchor".to_string(),
                    recovery: Some(
                        "Provide args={anchor:\"...\", key_mode:\"auto\", card:{...}}".to_string(),
                    ),
                },
            );
        }
        let source = parsed.title.as_deref().or(parsed.text.as_deref());
        let Some(source) = source else {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "auto key requires card title or text".to_string(),
                    recovery: Some(
                        "Provide card={title:\"...\"} or card={text:\"...\"}.".to_string(),
                    ),
                },
            );
        };
        let Some(slug) = crate::slugify_key(source) else {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "auto key could not be derived".to_string(),
                    recovery: Some(
                        "Provide key explicitly or use a more specific title.".to_string(),
                    ),
                },
            );
        };
        key = Some(slug);
    }

    let mut resolved_anchor_tag: Option<String> = None;
    let mut resolved_key_tag: Option<String> = None;

    let card_id = if let Some(key) = key.as_deref() {
        let Some(anchor) = anchor.as_deref() else {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "key requires anchor".to_string(),
                    recovery: Some(
                        "Provide args={anchor:\"core\", key:\"...\", card:{...}}".to_string(),
                    ),
                },
            );
        };

        let anchor_tag = {
            let candidate = if anchor.trim().starts_with(crate::ANCHOR_TAG_PREFIX) {
                anchor.trim().to_string()
            } else {
                format!("{}{}", crate::ANCHOR_TAG_PREFIX, anchor.trim())
            };
            crate::normalize_anchor_id_tag(&candidate).ok_or_else(|| OpError {
                code: "INVALID_INPUT".to_string(),
                message: "anchor must be a valid slug (a:<slug>)".to_string(),
                recovery: Some("Use anchor like: core | a:core | storage-sqlite".to_string()),
            })
        };
        let anchor_tag = match anchor_tag {
            Ok(v) => v,
            Err(e) => return OpResponse::error(env.cmd.clone(), e),
        };
        let candidate = if key.trim().starts_with(crate::KEY_TAG_PREFIX) {
            key.trim().to_string()
        } else {
            format!("{}{}", crate::KEY_TAG_PREFIX, key.trim())
        };
        let key_tag = match crate::normalize_key_id_tag(&candidate) {
            Some(v) => v,
            None => {
                let suggested_key = crate::slugify_key(key).unwrap_or_else(|| "example-key".into());
                let mut recovery = String::new();
                let _ = write!(
                    &mut recovery,
                    "Use key like: determinism | k:determinism | storage-locking. Suggested key: {suggested_key}"
                );

                let mut resp = OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: "key must be a valid slug (k:<slug>)".to_string(),
                        recovery: Some(recovery),
                    },
                );

                // Make the "right thing" copy/pasteable (only on error, to avoid noise).
                let title = parsed
                    .title
                    .as_deref()
                    .or(parsed.text.as_deref())
                    .unwrap_or("Knowledge card")
                    .to_string();
                resp.actions.push(Action {
                    action_id: format!("recover.key.suggest::{anchor}"),
                    priority: ActionPriority::High,
                    tool: ToolName::ThinkOps.as_str().to_string(),
                    args: json!({
                        "workspace": env.workspace,
                        "op": "call",
                        "cmd": "think.knowledge.key.suggest",
                        "args": {
                            "anchor": anchor,
                            "title": title
                        },
                        "budget_profile": "portal",
                        "portal_view": "compact"
                    }),
                    why: "Получить детерминированный suggested_key (anchor-first) вместо ручного подбора.".to_string(),
                    risk: "Низкий".to_string(),
                });

                return resp;
            }
        };

        resolved_anchor_tag = Some(anchor_tag.clone());
        resolved_key_tag = Some(key_tag.clone());

        let claim = normalized_claim(parsed.title.as_deref(), parsed.text.as_deref());
        if claim.is_empty() {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "card must include at least title or text".to_string(),
                    recovery: Some("Provide card={title:\"...\", text:\"...\"}".to_string()),
                },
            );
        }

        // Versioned, deterministic card identity:
        // - stable for the same (anchor,key,claim)
        // - new card_id for edits, so storage invariants (no payload mismatch for same id) hold
        let hash = fnv1a64(&format!("{anchor_tag}\n{key_tag}\n{claim}"));
        format!("CARD-KN-{hash:016x}")
    } else {
        let claim = normalized_claim(parsed.title.as_deref(), parsed.text.as_deref());
        if claim.is_empty() {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INVALID_INPUT".to_string(),
                    message: "card must include at least title or text".to_string(),
                    recovery: Some("Provide card={title:\"...\", text:\"...\"}".to_string()),
                },
            );
        }

        let hash = fnv1a64(&claim);
        format!("CARD-KN-{hash:016x}")
    };

    // Ensure the knowledge base branch exists (write path may auto-create it).
    match server.store.branch_create(
        &workspace,
        KB_BRANCH,
        Some(server.store.default_branch_name()),
    ) {
        Ok(_) => {}
        Err(bm_storage::StoreError::BranchAlreadyExists) => {}
        Err(err) => {
            return OpResponse::error(
                env.cmd.clone(),
                OpError {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("store error: {err}"),
                    recovery: Some(
                        "Retry after initializing the workspace checkout branch.".to_string(),
                    ),
                },
            );
        }
    };

    let mut forwarded = args_obj.clone();
    forwarded.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );
    forwarded.insert("branch".to_string(), Value::String(KB_BRANCH.to_string()));
    forwarded.insert(
        "graph_doc".to_string(),
        Value::String(KB_GRAPH_DOC.to_string()),
    );
    forwarded.insert(
        "trace_doc".to_string(),
        Value::String(KB_TRACE_DOC.to_string()),
    );
    forwarded.insert(
        "card".to_string(),
        upsert_card_id_into_value(card_value, &card_id),
    );

    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_add_knowledge", Value::Object(forwarded))
            .unwrap_or_else(|| {
                crate::ai_error("INTERNAL_ERROR", "think_add_knowledge dispatch failed")
            });

    let mut response =
        crate::ops::handler_to_op_response(&env.cmd, Some(workspace.as_str()), handler_resp);

    if lint_mode == "auto" && !server.knowledge_autolint_enabled {
        response.warnings.push(crate::warning(
            "FEATURE_DISABLED",
            "knowledge_autolint is disabled",
            "Run think.knowledge.lint manually or enable the feature flag (--knowledge-autolint).",
        ));
    } else if lint_mode == "auto"
        && let Some(key_tag) = resolved_key_tag.as_deref()
    {
        let key_slug = key_tag
            .trim_start_matches(crate::KEY_TAG_PREFIX)
            .to_string();
        let anchor_ids = resolved_anchor_tag
            .as_ref()
            .map(|a| vec![a.clone()])
            .unwrap_or_default();
        match server.store.knowledge_keys_list_by_key(
            &workspace,
            bm_storage::KnowledgeKeysListByKeyRequest {
                key: key_slug,
                anchor_ids,
                limit: 10,
            },
        ) {
            Ok(list) => {
                let mut collisions = Vec::<String>::new();
                for row in list.items {
                    if resolved_anchor_tag
                        .as_ref()
                        .is_some_and(|a| a == &row.anchor_id)
                    {
                        continue;
                    }
                    collisions.push(row.anchor_id);
                }
                if !collisions.is_empty() {
                    collisions.sort();
                    collisions.dedup();
                    response.warnings.push(crate::warning(
                        "KNOWLEDGE_KEY_COLLISION",
                        &format!(
                            "key already used in other anchors: {}",
                            collisions.join(", ")
                        ),
                        "Consider a more specific key or reuse the existing anchor/key pairing.",
                    ));
                }
            }
            Err(_) => {
                response.warnings.push(crate::warning(
                    "KNOWLEDGE_LINT_FAILED",
                    "auto lint failed to read knowledge key index",
                    "Retry with lint_mode=manual or run think.knowledge.lint separately.",
                ));
            }
        }
    }

    response
}

pub(crate) fn handle_knowledge_key_suggest(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
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
                recovery: None,
            },
        );
    };

    let anchor_raw = args_obj.get("anchor").and_then(|v| v.as_str());
    let title = args_obj
        .get("title")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let text = args_obj
        .get("text")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let limit = args_obj
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(6)
        .clamp(1, 20);

    let card_value = args_obj.get("card").cloned().unwrap_or(Value::Null);
    let parsed = if card_value.is_null() {
        None
    } else {
        match crate::parse_think_card(&workspace, card_value) {
            Ok(v) => Some(v),
            Err(resp) => {
                return crate::ops::handler_to_op_response(
                    &env.cmd,
                    Some(workspace.as_str()),
                    resp,
                );
            }
        }
    };

    let source = title
        .as_deref()
        .or(text.as_deref())
        .or_else(|| parsed.as_ref().and_then(|p| p.title.as_deref()))
        .or_else(|| parsed.as_ref().and_then(|p| p.text.as_deref()));
    let Some(source) = source else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "title/text is required to suggest a key".to_string(),
                recovery: Some("Provide title or text (or card with title/text).".to_string()),
            },
        );
    };

    let Some(slug) = crate::slugify_key(source) else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "could not derive key from input".to_string(),
                recovery: Some("Provide a more specific title or an explicit key.".to_string()),
            },
        );
    };

    let mut anchor_ids = Vec::<String>::new();
    let mut anchor_tag: Option<String> = None;
    if let Some(anchor_raw) = anchor_raw {
        let candidate = if anchor_raw.trim().starts_with(crate::ANCHOR_TAG_PREFIX) {
            anchor_raw.trim().to_string()
        } else {
            format!("{}{}", crate::ANCHOR_TAG_PREFIX, anchor_raw.trim())
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
            Ok(None) => normalized.clone(),
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
        anchor_tag = Some(resolved.clone());
        anchor_ids.push(resolved);
    }

    let list = match server.store.knowledge_keys_list_by_key(
        &workspace,
        bm_storage::KnowledgeKeysListByKeyRequest {
            key: slug.clone(),
            anchor_ids,
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

    let collisions = list
        .items
        .into_iter()
        .map(|row| {
            json!({
                "anchor": row.anchor_id,
                "key": row.key,
                "card_id": row.card_id
            })
        })
        .collect::<Vec<_>>();

    OpResponse::success(
        env.cmd.clone(),
        json!({
            "workspace": workspace.as_str(),
            "anchor": anchor_tag,
            "suggested_key": slug,
            "key_tag": format!("{}{}", crate::KEY_TAG_PREFIX, slugify_key_for_tag(&slug)),
            "collisions": collisions,
            "has_more": list.has_more
        }),
    )
}

pub(crate) fn slugify_key_for_tag(slug: &str) -> String {
    slug.trim().to_string()
}

pub(crate) fn parse_doc_entry_ref(raw: &str) -> Option<(String, i64)> {
    let raw = raw.trim();
    let (doc, seq_str) = raw.rsplit_once('@')?;
    let doc = doc.trim();
    let seq_str = seq_str.trim();
    if doc.is_empty() || seq_str.is_empty() {
        return None;
    }
    let seq = seq_str.parse::<i64>().ok()?;
    if seq < 0 {
        return None;
    }
    Some((doc.to_string(), seq))
}

pub(crate) fn handle_knowledge_query(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

    let Some(mut args_obj) = env.args.as_object().cloned() else {
        return OpResponse::error(
            env.cmd.clone(),
            OpError {
                code: "INVALID_INPUT".to_string(),
                message: "args must be an object".to_string(),
                recovery: Some("Provide args={...}".to_string()),
            },
        );
    };

    args_obj
        .entry("limit".to_string())
        .or_insert_with(|| json!(12));

    // v1 UX defaults to the knowledge base scope *only when it exists* (back-compat: before
    // kb/main is created, keep legacy behavior by reading from the default graph scope).
    let kb_exists = server
        .store
        .branch_list(&workspace, 501)
        .ok()
        .is_some_and(|branches| branches.iter().any(|b| b.name == KB_BRANCH));
    let desired_branch = args_obj
        .get("ref")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .or_else(|| kb_exists.then_some(KB_BRANCH.to_string()));
    let desired_graph_doc = args_obj
        .get("graph_doc")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .or_else(|| kb_exists.then_some(KB_GRAPH_DOC.to_string()));
    let use_kb_scope = kb_exists
        && desired_branch.as_deref() == Some(KB_BRANCH)
        && desired_graph_doc.as_deref() == Some(KB_GRAPH_DOC);

    let limit = args_obj
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(12)
        .clamp(1, 200);
    let include_history = args_obj
        .get("include_history")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let include_drafts = args_obj
        .get("include_drafts")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let text = args_obj
        .get("text")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let max_chars = args_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    // Product UX: when a single key is requested (and history is not), use the knowledge key index
    // to resolve the *latest* card_id per (anchor,key). This avoids returning historical duplicates.
    if let Some(key_raw) = args_obj.get("key").and_then(|v| v.as_str()) {
        let key_raw = key_raw.trim();
        if use_kb_scope && !key_raw.is_empty() && !include_history {
            let candidate = if key_raw.starts_with(crate::KEY_TAG_PREFIX) {
                key_raw.to_string()
            } else {
                format!("{}{}", crate::KEY_TAG_PREFIX, key_raw)
            };
            let Some(key_tag) = crate::normalize_key_id_tag(&candidate) else {
                return OpResponse::error(
                    env.cmd.clone(),
                    OpError {
                        code: "INVALID_INPUT".to_string(),
                        message: "key must be a valid slug (k:<slug>)".to_string(),
                        recovery: Some(
                            "Use key like: determinism | k:determinism | storage-locking"
                                .to_string(),
                        ),
                    },
                );
            };
            let key_slug = key_tag
                .strip_prefix(crate::KEY_TAG_PREFIX)
                .unwrap_or(key_tag.as_str())
                .to_string();

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
                                        message: "anchor must be a string or array of strings"
                                            .to_string(),
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
                                    "Use anchor:\"core\" or anchor:[\"core\",\"storage\"]"
                                        .to_string(),
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
                            recovery: Some(
                                "Use anchor like: core | a:core | storage-sqlite".to_string(),
                            ),
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

            let keys = match server.store.knowledge_keys_list_by_key(
                &workspace,
                bm_storage::KnowledgeKeysListByKeyRequest {
                    key: key_slug,
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

            let card_ids = keys
                .items
                .iter()
                .map(|row| row.card_id.clone())
                .collect::<Vec<_>>();

            if card_ids.is_empty() {
                return OpResponse::success(
                    env.cmd.clone(),
                    json!({
                        "workspace": workspace.as_str(),
                        "branch": desired_branch.as_deref().unwrap_or(KB_BRANCH),
                        "graph_doc": desired_graph_doc.as_deref().unwrap_or(KB_GRAPH_DOC),
                        "cards": [],
                        "pagination": { "cursor": Value::Null, "next_cursor": Value::Null, "has_more": false, "limit": limit, "count": 0 },
                        "truncated": false
                    }),
                );
            }

            let slice = match server.store.graph_query(
                &workspace,
                desired_branch.as_deref().unwrap_or(KB_BRANCH),
                desired_graph_doc.as_deref().unwrap_or(KB_GRAPH_DOC),
                bm_storage::GraphQueryRequest {
                    ids: Some(card_ids.clone()),
                    types: Some(vec!["knowledge".to_string()]),
                    status: None,
                    tags_any: None,
                    tags_all: None,
                    text,
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
                            "branch": desired_branch.as_deref().unwrap_or(KB_BRANCH),
                            "graph_doc": desired_graph_doc.as_deref().unwrap_or(KB_GRAPH_DOC),
                            "cards": [],
                            "pagination": { "cursor": Value::Null, "next_cursor": Value::Null, "has_more": false, "limit": limit, "count": 0 },
                            "truncated": false
                        }),
                    );
                    resp.warnings.push(crate::warning(
                        "KNOWLEDGE_BASE_MISSING",
                        "Knowledge base branch is missing",
                        "Create knowledge via think.knowledge.upsert (it will auto-create kb/main), then retry query.",
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

            let mut cards = crate::graph_nodes_to_cards(slice.nodes);
            if !include_drafts {
                cards.retain(|card| crate::card_value_visibility_allows(card, false, None));
            }

            // Prefer index ordering (recency-first by updated_at_ms).
            let mut pos = std::collections::HashMap::<String, usize>::new();
            for (idx, card_id) in card_ids.iter().enumerate() {
                pos.insert(card_id.clone(), idx);
            }
            cards.sort_by(|a, b| {
                let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let a_pos = pos.get(a_id).cloned().unwrap_or(usize::MAX);
                let b_pos = pos.get(b_id).cloned().unwrap_or(usize::MAX);
                a_pos.cmp(&b_pos).then_with(|| a_id.cmp(b_id))
            });
            if cards.len() > limit {
                cards.truncate(limit);
            }
            let count = cards.len();

            let mut result = json!({
                "workspace": workspace.as_str(),
                "branch": desired_branch.as_deref().unwrap_or(KB_BRANCH),
                "graph_doc": desired_graph_doc.as_deref().unwrap_or(KB_GRAPH_DOC),
                "cards": cards,
                "pagination": { "cursor": Value::Null, "next_cursor": Value::Null, "has_more": keys.has_more, "limit": limit, "count": count },
                "truncated": false
            });

            if let Some(max_chars) = max_chars {
                let (max_chars, clamped) = crate::clamp_budget_max(max_chars);
                let (_used, truncated) =
                    crate::enforce_graph_list_budget(&mut result, "cards", max_chars);
                crate::set_truncated_flag(&mut result, truncated);
                if truncated || clamped {
                    let warnings = crate::budget_warnings(truncated, false, clamped);
                    let mut resp = OpResponse::success(env.cmd.clone(), result);
                    resp.warnings.extend(warnings);
                    return resp;
                }
            }

            return OpResponse::success(env.cmd.clone(), result);
        }
    }

    args_obj.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );

    // If the knowledge base branch exists, default reads to it (cross-session memory).
    if kb_exists {
        args_obj
            .entry("ref".to_string())
            .or_insert_with(|| Value::String(KB_BRANCH.to_string()));
        args_obj
            .entry("graph_doc".to_string())
            .or_insert_with(|| Value::String(KB_GRAPH_DOC.to_string()));
    }

    let handler_resp =
        crate::handlers::dispatch_handler(server, "knowledge_list", Value::Object(args_obj))
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "knowledge_list dispatch failed"));

    crate::ops::handler_to_op_response(&env.cmd, Some(workspace.as_str()), handler_resp)
}

pub(crate) fn handle_knowledge_recall(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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
        .unwrap_or(12)
        .clamp(1, 50);
    let include_drafts = args_obj
        .get("include_drafts")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = args_obj
        .get("text")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let max_chars = args_obj
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

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

    let card_ids = keys
        .items
        .iter()
        .map(|row| row.card_id.clone())
        .collect::<Vec<_>>();

    if card_ids.is_empty() {
        return OpResponse::success(
            env.cmd.clone(),
            json!({
                "workspace": workspace.as_str(),
                "branch": KB_BRANCH,
                "graph_doc": KB_GRAPH_DOC,
                "cards": [],
                "pagination": { "cursor": Value::Null, "next_cursor": Value::Null, "has_more": false, "limit": limit, "count": 0 },
                "truncated": false
            }),
        );
    }

    let slice = match server.store.graph_query(
        &workspace,
        KB_BRANCH,
        KB_GRAPH_DOC,
        bm_storage::GraphQueryRequest {
            ids: Some(card_ids.clone()),
            types: Some(vec!["knowledge".to_string()]),
            status: None,
            tags_any: None,
            tags_all: None,
            text,
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
                    "branch": KB_BRANCH,
                    "graph_doc": KB_GRAPH_DOC,
                    "cards": [],
                    "pagination": { "cursor": Value::Null, "next_cursor": Value::Null, "has_more": false, "limit": limit, "count": 0 },
                    "truncated": false
                }),
            );
            resp.warnings.push(crate::warning(
                "KNOWLEDGE_BASE_MISSING",
                "Knowledge base branch is missing",
                "Create knowledge via think.knowledge.upsert (it will auto-create kb/main), then retry recall.",
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

    let mut cards = crate::graph_nodes_to_cards(slice.nodes);
    if !include_drafts {
        cards.retain(|card| crate::card_value_visibility_allows(card, false, None));
    }

    // Prefer index ordering (recency-first by updated_at_ms).
    let mut pos = std::collections::HashMap::<String, usize>::new();
    for (idx, card_id) in card_ids.iter().enumerate() {
        pos.insert(card_id.clone(), idx);
    }
    cards.sort_by(|a, b| {
        let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let a_pos = pos.get(a_id).cloned().unwrap_or(usize::MAX);
        let b_pos = pos.get(b_id).cloned().unwrap_or(usize::MAX);
        a_pos.cmp(&b_pos).then_with(|| a_id.cmp(b_id))
    });
    if cards.len() > limit {
        cards.truncate(limit);
    }
    let count = cards.len();

    let mut result = json!({
        "workspace": workspace.as_str(),
        "branch": KB_BRANCH,
        "graph_doc": KB_GRAPH_DOC,
        "cards": cards,
        "pagination": { "cursor": Value::Null, "next_cursor": Value::Null, "has_more": keys.has_more, "limit": limit, "count": count },
        "truncated": false
    });

    if let Some(max_chars) = max_chars {
        let (max_chars, clamped) = crate::clamp_budget_max(max_chars);
        let (_used, truncated) = crate::enforce_graph_list_budget(&mut result, "cards", max_chars);
        crate::set_truncated_flag(&mut result, truncated);
        if truncated || clamped {
            // OpResponse warning shape matches tool warnings (same helper).
            // Keep it small: budget warnings only.
            let warnings = crate::budget_warnings(truncated, false, clamped);
            let mut resp = OpResponse::success(env.cmd.clone(), result);
            resp.warnings.extend(warnings);
            return resp;
        }
    }

    OpResponse::success(env.cmd.clone(), result)
}

pub(crate) fn handle_knowledge_lint(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    super::knowledge_lint::handle(server, env)
}

pub(crate) fn handle_reasoning_seed(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_template", env.args.clone())
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_template dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(crate) fn handle_reasoning_pipeline(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let handler_resp =
        crate::handlers::dispatch_handler(server, "think_pipeline", env.args.clone())
            .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_pipeline dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(crate) fn handle_idea_branch_merge(
    server: &mut crate::McpServer,
    env: &Envelope,
) -> OpResponse {
    let handler_resp = crate::handlers::dispatch_handler(server, "merge", env.args.clone())
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "merge dispatch failed"));
    crate::ops::handler_to_op_response(&env.cmd, env.workspace.as_deref(), handler_resp)
}

pub(crate) fn should_skip_handler_name(name: &str) -> bool {
    if name.starts_with("tasks_") {
        return true;
    }
    if name.starts_with("graph_") {
        return true;
    }
    if matches!(
        name,
        // Dedicated v1 portals:
        "status" | "open" | "workspace_use" | "workspace_reset"
            // System:
            | "storage" | "init" | "help" | "skill" | "diagnostics"
            // VCS / docs:
            | "branch_create" | "branch_list" | "checkout" | "branch_rename" | "branch_delete"
            | "notes_commit" | "commit" | "log" | "reflog" | "reset" | "show" | "diff" | "merge"
            | "tag_create" | "tag_list" | "tag_delete"
            | "docs_list" | "transcripts_search" | "transcripts_open" | "transcripts_digest"
            | "export"
            // Curated cmds (registered explicitly):
            | "macro_branch_note"
            | "knowledge_list"
            | "think_lint"
            | "think_template"
            | "think_pipeline"
            | "atlas_suggest"
            | "macro_atlas_apply"
            | "atlas_bindings_list"
            | "think_macro_counter_hypothesis_stub"
    ) {
        return true;
    }
    false
}

pub(crate) fn handler_think_cmd(name: &str) -> String {
    if let Some(suffix) = name.strip_prefix("think_") {
        return format!("think.{}", name_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchors_") {
        return format!("think.anchor.{}", name_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchor_") {
        return format!("think.anchor.{}", name_to_cmd_segments(suffix));
    }
    format!("think.{}", name_to_cmd_segments(name))
}

pub(crate) fn normalized_claim(title: Option<&str>, text: Option<&str>) -> String {
    let mut raw = String::new();
    if let Some(title) = title {
        raw.push_str(title);
        raw.push('\n');
    }
    if let Some(text) = text {
        raw.push_str(text);
    }
    normalize_ws(&raw).to_ascii_lowercase()
}

pub(crate) fn normalize_ws(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_space = false;
    for ch in raw.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

pub(crate) fn fnv1a64(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

pub(crate) fn upsert_card_id_into_value(card: Value, id: &str) -> Value {
    match card {
        Value::Object(mut obj) => {
            obj.insert("id".to_string(), Value::String(id.to_string()));
            Value::Object(obj)
        }
        Value::String(text) => json!({ "id": id, "text": text }),
        Value::Null => json!({ "id": id, "text": "<card>" }),
        other => json!({ "id": id, "text": other.to_string() }),
    }
}
