#![forbid(unsafe_code)]

use crate::ops::{
    BudgetPolicy, CommandSpec, ConfirmLevel, DocRef, Envelope, OpError, OpResponse, Safety,
    SchemaSource, Stability, Tier, ToolName, legacy_to_cmd_segments,
};
use serde_json::{Value, json};

const KB_BRANCH: &str = "kb/main";
const KB_GRAPH_DOC: &str = "kb-graph";
const KB_TRACE_DOC: &str = "kb-trace";

pub(crate) fn register(specs: &mut Vec<CommandSpec>) {
    // v1 curated commands (custom UX layer).
    specs.push(CommandSpec {
        cmd: "think.knowledge.upsert".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.upsert".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "anchor": { "type": "string" },
                    "key": { "type": "string", "description": "Stable knowledge key slug (enables evolvable upsert)." },
                    "card": { "type": ["object", "string"] },
                    "supports": { "type": "array", "items": { "type": "string" } },
                    "blocks": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["card"]
            }),
            example_minimal_args: json!({
                "anchor": "core",
                "key": "determinism",
                "card": { "title": "Invariant", "text": "Claim: ... / Apply: ... / Proof: ... / Expiry: ..." }
            }),
        },
        op_aliases: vec!["knowledge.upsert".to_string()],
        legacy_tool: None,
        handler: Some(handle_knowledge_upsert),
    });

    specs.push(CommandSpec {
        cmd: "think.knowledge.query".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.query".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "limit": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": []
            }),
            example_minimal_args: json!({ "limit": 12 }),
        },
        op_aliases: vec!["knowledge.query".to_string()],
        legacy_tool: None,
        handler: Some(handle_knowledge_query),
    });

    specs.push(CommandSpec {
        cmd: "think.knowledge.recall".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.recall".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "anchor": {
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ],
                        "description": "Anchor slug(s) or a:<slug> (recall is anchor-first)."
                    },
                    "limit": { "type": "integer" },
                    "text": { "type": "string" },
                    "include_drafts": { "type": "boolean" },
                    "max_chars": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({
                "anchor": "core",
                "limit": 12
            }),
        },
        op_aliases: vec!["knowledge.recall".to_string()],
        legacy_tool: None,
        handler: Some(handle_knowledge_recall),
    });

    specs.push(CommandSpec {
        cmd: "think.knowledge.lint".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.knowledge.lint".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": []
            }),
            example_minimal_args: json!({}),
        },
        op_aliases: vec!["knowledge.lint".to_string()],
        legacy_tool: None,
        handler: Some(handle_knowledge_lint),
    });

    specs.push(CommandSpec {
        cmd: "think.reasoning.seed".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.reasoning.seed".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: true,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Legacy,
        op_aliases: vec!["reasoning.seed".to_string()],
        legacy_tool: Some("think_template".to_string()),
        handler: Some(handle_reasoning_seed),
    });

    specs.push(CommandSpec {
        cmd: "think.reasoning.pipeline".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Stable,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.reasoning.pipeline".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Legacy,
        op_aliases: vec!["reasoning.pipeline".to_string()],
        legacy_tool: Some("think_pipeline".to_string()),
        handler: Some(handle_reasoning_pipeline),
    });

    // Idea-branch helpers as golden ops (legacy-backed for v1).
    specs.push(CommandSpec {
        cmd: "think.idea.branch.create".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Experimental,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.idea.branch.create".to_string(),
        },
        safety: Safety {
            destructive: false,
            confirm_level: ConfirmLevel::None,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Legacy,
        op_aliases: vec!["idea.branch.create".to_string()],
        legacy_tool: Some("macro_branch_note".to_string()),
        handler: None,
    });

    specs.push(CommandSpec {
        cmd: "think.idea.branch.merge".to_string(),
        domain_tool: ToolName::ThinkOps,
        tier: Tier::Gold,
        stability: Stability::Experimental,
        doc_ref: DocRef {
            path: "docs/contracts/V1_COMMANDS.md".to_string(),
            anchor: "#think.idea.branch.merge".to_string(),
        },
        safety: Safety {
            destructive: true,
            confirm_level: ConfirmLevel::Soft,
            idempotent: false,
        },
        budget: BudgetPolicy::standard(),
        schema: SchemaSource::Custom {
            args_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "into": { "type": "string" },
                    "doc": { "type": "string" },
                    "dry_run": { "type": "boolean" }
                },
                "required": ["from", "into"]
            }),
            example_minimal_args: json!({
                "from": "<idea-branch>",
                "into": "<target-branch>",
                "dry_run": true
            }),
        },
        op_aliases: vec!["idea.branch.merge".to_string()],
        legacy_tool: None,
        handler: Some(handle_idea_branch_merge),
    });

    // Auto-map remaining legacy (non-tasks) tools into think op=call surface.
    for def in crate::tools::tool_definitions(crate::Toolset::Full) {
        let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if should_skip_legacy_tool(name) {
            continue;
        }

        let cmd = legacy_think_cmd(name);
        let op_aliases = Vec::<String>::new();

        specs.push(CommandSpec {
            cmd,
            domain_tool: ToolName::ThinkOps,
            tier: Tier::Advanced,
            stability: Stability::Stable,
            doc_ref: DocRef {
                path: "docs/contracts/V1_COMMANDS.md".to_string(),
                anchor: "#cmd-index".to_string(),
            },
            safety: Safety {
                destructive: false,
                confirm_level: ConfirmLevel::None,
                idempotent: matches!(
                    name,
                    "think_query"
                        | "think_pack"
                        | "think_next"
                        | "think_frontier"
                        | "think_lint"
                        | "knowledge_list"
                ),
            },
            budget: BudgetPolicy::standard(),
            schema: SchemaSource::Legacy,
            op_aliases,
            legacy_tool: Some(name.to_string()),
            handler: None,
        });
    }
}

