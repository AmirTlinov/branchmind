#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(in super::super) struct ThinkCardCommitInternalArgs<'a> {
    pub(in super::super) workspace: &'a WorkspaceId,
    pub(in super::super) branch: &'a str,
    pub(in super::super) trace_doc: &'a str,
    pub(in super::super) graph_doc: &'a str,
    pub(in super::super) parsed: ParsedThinkCard,
    pub(in super::super) supports: &'a [String],
    pub(in super::super) blocks: &'a [String],
}

impl McpServer {
    pub(in super::super) fn commit_think_card_internal(
        &mut self,
        args: ThinkCardCommitInternalArgs<'_>,
    ) -> Result<(String, bm_storage::ThinkCardCommitResult), Value> {
        let ThinkCardCommitInternalArgs {
            workspace,
            branch,
            trace_doc,
            graph_doc,
            parsed,
            supports,
            blocks,
        } = args;
        let mut parsed = parsed;

        // Meaning-mode: durable artifacts are shared-by-default (agent_id is audit/meta only).
        // Keep legacy lane tags readable (as draft markers), but ensure new writes are normalized.
        apply_lane_stamp_to_tags(&mut parsed.tags, None);
        apply_lane_stamp_to_meta(&mut parsed.meta_value, None);

        // Visibility defaults (type-based) keep the portal low-noise without deleting anything.
        //
        // Principle: "frontier lives in canon, chatter lives in draft".
        // - hypothesis/question are the active frontier and should be visible in smart views.
        // - decision/evidence/test are durable anchors and should be visible by default.
        // - update is a compact narrative artifact and should be visible in explore/audit views.
        // - note (and other free-form) default to draft to prevent long-term noise.
        if !tags_has(&parsed.tags, VIS_TAG_DRAFT) && !tags_has(&parsed.tags, VIS_TAG_CANON) {
            let default = match parsed.card_type.trim().to_ascii_lowercase().as_str() {
                "decision" | "evidence" | "test" | "hypothesis" | "question" | "update" => {
                    VIS_TAG_CANON
                }
                _ => VIS_TAG_DRAFT,
            };
            parsed.tags.push(default.to_string());
        }
        parsed.tags =
            bm_core::graph::normalize_tags(&parsed.tags).unwrap_or_else(|_| parsed.tags.clone());
        let card_id = match parsed.card_id.clone() {
            Some(id) => id,
            None => match self.store.next_card_id(workspace) {
                Ok(id) => id,
                Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
            },
        };
        let (payload_json, meta_json, content) = build_think_card_payload(
            &card_id,
            &parsed.card_type,
            parsed.title.as_deref(),
            parsed.text.as_deref(),
            &parsed.status,
            &parsed.tags,
            &parsed.meta_value,
        );

        let result = match self.store.think_card_commit(
            workspace,
            bm_storage::ThinkCardCommitRequest {
                branch: branch.to_string(),
                trace_doc: trace_doc.to_string(),
                graph_doc: graph_doc.to_string(),
                card: bm_storage::ThinkCardInput {
                    card_id: card_id.clone(),
                    card_type: parsed.card_type.clone(),
                    title: parsed.title.clone(),
                    text: parsed.text.clone(),
                    status: Some(parsed.status.clone()),
                    tags: parsed.tags.clone(),
                    meta_json: Some(meta_json),
                    content,
                    payload_json,
                },
                supports: supports.to_vec(),
                blocks: blocks.to_vec(),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::UnknownBranch) => {
                return Err(ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown branch",
                    Some("Call branch_list to discover existing branches, then retry."),
                    vec![suggest_call(
                        "branch_list",
                        "List known branches for this workspace.",
                        "high",
                        json!({ "workspace": workspace.as_str() }),
                    )],
                ));
            }
            Err(StoreError::InvalidInput("unsupported card.type")) => {
                let supported = bm_core::think::SUPPORTED_THINK_CARD_TYPES;
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    "Unsupported card.type",
                    Some(&format!("Supported: {}", supported.join(", "))),
                    vec![suggest_call(
                        "think_template",
                        "Get a valid card skeleton.",
                        "high",
                        json!({ "workspace": workspace.as_str(), "type": "hypothesis" }),
                    )],
                ));
            }
            Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        };

        Ok((card_id, result))
    }
}
