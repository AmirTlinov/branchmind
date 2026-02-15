#![forbid(unsafe_code)]

// NOTE: Split into `open/*` submodules to avoid a high-churn god-file and keep the `open` handler
// agent-friendly to evolve.

use crate::*;
use serde_json::{Value, json};
use std::path::Path;

mod ids;
use ids::*;

mod compact;
use compact::parse_response_verbosity as parse_open_response_verbosity;

mod compact_budget;

mod budget;
use budget::{apply_open_budget_and_verbosity, dedupe_warnings_by_code};

mod cards;
use cards::*;

mod util;

mod via_resume_super;
use via_resume_super::*;

mod kinds;
use kinds::{
    OpenJobEventRefArgs, open_card, open_doc_entry_ref, open_job, open_job_artifact,
    open_job_event_ref, open_runner_ref, open_slice,
};

impl McpServer {
    pub(crate) fn tool_branchmind_open(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let id = match require_string(args_obj, "id") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let include_drafts = match optional_bool(args_obj, "include_drafts") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let include_content = match optional_bool(args_obj, "include_content") {
            Ok(v) => v.unwrap_or(false),
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
        let artifact_max_chars = max_chars.unwrap_or(4000).clamp(1, 4000);
        let verbosity = match parse_open_response_verbosity(args_obj, self.response_verbosity) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut id = id.trim().to_string();
        if id.is_empty() {
            return ai_error("INVALID_INPUT", "id must not be empty");
        }

        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();

        let mut jump: Option<Value> = None;
        if !id.starts_with("artifact://") && !is_anchor_id(&id) && looks_like_repo_path_id(&id) {
            let repo_root = match self.store.workspace_path_primary_get(&workspace) {
                Ok(v) => v,
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let repo_root_path = repo_root.as_deref().map(Path::new);
            let repo_rel = match repo_rel_from_path_input(&id, repo_root_path) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let prefixes = repo_rel_prefixes(&repo_rel);

            let lookup = match self.store.anchor_bindings_lookup_any(
                &workspace,
                bm_storage::AnchorBindingsLookupAnyRequest {
                    repo_rels: prefixes.clone(),
                    limit: 50,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let Some(best) = lookup.bindings.first() else {
                let anchor_id = {
                    let base = if repo_rel == "." {
                        "root".to_string()
                    } else {
                        repo_rel
                            .rsplit('/')
                            .next()
                            .unwrap_or("component")
                            .to_string()
                    };
                    let mut slug = String::new();
                    let mut prev_dash = false;
                    for ch in base.chars() {
                        let lc = ch.to_ascii_lowercase();
                        if lc.is_ascii_alphanumeric() {
                            slug.push(lc);
                            prev_dash = false;
                            continue;
                        }
                        if matches!(lc, '-' | '_' | '.' | ' ') {
                            if !slug.is_empty() && !prev_dash {
                                slug.push('-');
                                prev_dash = true;
                            }
                            continue;
                        }
                        if !slug.is_empty() && !prev_dash {
                            slug.push('-');
                            prev_dash = true;
                        }
                    }
                    let slug = slug.trim_matches('-');
                    let slug = if slug.is_empty() { "component" } else { slug };
                    format!("a:{slug}")
                };
                let title = anchor_title_from_id(&anchor_id);
                return ai_error_with(
                    "UNKNOWN_ID",
                    "No anchor binding found for path",
                    Some(
                        "Bind this path to an anchor (manual bind or atlas seed), then retry open.",
                    ),
                    vec![
                        suggest_call(
                            "macro_anchor_note",
                            "Create an anchor and bind it to this path.",
                            "high",
                            json!({
                                "anchor": anchor_id,
                                "title": title,
                                "kind": "component",
                                "bind_paths": [repo_rel],
                                "content": "Bind this code area to a semantic anchor (why/ownership/invariants).",
                                "card_type": "note",
                                "visibility": "canon"
                            }),
                        ),
                        suggest_call(
                            "atlas_suggest",
                            "Seed a directory-based atlas (then apply bindings).",
                            "low",
                            json!({ "granularity": "depth2", "limit": 30 }),
                        ),
                    ],
                );
            };

            let resolved = match self
                .store
                .anchor_resolve_id(&workspace, best.anchor_id.as_str())
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let effective_anchor_id = resolved.unwrap_or_else(|| best.anchor_id.clone());
            let effective_anchor_id_json = effective_anchor_id.clone();

            jump = Some(json!({
                "input": {
                    "id": id.as_str(),
                    "repo_rel": repo_rel.as_str(),
                    "bound_root": repo_root
                },
                "matched_binding": {
                    "kind": best.kind.as_str(),
                    "repo_rel": best.repo_rel.as_str(),
                    "created_at_ms": best.created_at_ms,
                    "updated_at_ms": best.updated_at_ms
                },
                "resolved": {
                    "anchor_id": effective_anchor_id_json,
                    "candidates": lookup.bindings.len(),
                    "has_more": lookup.has_more
                }
            }));

            id = effective_anchor_id;
        }

        let mut result = if id.starts_with("artifact://jobs/") {
            match open_job_artifact(self, &workspace, &id, artifact_max_chars) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else if is_anchor_id(&id) {
            let resolved = match self.store.anchor_resolve_id(&workspace, &id) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let effective_anchor_id = resolved.clone().unwrap_or_else(|| id.clone());
            if let Some(canonical) = resolved
                && !id.eq_ignore_ascii_case(&canonical)
            {
                warnings.push(warning(
                    "ANCHOR_ALIAS_RESOLVED",
                    "anchor id resolved via alias mapping",
                    "Use the canonical anchor id for new work; history is included automatically.",
                ));
            }

            let anchor_row = match self.store.anchor_get(
                &workspace,
                bm_storage::AnchorGetRequest {
                    id: effective_anchor_id.clone(),
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let query_limit = if include_drafts {
                limit
            } else {
                limit.saturating_mul(4).clamp(1, 200)
            };

            let mut anchor_ids = match anchor_row.as_ref() {
                Some(anchor) => {
                    let mut ids = vec![anchor.id.clone()];
                    ids.extend(anchor.aliases.clone());
                    ids
                }
                None => vec![effective_anchor_id.clone()],
            };
            anchor_ids.sort();
            anchor_ids.dedup();

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

            let anchor_id = anchor_row
                .as_ref()
                .map(|a| a.id.clone())
                .or_else(|| links.links.first().map(|l| l.anchor_id.clone()))
                .unwrap_or_else(|| effective_anchor_id.clone());

            // Collect cards by following the anchor_links index across graphs.
            let mut cards = Vec::<Value>::new();
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
                        Err(StoreError::InvalidInput(msg)) => {
                            return ai_error("INVALID_INPUT", msg);
                        }
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
                    .then_with(|| card_type(a).cmp(card_type(b)))
                    .then_with(|| card_ts(b).cmp(&card_ts(a)))
                    .then_with(|| card_id(a).cmp(card_id(b)))
            });
            cards.truncate(limit);

            let (anchor, registered) = if let Some(anchor) = anchor_row {
                (anchor, true)
            } else {
                warnings.push(warning(
                    "ANCHOR_UNREGISTERED",
                    "Anchor is not registered in the anchors index; showing a best-effort snapshot from anchor_links.",
                    "Optional: create the anchor via macro_anchor_note to add title/kind/refs and explicit relations.",
                ));
                (
                    bm_storage::AnchorRow {
                        id: anchor_id.clone(),
                        title: anchor_title_from_id(&anchor_id),
                        kind: "component".to_string(),
                        status: "active".to_string(),
                        description: None,
                        refs: Vec::new(),
                        aliases: Vec::new(),
                        parent_id: None,
                        depends_on: Vec::new(),
                        created_at_ms: 0,
                        updated_at_ms: 0,
                    },
                    false,
                )
            };

            let bindings = match self
                .store
                .anchor_bindings_list_for_anchor(&workspace, anchor.id.as_str())
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let bindings_json = bindings
                .into_iter()
                .map(|b| {
                    json!({
                        "kind": b.kind,
                        "repo_rel": b.repo_rel,
                        "created_at_ms": b.created_at_ms,
                        "updated_at_ms": b.updated_at_ms
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "workspace": workspace.as_str(),
                "kind": "anchor",
                "id": anchor_id,
                "anchor": {
                    "id": anchor.id,
                    "title": anchor.title,
                    "kind": anchor.kind,
                    "status": anchor.status,
                    "description": anchor.description,
                    "refs": anchor.refs,
                    "bindings": bindings_json,
                    "aliases": anchor.aliases,
                    "parent_id": anchor.parent_id,
                    "depends_on": anchor.depends_on,
                    "created_at_ms": anchor.created_at_ms,
                    "updated_at_ms": anchor.updated_at_ms,
                    "registered": registered
                },
                "stats": {
                    "links_count": links.links.len(),
                    "links_has_more": links.has_more
                },
                "cards": cards,
                "count": cards.len(),
                "truncated": false
            })
        } else if let Some(runner_id) = parse_runner_ref(&id) {
            match open_runner_ref(self, &workspace, runner_id, &mut suggestions) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else if is_step_id(&id) {
            let located = match self.store.step_locate(&workspace, &id) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some((task_id, step)) = located else {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown step id",
                    Some("Copy a STEP-* id from tasks.resume.super(step_focus.step.step_id)."),
                    vec![],
                );
            };

            let mut extra = serde_json::Map::new();
            extra.insert("step_id".to_string(), Value::String(step.step_id.clone()));

            let (mut out, extra_warnings, extra_suggestions) = match open_target_via_resume_super(
                self,
                &workspace,
                OpenTargetViaResumeSuperArgs {
                    open_id: &id,
                    target_kind: "step",
                    target_key: "task",
                    target_id: &task_id,
                    include_drafts,
                    include_content,
                    max_chars,
                    limit,
                    limit_explicit: args_obj.contains_key("limit"),
                    extra_resume_args: Some(extra),
                },
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            warnings.extend(extra_warnings);
            suggestions.extend(extra_suggestions);

            if let Some(obj) = out.as_object_mut() {
                obj.insert("task_id".to_string(), Value::String(task_id));
                obj.insert(
                    "step".to_string(),
                    json!({ "step_id": step.step_id, "path": step.path }),
                );
            }
            out
        } else if let Some((task_raw, path_raw)) = id.split_once('@')
            && is_task_id(task_raw)
        {
            let task_id = task_raw.trim();
            let path_str = path_raw.trim();

            if StepPath::parse(path_str).is_err() {
                return ai_error_with(
                    "INVALID_INPUT",
                    "Invalid step path",
                    Some("Expected TASK-###@s:n[.s:m...] (e.g. TASK-001@s:0)."),
                    vec![],
                );
            }

            let mut extra = serde_json::Map::new();
            extra.insert("path".to_string(), Value::String(path_str.to_string()));

            let (mut out, extra_warnings, extra_suggestions) = match open_target_via_resume_super(
                self,
                &workspace,
                OpenTargetViaResumeSuperArgs {
                    open_id: &id,
                    target_kind: "step",
                    target_key: "task",
                    target_id: task_id,
                    include_drafts,
                    include_content,
                    max_chars,
                    limit,
                    limit_explicit: args_obj.contains_key("limit"),
                    extra_resume_args: Some(extra),
                },
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            warnings.extend(extra_warnings);
            suggestions.extend(extra_suggestions);

            if let Some(obj) = out.as_object_mut() {
                obj.insert("task_id".to_string(), Value::String(task_id.to_string()));
                obj.insert("path".to_string(), Value::String(path_str.to_string()));
            }
            out
        } else if is_task_or_plan_id(&id) {
            let is_task = id.starts_with("TASK-");
            let target_key = if is_task { "task" } else { "plan" };
            let target_kind = if is_task { "task" } else { "plan" };

            let (out, extra_warnings, extra_suggestions) = match open_target_via_resume_super(
                self,
                &workspace,
                OpenTargetViaResumeSuperArgs {
                    open_id: &id,
                    target_kind,
                    target_key,
                    target_id: &id,
                    include_drafts,
                    include_content,
                    max_chars,
                    limit,
                    limit_explicit: args_obj.contains_key("limit"),
                    extra_resume_args: None,
                },
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            warnings.extend(extra_warnings);
            suggestions.extend(extra_suggestions);
            out
        } else if is_slice_id(&id) {
            match open_slice(self, &workspace, &id, include_content) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else if let Some((job_id, seq)) = parse_job_event_ref(&id) {
            match open_job_event_ref(
                self,
                &workspace,
                OpenJobEventRefArgs {
                    ref_str: &id,
                    job_id: &job_id,
                    seq,
                    include_drafts,
                    limit,
                },
                &mut suggestions,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else if id.starts_with("JOB-") {
            match open_job(
                self,
                &workspace,
                &id,
                include_drafts,
                limit,
                &mut suggestions,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else if id.starts_with("CARD-") {
            match open_card(self, &workspace, &id, include_content) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else if let Some((doc, seq)) = parse_doc_entry_ref(&id) {
            match open_doc_entry_ref(self, &workspace, &id, doc, seq) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else {
            return ai_error_with(
                "INVALID_INPUT",
                "Unsupported open id format",
                Some(
                    "Supported: CARD-..., <doc>@<seq> (e.g. notes@123), a:<anchor>, runner:<id>, SLC-..., STEP-..., TASK-..., TASK-...@s:n[.s:m...], PLAN-..., JOB-..., JOB-...@<seq>, artifact://jobs/JOB-.../<artifact_key>.",
                ),
                vec![],
            );
        };

        if let Some(jump) = jump
            && let Some(obj) = result.as_object_mut()
        {
            obj.insert("jump".to_string(), jump);
        }

        redact_value(&mut result, 6);
        result = apply_open_budget_and_verbosity(result, &id, verbosity, max_chars, &mut warnings);
        dedupe_warnings_by_code(&mut warnings);

        if warnings.is_empty() {
            ai_ok_with("open", result, suggestions)
        } else {
            ai_ok_with_warnings("open", result, warnings, suggestions)
        }
    }
}
