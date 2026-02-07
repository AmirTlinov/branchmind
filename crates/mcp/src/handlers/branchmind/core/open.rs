#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

fn is_anchor_id(raw: &str) -> bool {
    raw.trim().to_ascii_lowercase().starts_with("a:")
}

fn is_anchor_tag_any(tag: &str, anchor_ids: &[String]) -> bool {
    let tag = tag.trim();
    if tag.is_empty() {
        return false;
    }
    anchor_ids
        .iter()
        .any(|id| tag.eq_ignore_ascii_case(id.as_str()))
}

fn anchor_title_from_id(anchor_id: &str) -> String {
    let raw = anchor_id.trim();
    let Some(slug) = raw.strip_prefix("a:").or_else(|| raw.strip_prefix("A:")) else {
        return "Anchor".to_string();
    };
    let words = slug
        .split('-')
        .filter(|w| !w.trim().is_empty())
        .map(|w| {
            let mut chars = w.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
            out
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        "Anchor".to_string()
    } else {
        words.join(" ")
    }
}

fn parse_response_verbosity(
    args_obj: &serde_json::Map<String, Value>,
    fallback: ResponseVerbosity,
) -> Result<ResponseVerbosity, Value> {
    let raw = match optional_string(args_obj, "verbosity")? {
        Some(v) => v,
        None => return Ok(fallback),
    };
    let trimmed = raw.trim();
    ResponseVerbosity::from_str(trimmed)
        .ok_or_else(|| ai_error("INVALID_INPUT", "verbosity must be one of: full|compact"))
}

fn compact_open_result(id: &str, result: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("id".to_string(), Value::String(id.to_string()));
    if let Some(workspace) = result.get("workspace") {
        out.insert("workspace".to_string(), workspace.clone());
    }
    if let Some(kind) = result.get("kind") {
        out.insert("kind".to_string(), kind.clone());
    }
    if let Some(ref_val) = result.get("ref") {
        out.insert("ref".to_string(), ref_val.clone());
    }
    if let Some(budget) = result.get("budget") {
        out.insert("budget".to_string(), budget.clone());
    }
    if let Some(truncated) = result.get("truncated") {
        out.insert("truncated".to_string(), truncated.clone());
    }
    if let Some(reasoning_ref) = result.get("reasoning_ref") {
        out.insert("reasoning_ref".to_string(), reasoning_ref.clone());
    }
    if let Some(content) = result.get("content") {
        out.insert("content".to_string(), content.clone());
    }
    if let Some(card) = result.get("card") {
        if let Some(card_id) = card.get("id") {
            out.insert("card_id".to_string(), card_id.clone());
        }
        if let Some(card_type) = card.get("type") {
            out.insert("card_type".to_string(), card_type.clone());
        }
    }
    if let Some(entry) = result.get("entry") {
        if let Some(doc) = entry.get("doc") {
            out.insert("entry_doc".to_string(), doc.clone());
        }
        if let Some(seq) = entry.get("seq") {
            out.insert("entry_seq".to_string(), seq.clone());
        }
    }
    if let Some(capsule) = result.get("capsule") {
        if let Some(focus) = capsule.get("focus") {
            out.insert("focus".to_string(), focus.clone());
        }
        if let Some(action) = capsule.get("action") {
            let mut action_out = serde_json::Map::new();
            if let Some(tool) = action.get("tool") {
                action_out.insert("tool".to_string(), tool.clone());
            }
            if let Some(args) = action.get("args") {
                action_out.insert("args".to_string(), args.clone());
            }
            if !action_out.is_empty() {
                out.insert("next_action".to_string(), Value::Object(action_out));
            }
        }
    }
    Value::Object(out)
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

fn parse_doc_entry_ref(raw: &str) -> Option<(String, i64)> {
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

fn parse_job_event_ref(raw: &str) -> Option<(String, i64)> {
    let raw = raw.trim();
    let (job_id, seq_str) = raw.rsplit_once('@')?;
    let job_id = job_id.trim();
    let seq_str = seq_str.trim();
    if job_id.is_empty() || seq_str.is_empty() {
        return None;
    }
    if !job_id.starts_with("JOB-") {
        return None;
    }
    if !job_id
        .trim_start_matches("JOB-")
        .chars()
        .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let seq = seq_str.parse::<i64>().ok()?;
    if seq <= 0 {
        return None;
    }
    Some((job_id.to_string(), seq))
}

fn parse_runner_ref(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let prefix = "runner:";
    if raw.len() <= prefix.len() {
        return None;
    }
    if !raw[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }
    let runner_id = raw[prefix.len()..].trim();
    if runner_id.is_empty() {
        return None;
    }
    Some(runner_id.to_string())
}

fn is_task_or_plan_id(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.contains('@') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix("TASK-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    if let Some(rest) = raw.strip_prefix("PLAN-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}

fn is_task_id(raw: &str) -> bool {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("TASK-") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}

fn is_step_id(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.contains('@') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix("STEP-") {
        // Step IDs are generated as a fixed-width uppercase hex counter (e.g. STEP-0000000A),
        // so they may include A-F. Accept ASCII hex digits to keep `open STEP-*` stable.
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit());
    }
    false
}

fn summary_one_line(text: Option<&str>, title: Option<&str>, max_len: usize) -> String {
    let title = title.unwrap_or("").trim();
    if !title.is_empty() {
        return truncate_string(&redact_text(title), max_len);
    }
    let text = text.unwrap_or("").trim();
    if text.is_empty() {
        return String::new();
    }
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or(text);
    truncate_string(&redact_text(first.trim()), max_len)
}

struct OpenTargetViaResumeSuperArgs<'a> {
    open_id: &'a str,
    target_kind: &'a str,
    target_key: &'a str,
    target_id: &'a str,
    include_drafts: bool,
    include_content: bool,
    max_chars: Option<usize>,
    limit: usize,
    limit_explicit: bool,
    extra_resume_args: Option<serde_json::Map<String, Value>>,
}

fn open_target_via_resume_super(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    args: OpenTargetViaResumeSuperArgs<'_>,
) -> Result<(Value, Vec<Value>, Vec<Value>), Value> {
    // `open` is read-only by contract. For targets, delegate to the existing
    // super-resume machinery (budget-aware + deterministic), but shape it
    // into a small navigation-friendly payload.
    let resume_max_chars = args.max_chars.unwrap_or(12_000);
    let resume_max_chars = resume_max_chars.saturating_sub(1_200).max(2_000);
    let resume_max_chars = args
        .max_chars
        .map(|cap| resume_max_chars.min(cap))
        .unwrap_or(resume_max_chars);

    let mut resume_args = serde_json::Map::new();
    resume_args.insert(
        "workspace".to_string(),
        Value::String(workspace.as_str().to_string()),
    );
    resume_args.insert(
        args.target_key.to_string(),
        Value::String(args.target_id.to_string()),
    );
    resume_args.insert("read_only".to_string(), Value::Bool(true));
    resume_args.insert(
        "view".to_string(),
        Value::String(if args.include_drafts {
            "audit".to_string()
        } else {
            "focus_only".to_string()
        }),
    );
    resume_args.insert(
        "max_chars".to_string(),
        Value::Number(serde_json::Number::from(resume_max_chars as i64)),
    );
    if args.limit_explicit {
        resume_args.insert(
            "cards_limit".to_string(),
            Value::Number(serde_json::Number::from(args.limit as i64)),
        );
    }
    if let Some(extra) = args.extra_resume_args {
        resume_args.extend(extra);
    }

    let resume_resp = server.tool_tasks_resume_super(Value::Object(resume_args));
    if !resume_resp
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(resume_resp);
    }

    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();
    if let Some(extra) = resume_resp.get("warnings").and_then(|v| v.as_array()) {
        warnings.extend(extra.iter().cloned());
    }
    if let Some(extra) = resume_resp.get("suggestions").and_then(|v| v.as_array()) {
        suggestions.extend(extra.iter().cloned());
    }

    let resume = resume_resp.get("result").cloned().unwrap_or(Value::Null);
    let truncated = resume
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut out = json!({
        "workspace": workspace.as_str(),
        "kind": args.target_kind,
        "id": args.open_id,
        "target": resume.get("target").cloned().unwrap_or(Value::Null),
        "reasoning_ref": resume.get("reasoning_ref").cloned().unwrap_or(Value::Null),
        "budget": resume.get("budget").cloned().unwrap_or(Value::Null),
        "capsule": resume.get("capsule").cloned().unwrap_or(Value::Null),
        "step_focus": resume.get("step_focus").cloned().unwrap_or(Value::Null),
        "degradation": resume.get("degradation").cloned().unwrap_or(Value::Null),
        "truncated": truncated
    });

    // Portal UX: optionally include the most-used content blocks for the target so
    // agents don't have to bounce between `open` and `tasks.snapshot` for the common
    // “what’s next + what changed” loop.
    if args.include_content
        && let Some(obj) = out.as_object_mut()
    {
        let mut content = serde_json::Map::new();
        for key in [
            "radar",
            "steps",
            "signals",
            "memory",
            "timeline",
            "graph_diff",
        ] {
            if let Some(v) = resume.get(key) {
                content.insert(key.to_string(), v.clone());
            }
        }
        obj.insert("content".to_string(), Value::Object(content));
    }

    Ok((out, warnings, suggestions))
}

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
        let verbosity = match parse_response_verbosity(args_obj, self.response_verbosity) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let id = id.trim().to_string();
        if id.is_empty() {
            return ai_error("INVALID_INPUT", "id must not be empty");
        }

        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();

        let mut result = if is_anchor_id(&id) {
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
            let now_ms = crate::support::now_ms_i64();
            let lease = match self
                .store
                .runner_lease_get(&workspace, bm_storage::RunnerLeaseGetRequest { runner_id })
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(lease) = lease else {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown runner id",
                    Some(
                        "Copy a runner:<id> ref from tasks_jobs_radar runner lines or ensure the runner is heartbeating.",
                    ),
                    vec![],
                );
            };

            let lease_active = lease.lease.lease_expires_at_ms > now_ms;
            let effective_status = if lease_active {
                lease.lease.status.clone()
            } else {
                "offline".to_string()
            };
            let expires_in_ms = lease
                .lease
                .lease_expires_at_ms
                .saturating_sub(now_ms)
                .max(0);

            if let Some(job_id) = lease.lease.active_job_id.as_deref() {
                suggestions.push(json!({
                    "tool": "open",
                    "reason": "Open the active job for this runner",
                    "args_hint": {
                        "workspace": workspace.as_str(),
                        "id": job_id
                    }
                }));
            }

            json!({
                "workspace": workspace.as_str(),
                "kind": "runner",
                "id": format!("runner:{}", lease.lease.runner_id),
                "status": effective_status,
                "lease": {
                    "runner_id": lease.lease.runner_id,
                    "status": lease.lease.status,
                    "active_job_id": lease.lease.active_job_id,
                    "lease_expires_at_ms": lease.lease.lease_expires_at_ms,
                    "created_at_ms": lease.lease.created_at_ms,
                    "updated_at_ms": lease.lease.updated_at_ms,
                    "lease_active": lease_active,
                    "expires_in_ms": expires_in_ms
                },
                "meta": lease
                    .meta_json
                    .as_ref()
                    .map(|raw| parse_json_or_string(raw))
                    .unwrap_or(Value::Null),
                "truncated": false
            })
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
        } else if let Some((job_id, seq)) = parse_job_event_ref(&id) {
            let job_row = match self
                .store
                .job_get(&workspace, bm_storage::JobGetRequest { id: job_id.clone() })
            {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(job) = job_row else {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown job id",
                    Some("Copy a JOB-* id from tasks_snapshot or tasks_jobs_list."),
                    vec![],
                );
            };

            let event_row = match self.store.job_event_get(
                &workspace,
                bm_storage::JobEventGetRequest {
                    job_id: job_id.clone(),
                    seq,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(event) = event_row else {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown job event ref",
                    Some("Open the job and copy a valid event seq."),
                    vec![],
                );
            };

            let job_ref = format!("{job_id}@{seq}");
            let mut event_json = json!({
                "ref": job_ref,
                "seq": event.seq,
                "ts": ts_ms_to_rfc3339(event.ts_ms),
                "ts_ms": event.ts_ms,
                "kind": event.kind,
                "message": event.message,
                "percent": event.percent,
                "refs": event.refs
            });
            if let Some(meta_json) = event.meta_json.as_deref()
                && let Ok(meta) = serde_json::from_str::<Value>(meta_json)
                && let Some(obj) = event_json.as_object_mut()
            {
                obj.insert("meta".to_string(), meta);
            }

            let max_events = limit.clamp(1, 50);
            let ctx = match self.store.job_open(
                &workspace,
                bm_storage::JobOpenRequest {
                    id: job_id.clone(),
                    include_prompt: false,
                    include_events: true,
                    include_meta: false,
                    max_events,
                    before_seq: Some(seq.saturating_add(1)),
                },
            ) {
                Ok(v) => v,
                Err(_) => bm_storage::JobOpenResult {
                    job: job.clone(),
                    prompt: None,
                    meta_json: None,
                    events: Vec::new(),
                    has_more_events: false,
                },
            };

            let ctx_events = ctx
                .events
                .iter()
                .map(|e| {
                    let job_ref = format!("{}@{}", e.job_id, e.seq);
                    json!({
                        "ref": job_ref,
                        "seq": e.seq,
                        "ts": ts_ms_to_rfc3339(e.ts_ms),
                        "ts_ms": e.ts_ms,
                        "kind": e.kind,
                        "message": e.message,
                        "percent": e.percent,
                        "refs": e.refs
                    })
                })
                .collect::<Vec<_>>();

            let ctx_count = ctx_events.len();

            suggestions.push(json!({
                "tool": "tasks_jobs_tail",
                "reason": "Follow job events incrementally (no lose-place loops)",
                "args_hint": {
                    "workspace": workspace.as_str(),
                    "job": job_id.as_str(),
                    "after_seq": seq,
                    "limit": 50,
                    "max_chars": 4000
                }
            }));

            suggestions.push(json!({
                "tool": "tasks_jobs_open",
                "reason": "Open the job (status + prompt + recent events)",
                "args_hint": {
                    "workspace": workspace.as_str(),
                    "job": job_id.as_str(),
                    "include_prompt": include_drafts,
                    "include_events": true,
                    "max_events": max_events,
                    "max_chars": 8000
                }
            }));

            json!({
                "workspace": workspace.as_str(),
                "kind": "job_event",
                "ref": id,
                "job": {
                    "id": job.id,
                    "revision": job.revision,
                    "status": job.status,
                    "title": job.title,
                    "kind": job.kind,
                    "priority": job.priority,
                    "task_id": job.task_id,
                    "anchor_id": job.anchor_id,
                    "runner": job.runner,
                    "summary": job.summary,
                    "created_at_ms": job.created_at_ms,
                    "updated_at_ms": job.updated_at_ms,
                    "completed_at_ms": job.completed_at_ms
                },
                "event": event_json,
                "context": {
                    "events": ctx_events,
                    "count": ctx_count,
                    "has_more_events": ctx.has_more_events
                },
                "truncated": false
            })
        } else if id.starts_with("JOB-") {
            let max_events = limit.clamp(1, 50);
            let opened = match self.store.job_open(
                &workspace,
                bm_storage::JobOpenRequest {
                    id: id.clone(),
                    include_prompt: include_drafts,
                    include_events: true,
                    include_meta: true,
                    max_events,
                    before_seq: None,
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(StoreError::UnknownId) => {
                    return ai_error_with(
                        "UNKNOWN_ID",
                        "Unknown job id",
                        Some("Copy a JOB-* id from tasks_snapshot or tasks_jobs_list."),
                        vec![],
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let events = opened
                .events
                .iter()
                .map(|e| {
                    let job_ref = format!("{}@{}", e.job_id, e.seq);
                    json!({
                        "ref": job_ref,
                        "seq": e.seq,
                        "ts": ts_ms_to_rfc3339(e.ts_ms),
                        "ts_ms": e.ts_ms,
                        "kind": e.kind,
                        "message": e.message,
                        "percent": e.percent,
                        "refs": e.refs
                    })
                })
                .collect::<Vec<_>>();

            if !include_drafts {
                suggestions.push(json!({
                    "tool": "tasks_jobs_open",
                    "reason": "Open full job spec (prompt) and more events",
                    "args_hint": {
                        "workspace": workspace.as_str(),
                        "job": id,
                        "include_prompt": true,
                        "include_events": true,
                        "max_events": max_events
                    }
                }));
            }

            if opened.has_more_events
                && let Some(oldest) = opened.events.last()
            {
                suggestions.push(json!({
                    "tool": "tasks_jobs_open",
                    "reason": "Page older job events",
                    "args_hint": {
                        "workspace": workspace.as_str(),
                        "job": id,
                        "include_prompt": false,
                        "include_events": true,
                        "max_events": max_events,
                        "before_seq": oldest.seq
                    }
                }));
            }

            json!({
                "workspace": workspace.as_str(),
                "kind": "job",
                "id": id,
                "job": {
                    "id": opened.job.id,
                    "revision": opened.job.revision,
                    "status": opened.job.status,
                    "title": opened.job.title,
                    "kind": opened.job.kind,
                    "priority": opened.job.priority,
                    "task_id": opened.job.task_id,
                    "anchor_id": opened.job.anchor_id,
                    "runner": opened.job.runner,
                    "summary": opened.job.summary,
                    "created_at_ms": opened.job.created_at_ms,
                    "updated_at_ms": opened.job.updated_at_ms,
                    "completed_at_ms": opened.job.completed_at_ms
                },
                "prompt": opened.prompt,
                "meta": opened.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                "events": events,
                "has_more_events": opened.has_more_events,
                "truncated": false
            })
        } else if id.starts_with("CARD-") {
            let opened = match self.store.graph_card_open_by_id(&workspace, &id) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(StoreError::UnknownBranch) => {
                    return ai_error("UNKNOWN_ID", "Unknown branch for the requested card");
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(opened) = opened else {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown card id",
                    Some("Copy a CARD-* id from snapshot delta or a prior think_* response."),
                    vec![],
                );
            };

            let card = json!({
                "id": opened.node.id,
                "type": opened.node.node_type,
                "title": opened.node.title,
                "text": opened.node.text,
                "status": opened.node.status.unwrap_or_else(|| "open".to_string()),
                "tags": opened.node.tags,
                "meta": opened.node.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
            });

            let mut out = json!({
                "workspace": workspace.as_str(),
                "kind": "card",
                "id": id,
                "head": {
                    "seq": opened.head.seq,
                    "ts": ts_ms_to_rfc3339(opened.head.ts_ms),
                    "ts_ms": opened.head.ts_ms,
                    "branch": opened.head.branch,
                    "doc": opened.head.doc
                },
                "card": card,
                "edges": {
                    "supports": opened.supports,
                    "blocks": opened.blocks
                },
                "summary": summary_one_line(
                    opened.node.text.as_deref(),
                    opened.node.title.as_deref(),
                    120
                ),
                "truncated": false
            });

            if include_content && let Some(obj) = out.as_object_mut() {
                obj.insert(
                    "content".to_string(),
                    json!({
                        "title": card.get("title").cloned().unwrap_or(Value::Null),
                        "text": card.get("text").cloned().unwrap_or(Value::Null)
                    }),
                );
            }

            out
        } else if let Some((doc, seq)) = parse_doc_entry_ref(&id) {
            let entry = match self.store.doc_entry_get_by_seq(&workspace, seq) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let Some(entry) = entry else {
                return ai_error_with(
                    "UNKNOWN_ID",
                    "Unknown doc entry ref",
                    Some(
                        "Copy a <doc>@<seq> ref from snapshot delta or a prior notes_commit/think_* response.",
                    ),
                    vec![],
                );
            };
            if entry.doc != doc {
                return ai_error_with(
                    "INVALID_INPUT",
                    "Doc prefix mismatch for ref",
                    Some(&format!("Expected {}@{}", entry.doc, entry.seq)),
                    vec![],
                );
            }

            json!({
                "workspace": workspace.as_str(),
                "kind": "doc_entry",
                "ref": id,
                "entry": {
                    "seq": entry.seq,
                    "ts": ts_ms_to_rfc3339(entry.ts_ms),
                    "ts_ms": entry.ts_ms,
                    "branch": entry.branch,
                    "doc": entry.doc,
                    "kind": entry.kind.as_str(),
                    "title": entry.title,
                    "format": entry.format,
                    "meta": entry.meta_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                    "content": entry.content,
                    "payload": entry.payload_json.as_ref().map(|raw| parse_json_or_string(raw)).unwrap_or(Value::Null),
                },
                "summary": summary_one_line(
                    entry.content.as_deref(),
                    entry.title.as_deref(),
                    120
                ),
                "truncated": false
            })
        } else {
            return ai_error_with(
                "INVALID_INPUT",
                "Unsupported open id format",
                Some(
                    "Supported: CARD-..., <doc>@<seq> (e.g. notes@123), a:<anchor>, runner:<id>, STEP-..., TASK-..., TASK-...@s:n[.s:m...], PLAN-..., JOB-....",
                ),
                vec![],
            );
        };

        redact_value(&mut result, 6);

        if verbosity == ResponseVerbosity::Compact {
            result = compact_open_result(&id, &result);
            if let Some(limit) = max_chars {
                let (limit, clamped) = clamp_budget_max(limit);
                let mut truncated = false;
                let mut minimal = false;

                let _used =
                    ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |v| {
                        let mut changed = false;
                        // Compact open results should keep navigation handles; drop payload-heavy
                        // fields first.
                        if json_len_chars(v) > limit {
                            changed |= drop_fields_at(v, &[], &["content"]);
                        }
                        if json_len_chars(v) > limit {
                            changed |= drop_fields_at(v, &[], &["next_action"]);
                        }
                        if json_len_chars(v) > limit {
                            changed |= drop_fields_at(v, &[], &["reasoning_ref"]);
                        }
                        if json_len_chars(v) > limit {
                            changed |= drop_fields_at(v, &[], &["focus"]);
                        }
                        changed
                    });

                warnings.extend(budget_warnings(truncated, minimal, clamped));
            } else if result.get("budget").is_none() {
                // Some open kinds don't go through the budget-aware super-resume machinery.
                // Ensure budget is visible even in compact output (UX invariant).
                let used = json_len_chars(&result);
                let (limit, _clamped) = clamp_budget_max(used);
                let _used = attach_budget(&mut result, limit, false);
            }
        } else if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |v| {
                    let mut changed = false;
                    // Prefer dropping the heaviest content fields first.
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["card"], &["text"]);
                        changed |= drop_fields_at(v, &["entry"], &["content"]);
                        changed |= drop_fields_at(v, &["entry"], &["payload"]);
                        changed |= drop_fields_at(v, &[], &["prompt"]);
                        changed |= drop_fields_at(v, &["content", "memory"], &["cards"]);
                        changed |=
                            drop_fields_at(v, &["content", "memory", "trace"], &["sequential"]);
                        changed |= drop_fields_at(v, &["content", "memory", "trace"], &["entries"]);
                        changed |= drop_fields_at(v, &["content", "memory", "notes"], &["entries"]);
                        changed |= drop_fields_at(v, &["content", "timeline"], &["events"]);
                        changed |= compact_card_fields_at(v, &["cards"], 160, true, false, true);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["card"], &["meta"]);
                        changed |= drop_fields_at(v, &["entry"], &["meta"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &[], &["step_focus"]);
                        changed |= drop_fields_at(v, &[], &["degradation"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["edges"], &["supports", "blocks"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["card"], &["tags"]);
                    }
                    if json_len_chars(v) > limit {
                        if let Some(events) = v.get_mut("events").and_then(|vv| vv.as_array_mut()) {
                            for ev in events.iter_mut() {
                                if let Some(msg) = ev.get("message").and_then(|vv| vv.as_str()) {
                                    let msg = truncate_string(&redact_text(msg), 140);
                                    if let Some(obj) = ev.as_object_mut() {
                                        obj.insert("message".to_string(), Value::String(msg));
                                    }
                                }
                            }
                            changed = true;
                        }
                        if json_len_chars(v) > limit
                            && let Some(events) =
                                v.get_mut("events").and_then(|vv| vv.as_array_mut())
                        {
                            for ev in events.iter_mut() {
                                if let Some(obj) = ev.as_object_mut() {
                                    obj.remove("refs");
                                }
                            }
                            changed = true;
                        }
                    }
                    if json_len_chars(v) > limit {
                        let (_used, truncated_cards) = enforce_graph_list_budget(v, "cards", limit);
                        if truncated_cards {
                            changed = true;
                        }
                        let (_used, truncated_events) =
                            enforce_graph_list_budget(v, "events", limit);
                        if truncated_events {
                            changed = true;
                        }
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &[], &["cards"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &[], &["events"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["content"], &["signals"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["content"], &["steps"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["content"], &["radar"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &["anchor"], &["description", "refs"]);
                    }
                    if json_len_chars(v) > limit {
                        changed |= drop_fields_at(v, &[], &["stats"]);
                    }
                    changed
                });

            warnings.extend(budget_warnings(truncated, minimal, clamped));
        } else if result.get("budget").is_none() {
            // For open calls without explicit budgets, keep the payload stable but still report the
            // effective size to the caller (cheap drift guard + UX).
            let used = json_len_chars(&result);
            let (limit, _clamped) = clamp_budget_max(used);
            let _used = attach_budget(&mut result, limit, false);
        }

        if warnings.is_empty() {
            ai_ok_with("open", result, suggestions)
        } else {
            ai_ok_with_warnings("open", result, warnings, suggestions)
        }
    }
}
