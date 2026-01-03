#![forbid(unsafe_code)]

use super::super::graph::ThinkCardCommitInternalArgs;
use crate::*;
use serde_json::{Value, json};

impl McpServer {
    pub(crate) fn tool_branchmind_think_pipeline(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let target = args_obj
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let branch_override = match optional_string(args_obj, "branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let trace_doc = match optional_string(args_obj, "trace_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let graph_doc = match optional_string(args_obj, "graph_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let notes_doc = match optional_string(args_obj, "notes_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        if let Err(resp) = ensure_nonempty_doc(&trace_doc, "trace_doc") {
            return resp;
        }
        if let Err(resp) = ensure_nonempty_doc(&graph_doc, "graph_doc") {
            return resp;
        }
        if let Err(resp) = ensure_nonempty_doc(&notes_doc, "notes_doc") {
            return resp;
        }

        let scope = match self.resolve_reasoning_scope(
            &workspace,
            ReasoningScopeInput {
                target,
                branch: branch_override,
                notes_doc,
                graph_doc,
                trace_doc,
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let status_map = match args_obj.get("status") {
            None => std::collections::BTreeMap::new(),
            Some(Value::Object(obj)) => obj
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.to_string(), s.to_string())))
                .collect::<std::collections::BTreeMap<String, String>>(),
            Some(Value::Null) => std::collections::BTreeMap::new(),
            Some(_) => return ai_error("INVALID_INPUT", "status must be an object"),
        };

        let note_decision = args_obj
            .get("note_decision")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let note_title = match optional_string(args_obj, "note_title") {
            Ok(v) => v.unwrap_or_else(|| "Decision".to_string()),
            Err(resp) => return resp,
        };
        let note_format = match optional_string(args_obj, "note_format") {
            Ok(v) => v.unwrap_or_else(|| "text".to_string()),
            Err(resp) => return resp,
        };

        let stages = [
            ("frame", args_obj.get("frame")),
            ("hypothesis", args_obj.get("hypothesis")),
            ("test", args_obj.get("test")),
            ("evidence", args_obj.get("evidence")),
            ("decision", args_obj.get("decision")),
        ];

        let mut provided_stages = std::collections::BTreeSet::new();
        for (stage, value) in stages {
            if value.is_some_and(|value| !value.is_null()) {
                provided_stages.insert(stage.to_string());
            }
        }
        let allowed_stages = ["frame", "hypothesis", "test", "evidence", "decision"];
        for (stage, status) in &status_map {
            if !allowed_stages.iter().any(|s| *s == stage) {
                return ai_error(
                    "INVALID_INPUT",
                    "status keys must be one of: frame, hypothesis, test, evidence, decision",
                );
            }
            if status.trim().is_empty() {
                return ai_error("INVALID_INPUT", "status values must be non-empty strings");
            }
            if !provided_stages.contains(stage) {
                return ai_error(
                    "INVALID_INPUT",
                    "status provided for a missing pipeline stage",
                );
            }
        }

        let mut created = Vec::new();
        let mut prev_card_id: Option<String> = None;
        let mut decision_summary: Option<String> = None;
        let mut decision_card_id: Option<String> = None;
        let mut aggregate_nodes = 0usize;
        let mut aggregate_edges = 0usize;
        let mut last_seq: Option<i64> = None;

        for (stage, value) in stages {
            let Some(value) = value else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            let mut parsed = match parse_think_card(&workspace, value.clone()) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            parsed.card_type = stage.to_string();
            if let Some(status) = status_map.get(stage) {
                parsed.status = status.clone();
            }
            let supports = prev_card_id.clone().into_iter().collect::<Vec<_>>();
            let (card_id, commit) =
                match self.commit_think_card_internal(ThinkCardCommitInternalArgs {
                    workspace: &workspace,
                    branch: &scope.branch,
                    trace_doc: &scope.trace_doc,
                    graph_doc: &scope.graph_doc,
                    parsed: parsed.clone(),
                    supports: &supports,
                    blocks: &[],
                }) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
            aggregate_nodes += commit.nodes_upserted;
            aggregate_edges += commit.edges_upserted;
            if let Some(seq) = commit.last_seq {
                last_seq = Some(seq);
            }
            if stage == "decision" {
                decision_card_id = Some(card_id.clone());
                decision_summary = parsed
                    .title
                    .clone()
                    .or(parsed.text.clone())
                    .map(|s| s.trim().to_string());
            }
            created.push(json!({
                "stage": stage,
                "card_id": card_id,
                "inserted": commit.inserted,
                "edges_upserted": commit.edges_upserted,
                "last_seq": commit.last_seq
            }));
            prev_card_id = Some(card_id);
        }

        if created.is_empty() {
            return ai_error("INVALID_INPUT", "pipeline requires at least one stage");
        }

        let mut decision_note = Value::Null;
        if note_decision
            && let (Some(card_id), Some(summary)) = (decision_card_id.clone(), decision_summary)
        {
            let meta = json!({
                "source": "think_pipeline",
                "card_id": card_id,
                "stage": "decision"
            })
            .to_string();
            let content = format!("Decision ({card_id}): {summary}");
            let entry = match self.store.doc_append_note(
                &workspace,
                bm_storage::DocAppendRequest {
                    branch: scope.branch.clone(),
                    doc: scope.notes_doc.clone(),
                    title: Some(note_title),
                    format: Some(note_format),
                    meta_json: Some(meta),
                    content,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            decision_note = json!({
                "seq": entry.seq,
                "ts": ts_ms_to_rfc3339(entry.ts_ms),
                "ts_ms": entry.ts_ms,
                "doc": entry.doc,
                "card_id": card_id
            });
        }

        ai_ok(
            "think_pipeline",
            json!({
                "workspace": workspace.as_str(),
                "branch": scope.branch,
                "trace_doc": scope.trace_doc,
                "graph_doc": scope.graph_doc,
                "notes_doc": scope.notes_doc,
                "cards": created,
                "graph_applied": {
                    "nodes_upserted": aggregate_nodes,
                    "edges_upserted": aggregate_edges
                },
                "last_seq": last_seq,
                "decision_note": decision_note
            }),
        )
    }
}