fn handle_knowledge_upsert(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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
            return crate::ops::legacy_to_op_response(&env.cmd, Some(workspace.as_str()), resp);
        }
    };

    let anchor = args_obj.get("anchor").and_then(|v| v.as_str());
    let key = args_obj.get("key").and_then(|v| v.as_str());

    let card_id = if let Some(key) = key {
        let Some(anchor) = anchor else {
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
        let key_tag = {
            let candidate = if key.trim().starts_with(crate::KEY_TAG_PREFIX) {
                key.trim().to_string()
            } else {
                format!("{}{}", crate::KEY_TAG_PREFIX, key.trim())
            };
            crate::normalize_key_id_tag(&candidate).ok_or_else(|| OpError {
                code: "INVALID_INPUT".to_string(),
                message: "key must be a valid slug (k:<slug>)".to_string(),
                recovery: Some(
                    "Use key like: determinism | k:determinism | storage-locking".to_string(),
                ),
            })
        };
        let key_tag = match key_tag {
            Ok(v) => v,
            Err(e) => return OpResponse::error(env.cmd.clone(), e),
        };

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

    let legacy =
        crate::tools::dispatch_tool(server, "think_add_knowledge", Value::Object(forwarded))
            .unwrap_or_else(|| {
                crate::ai_error("INTERNAL_ERROR", "think_add_knowledge dispatch failed")
            });

    crate::ops::legacy_to_op_response(&env.cmd, Some(workspace.as_str()), legacy)
}

fn handle_knowledge_query(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

    // Convenience: allow `key` in v1 args and translate into a stable tag filter.
    if let Some(key) = args_obj.get("key").and_then(|v| v.as_str()) {
        let candidate = if key.trim().starts_with(crate::KEY_TAG_PREFIX) {
            key.trim().to_string()
        } else {
            format!("{}{}", crate::KEY_TAG_PREFIX, key.trim())
        };
        if let Some(tag) = crate::normalize_key_id_tag(&candidate) {
            let mut tags_all = args_obj
                .get("tags_all")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if !tags_all.iter().any(|v| v.as_str() == Some(tag.as_str())) {
                tags_all.push(Value::String(tag));
            }
            args_obj.insert("tags_all".to_string(), Value::Array(tags_all));
        }
    }

    args_obj.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );

    // If the knowledge base branch exists, default reads to it (cross-session memory).
    if let Ok(branches) = server.store.branch_list(&workspace, 501)
        && branches.iter().any(|b| b.name == KB_BRANCH)
    {
        args_obj
            .entry("ref".to_string())
            .or_insert_with(|| Value::String(KB_BRANCH.to_string()));
        args_obj
            .entry("graph_doc".to_string())
            .or_insert_with(|| Value::String(KB_GRAPH_DOC.to_string()));
    }

    let legacy = crate::tools::dispatch_tool(server, "knowledge_list", Value::Object(args_obj))
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "knowledge_list dispatch failed"));

    crate::ops::legacy_to_op_response(&env.cmd, Some(workspace.as_str()), legacy)
}

