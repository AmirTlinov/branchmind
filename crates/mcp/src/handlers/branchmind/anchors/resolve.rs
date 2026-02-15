#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

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

fn suggest_anchor_id_for_repo_rel(repo_rel: &str) -> String {
    let base = if repo_rel == "." {
        "root"
    } else {
        repo_rel.rsplit('/').next().unwrap_or(repo_rel)
    };
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in base.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
            continue;
        }
        if matches!(lc, '-' | '_' | '.' | ' ') {
            if !out.is_empty() && !prev_dash {
                out.push('-');
                prev_dash = true;
            }
            continue;
        }
        if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let mut slug = if trimmed.is_empty() {
        "component".to_string()
    } else {
        trimmed.to_string()
    };
    if slug.len() > 64 {
        slug.truncate(64);
        slug = slug.trim_matches('-').to_string();
        if slug.is_empty() {
            slug = "component".to_string();
        }
    }
    format!("a:{slug}")
}

fn repo_rel_prefixes(repo_rel: &str) -> Vec<String> {
    let repo_rel = repo_rel.trim();
    if repo_rel == "." {
        return vec![".".to_string()];
    }
    let parts = repo_rel
        .split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return vec![".".to_string()];
    }
    let mut out = Vec::<String>::new();
    for i in (1..=parts.len()).rev() {
        out.push(parts[0..i].join("/"));
    }
    out.push(".".to_string());
    out
}

fn score_depth(repo_rel: &str) -> usize {
    if repo_rel.trim() == "." {
        0
    } else {
        repo_rel.split('/').filter(|p| !p.is_empty()).count()
    }
}

impl McpServer {
    pub(crate) fn tool_branchmind_anchor_resolve(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let path_raw = match require_string(args_obj, "path") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(20).clamp(1, 50),
            Err(resp) => return resp,
        };

        let workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists && let Err(err) = self.store.workspace_init(&workspace) {
            return ai_error("STORE_ERROR", &format_store_error(err));
        }

        let repo_root = match self.store.workspace_path_primary_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let repo_root_path = repo_root.as_deref().map(Path::new);

        let repo_rel = match repo_rel_from_path_input(&path_raw, repo_root_path) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let prefixes = repo_rel_prefixes(&repo_rel);

        let lookup = match self.store.anchor_bindings_lookup_any(
            &workspace,
            bm_storage::AnchorBindingsLookupAnyRequest {
                repo_rels: prefixes.clone(),
                limit: limit.saturating_mul(4).clamp(1, 200),
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut anchor_cache = BTreeMap::<String, Value>::new();
        for hit in &lookup.bindings {
            if anchor_cache.contains_key(&hit.anchor_id) {
                continue;
            }
            let row = match self.store.anchor_get(
                &workspace,
                bm_storage::AnchorGetRequest {
                    id: hit.anchor_id.clone(),
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            let summary = if let Some(a) = row {
                json!({
                    "id": a.id,
                    "title": a.title,
                    "kind": a.kind,
                    "status": a.status
                })
            } else {
                json!({
                    "id": hit.anchor_id,
                    "title": anchor_title_from_id(&hit.anchor_id),
                    "kind": Value::Null,
                    "status": Value::Null
                })
            };
            let id = summary
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            anchor_cache.insert(id, summary);
        }

        let candidates = lookup
            .bindings
            .iter()
            .map(|hit| {
                let anchor = anchor_cache
                    .get(&hit.anchor_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({
                            "id": hit.anchor_id,
                            "title": anchor_title_from_id(&hit.anchor_id),
                            "kind": Value::Null,
                            "status": Value::Null
                        })
                    });
                let depth = score_depth(&hit.repo_rel);
                let exact = hit.repo_rel == repo_rel;
                json!({
                    "anchor": anchor,
                    "binding": {
                        "kind": hit.kind,
                        "repo_rel": hit.repo_rel,
                        "created_at_ms": hit.created_at_ms,
                        "updated_at_ms": hit.updated_at_ms
                    },
                    "score": {
                        "depth": depth,
                        "exact": exact
                    }
                })
            })
            .collect::<Vec<_>>();

        let best = candidates.first().cloned();

        let mut suggestions = Vec::<Value>::new();
        if let Some(best) = best.as_ref()
            && let Some(anchor_id) = best
                .get("anchor")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
        {
            suggestions.push(suggest_call(
                "open",
                "Open the resolved anchor.",
                "high",
                json!({ "workspace": workspace.as_str(), "id": anchor_id }),
            ));
        } else {
            let anchor_id = suggest_anchor_id_for_repo_rel(&repo_rel);
            let title = anchor_title_from_id(&anchor_id);
            suggestions.push(suggest_call(
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
            ));
        }

        let result = json!({
            "workspace": workspace.as_str(),
            "path": {
                "input": path_raw,
                "repo_rel": repo_rel,
                "bound_root": repo_root
            },
            "match": {
                "prefixes": prefixes
            },
            "best": best,
            "candidates": candidates,
            "count": candidates.len(),
            "truncated": false
        });

        ai_ok_with_warnings("anchor_resolve", result, Vec::new(), suggestions)
    }
}