fn handle_knowledge_recall(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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

fn handle_knowledge_lint(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
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
        .or_insert_with(|| json!(50));
    let legacy = crate::tools::dispatch_tool(server, "knowledge_list", Value::Object(args_obj))
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "knowledge_list dispatch failed"));
    let mut resp = crate::ops::legacy_to_op_response(&env.cmd, env.workspace.as_deref(), legacy);

    let scope = env
        .args
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("step");
    if scope == "step" {
        let count = resp
            .result
            .get("cards")
            .and_then(|v| v.as_array())
            .map(|cards| cards.len())
            .unwrap_or(0);
        if count > 20 {
            resp.actions.push(crate::ops::Action {
                action_id: "knowledge.consolidate.open".to_string(),
                priority: crate::ops::ActionPriority::High,
                tool: "think".to_string(),
                args: json!({
                    "op": "call",
                    "cmd": "think.knowledge.query",
                    "args": { "scope": "step", "limit": 20 }
                }),
                why: "Слишком много knowledge в scope=step — открыть список для консолидации."
                    .to_string(),
                risk: "Низкий".to_string(),
            });
        }
    }

    resp
}

fn handle_reasoning_seed(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let legacy = crate::tools::dispatch_tool(server, "think_template", env.args.clone())
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_template dispatch failed"));
    crate::ops::legacy_to_op_response(&env.cmd, env.workspace.as_deref(), legacy)
}

fn handle_reasoning_pipeline(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let legacy = crate::tools::dispatch_tool(server, "think_pipeline", env.args.clone())
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "think_pipeline dispatch failed"));
    crate::ops::legacy_to_op_response(&env.cmd, env.workspace.as_deref(), legacy)
}

fn handle_idea_branch_merge(server: &mut crate::McpServer, env: &Envelope) -> OpResponse {
    let legacy = crate::tools::dispatch_tool(server, "merge", env.args.clone())
        .unwrap_or_else(|| crate::ai_error("INTERNAL_ERROR", "merge dispatch failed"));
    crate::ops::legacy_to_op_response(&env.cmd, env.workspace.as_deref(), legacy)
}

fn should_skip_legacy_tool(name: &str) -> bool {
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
    ) {
        return true;
    }
    false
}

fn legacy_think_cmd(name: &str) -> String {
    if let Some(suffix) = name.strip_prefix("think_") {
        return format!("think.{}", legacy_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchors_") {
        return format!("think.anchor.{}", legacy_to_cmd_segments(suffix));
    }
    if let Some(suffix) = name.strip_prefix("anchor_") {
        return format!("think.anchor.{}", legacy_to_cmd_segments(suffix));
    }
    format!("think.{}", legacy_to_cmd_segments(name))
}

fn normalized_claim(title: Option<&str>, text: Option<&str>) -> String {
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

fn normalize_ws(raw: &str) -> String {
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

fn fnv1a64(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn upsert_card_id_into_value(card: Value, id: &str) -> Value {
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
