#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const DEFAULT_IGNORE_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".cache",
    ".next",
    ".venv",
    "venv",
];

const DEFAULT_CONTAINERS: &[&str] = &["crates", "apps", "services", "packages", "libs"];

#[derive(Clone, Debug)]
struct AtlasCandidate {
    repo_rel: String,
    container: Option<String>,
    title: String,
    kind: String,
    confidence: &'static str,
    reason: String,
    anchor_id: String,
}

fn slugify(raw: &str) -> String {
    let raw = raw.trim();
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
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
    slug
}

fn title_case(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return "Component".to_string();
    }
    let parts = raw
        .split(['-', '_', '.', ' ', '/'])
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>();
    let mut words = Vec::<String>::new();
    for part in parts {
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        let mut w = String::new();
        w.push(first.to_ascii_uppercase());
        w.push_str(chars.as_str());
        if !w.is_empty() {
            words.push(w);
        }
    }
    if words.is_empty() {
        "Component".to_string()
    } else {
        words.join(" ")
    }
}

fn kind_for_repo_rel(repo_rel: &str) -> &'static str {
    let first = repo_rel
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match first.as_str() {
        "infra" | "ops" | "deploy" | "k8s" | "terraform" | "helm" | "ansible" => "ops",
        "contracts" | "contract" | "openapi" | "schemas" | "spec" | "specs" => "contract",
        "data" | "db" | "migrations" => "data",
        "tests" | "test" => "test-surface",
        _ => "component",
    }
}

fn expand_tilde(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if (raw == "~" || raw.starts_with("~/"))
        && let Some(home) = std::env::var_os("HOME")
    {
        let home = PathBuf::from(home);
        if raw == "~" {
            return home;
        }
        return home.join(raw.trim_start_matches("~/"));
    }
    PathBuf::from(raw)
}

fn list_immediate_subdirs(root: &Path, ignore: &BTreeSet<String>) -> Result<Vec<String>, Value> {
    let mut out = Vec::<String>::new();
    let iter = std::fs::read_dir(root)
        .map_err(|_| ai_error("INVALID_INPUT", "repo_root must be a readable directory"))?;
    for entry in iter {
        let entry =
            entry.map_err(|_| ai_error("INVALID_INPUT", "repo_root directory scan failed"))?;
        let ft = entry
            .file_type()
            .map_err(|_| ai_error("INVALID_INPUT", "repo_root directory scan failed"))?;
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        if ignore.contains(&name) {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

impl McpServer {
    pub(crate) fn tool_branchmind_atlas_suggest(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let repo_root_override = match optional_string(args_obj, "repo_root") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };

        let granularity = match optional_string(args_obj, "granularity") {
            Ok(v) => v.unwrap_or_else(|| "depth2".to_string()),
            Err(resp) => return resp,
        };
        let granularity = granularity.trim().to_ascii_lowercase();
        if !matches!(granularity.as_str(), "top" | "depth2") {
            return ai_error("INVALID_INPUT", "granularity must be one of: top, depth2");
        }

        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(30).clamp(1, 100),
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let ignore_dirs = match optional_string_array(args_obj, "ignore_dirs") {
            Ok(v) => {
                v.unwrap_or_else(|| DEFAULT_IGNORE_DIRS.iter().map(|s| s.to_string()).collect())
            }
            Err(resp) => return resp,
        };
        let ignore = ignore_dirs
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<BTreeSet<_>>();

        let containers = match optional_string_array(args_obj, "include_containers") {
            Ok(v) => {
                v.unwrap_or_else(|| DEFAULT_CONTAINERS.iter().map(|s| s.to_string()).collect())
            }
            Err(resp) => return resp,
        };
        let containers = containers
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<BTreeSet<_>>();

        let repo_root = if let Some(root) = repo_root_override {
            expand_tilde(&root)
        } else {
            match self.store.workspace_path_primary_get(&workspace) {
                Ok(Some(v)) => PathBuf::from(v),
                Ok(None) => {
                    return ai_error_with(
                        "INVALID_INPUT",
                        "workspace has no bound path; cannot suggest atlas domains",
                        Some(
                            "Bind the workspace to a repo path first (e.g. call status with workspace=\"/path/to/repo\").",
                        ),
                        Vec::new(),
                    );
                }
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            }
        };

        let repo_root = std::fs::canonicalize(&repo_root).unwrap_or(repo_root);
        let md = match std::fs::metadata(&repo_root) {
            Ok(v) => v,
            Err(_) => return ai_error("INVALID_INPUT", "repo_root must exist"),
        };
        if !md.is_dir() {
            return ai_error("INVALID_INPUT", "repo_root must be a directory");
        }

        let mut candidates = Vec::<AtlasCandidate>::new();

        let top_dirs = match list_immediate_subdirs(&repo_root, &ignore) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut container_dirs = Vec::<String>::new();
        let mut real_top = Vec::<String>::new();
        for d in top_dirs {
            if containers.contains(&d) {
                container_dirs.push(d);
            } else {
                real_top.push(d);
            }
        }
        container_dirs.sort();
        real_top.sort();

        for d in real_top {
            let kind = kind_for_repo_rel(&d).to_string();
            candidates.push(AtlasCandidate {
                repo_rel: d.clone(),
                container: None,
                title: title_case(&d),
                kind,
                confidence: "high",
                reason: "top-level directory".to_string(),
                anchor_id: String::new(),
            });
        }

        if granularity == "depth2" {
            for container in container_dirs {
                let container_path = repo_root.join(&container);
                if !container_path.is_dir() {
                    continue;
                }
                let children = match list_immediate_subdirs(&container_path, &ignore) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                for child in children {
                    let repo_rel = format!("{container}/{child}");
                    let title = format!("{} {}", title_case(&container), title_case(&child));
                    candidates.push(AtlasCandidate {
                        repo_rel,
                        container: Some(container.clone()),
                        title,
                        kind: kind_for_repo_rel(&container).to_string(),
                        confidence: "medium",
                        reason: "container child directory".to_string(),
                        anchor_id: String::new(),
                    });
                }
            }
        }

        candidates.sort_by(|a, b| a.repo_rel.cmp(&b.repo_rel));

        let truncated_by_limit = candidates.len() > limit;
        if truncated_by_limit {
            candidates.truncate(limit);
        }

        // Count duplicates by base dir name (last segment), then qualify with container when needed.
        let mut base_counts = BTreeMap::<String, usize>::new();
        for c in &candidates {
            let base = c.repo_rel.rsplit('/').next().unwrap_or(c.repo_rel.as_str());
            let base = slugify(base);
            *base_counts.entry(base).or_insert(0) += 1;
        }

        for c in &mut candidates {
            let base = c.repo_rel.rsplit('/').next().unwrap_or(c.repo_rel.as_str());
            let base_slug = slugify(base);
            let mut slug = base_slug.clone();
            if base_counts.get(&base_slug).copied().unwrap_or(0) > 1
                && let Some(container) = c.container.as_deref()
            {
                slug = format!("{}-{base_slug}", slugify(container));
            }
            c.anchor_id = format!("a:{slug}");
        }

        // Ensure unique ids deterministically (append -2/-3 as needed).
        let mut used = BTreeSet::<String>::new();
        for c in &mut candidates {
            let base_slug = c.anchor_id.trim().strip_prefix("a:").unwrap_or("component");
            let mut id = c.anchor_id.clone();
            if used.contains(&id) {
                let mut n = 2usize;
                loop {
                    id = format!("a:{base_slug}-{n}");
                    if !used.contains(&id) {
                        break;
                    }
                    n += 1;
                }
            }
            used.insert(id.clone());
            c.anchor_id = id;
        }

        let mut proposals = Vec::<Value>::new();
        let mut apply_anchors = Vec::<Value>::new();
        for c in &candidates {
            proposals.push(json!({
                "anchor": c.anchor_id,
                "title": c.title,
                "kind": c.kind,
                "bind_paths": [c.repo_rel],
                "confidence": c.confidence,
                "reason": c.reason
            }));
            apply_anchors.push(json!({
                "anchor": c.anchor_id,
                "title": c.title,
                "kind": c.kind,
                "bind_paths": [c.repo_rel]
            }));
        }

        let suggest_apply = json!({
            "action": "call_tool",
            "target": "macro_atlas_apply",
            "priority": "high",
            "reason": "Apply suggested atlas domains (upsert anchors + bind_paths).",
            "params": { "anchors": apply_anchors }
        });

        let mut result = json!({
            "workspace": workspace.as_str(),
            "repo_root": repo_root.to_string_lossy(),
            "suggestions": proposals,
            "count": proposals.len(),
            "limit": limit,
            "truncated": truncated_by_limit
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) =
                enforce_graph_list_budget(&mut result, "suggestions", limit);
            if budget_truncated || clamped {
                set_truncated_flag(&mut result, true);
            }
        }

        ai_ok_with("atlas_suggest", result, vec![suggest_apply])
    }

    pub(crate) fn tool_branchmind_macro_atlas_apply(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let atomic = match optional_bool(args_obj, "atomic") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };

        let Some(anchors_raw) = args_obj.get("anchors") else {
            return ai_error("INVALID_INPUT", "anchors is required");
        };
        let Some(anchors_arr) = anchors_raw.as_array() else {
            return ai_error("INVALID_INPUT", "anchors must be an array");
        };
        if anchors_arr.is_empty() {
            return ai_error("INVALID_INPUT", "anchors must not be empty");
        }

        let repo_root = match self.store.workspace_path_primary_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let repo_root = repo_root.as_deref().map(Path::new);

        let mut seen = BTreeSet::<String>::new();
        let mut requests = Vec::<bm_storage::AnchorUpsertRequest>::with_capacity(anchors_arr.len());

        // Pre-merge against existing anchors to avoid erasing refs/aliases/depends_on.
        for (idx, item) in anchors_arr.iter().enumerate() {
            let Some(obj) = item.as_object() else {
                return ai_error(
                    "INVALID_INPUT",
                    &format!("anchors[{idx}] must be an object"),
                );
            };
            let anchor_id = match require_string(obj, "anchor") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            if !seen.insert(anchor_id.clone()) {
                return ai_error(
                    "INVALID_INPUT",
                    &format!("anchors[{idx}].anchor duplicates a previous anchor id"),
                );
            }

            let title = match require_string(obj, "title") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let kind = match require_string(obj, "kind") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let status = match optional_string(obj, "status") {
                Ok(v) => v.unwrap_or_else(|| "active".to_string()),
                Err(resp) => return resp,
            };
            let description_override = match optional_nullable_string(obj, "description") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let refs_in = match optional_string_array(obj, "refs") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };
            let bind_paths = match optional_string_array(obj, "bind_paths") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };
            let aliases_in = match optional_string_array(obj, "aliases") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };
            let parent_override = match optional_nullable_string(obj, "parent_id") {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            let depends_in = match optional_string_array(obj, "depends_on") {
                Ok(v) => v.unwrap_or_default(),
                Err(resp) => return resp,
            };

            let existing = match self.store.anchor_get(
                &workspace,
                bm_storage::AnchorGetRequest {
                    id: anchor_id.clone(),
                },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };

            let mut refs = existing
                .as_ref()
                .map(|a| a.refs.clone())
                .unwrap_or_default();
            refs.extend(refs_in);
            for raw in bind_paths {
                let repo_rel = match repo_rel_from_path_input(&raw, repo_root) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                refs.push(format!("path:{repo_rel}"));
            }

            let mut aliases = existing
                .as_ref()
                .map(|a| a.aliases.clone())
                .unwrap_or_default();
            aliases.extend(aliases_in);

            let mut depends_on = existing
                .as_ref()
                .map(|a| a.depends_on.clone())
                .unwrap_or_default();
            depends_on.extend(depends_in);

            let description = match description_override {
                Some(v) => v.filter(|s| !s.trim().is_empty()),
                None => existing.as_ref().and_then(|a| a.description.clone()),
            };

            let parent_id = match parent_override {
                Some(v) => v.filter(|s| !s.trim().is_empty()),
                None => existing.as_ref().and_then(|a| a.parent_id.clone()),
            };

            requests.push(bm_storage::AnchorUpsertRequest {
                id: anchor_id,
                title,
                kind,
                description,
                refs,
                aliases,
                parent_id,
                depends_on,
                status,
            });
        }

        let mut created = 0usize;
        let mut updated = 0usize;
        let mut anchors_json = Vec::<Value>::new();

        if atomic {
            let boot = match self.store.anchors_bootstrap(
                &workspace,
                bm_storage::AnchorsBootstrapRequest { anchors: requests },
            ) {
                Ok(v) => v,
                Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            };
            for a in boot.anchors {
                if a.created {
                    created += 1;
                } else {
                    updated += 1;
                }
                anchors_json.push(json!({ "id": a.anchor.id, "created": a.created }));
            }
        } else {
            for req in requests {
                let existed = match self.store.anchor_get(
                    &workspace,
                    bm_storage::AnchorGetRequest { id: req.id.clone() },
                ) {
                    Ok(v) => v.is_some(),
                    Err(_) => false,
                };
                match self.store.anchor_upsert(&workspace, req) {
                    Ok(res) => {
                        if existed {
                            updated += 1;
                        } else {
                            created += 1;
                        }
                        anchors_json.push(json!({ "id": res.anchor.id, "created": !existed }));
                    }
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                }
            }
        }

        // Attach a compact bindings view (repo_rel strings) for transparency.
        let ids = anchors_json
            .iter()
            .filter_map(|v| v.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect::<Vec<_>>();
        let bindings = match self
            .store
            .anchor_bindings_list_for_anchors_any(&workspace, ids.clone())
        {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let mut by_anchor = BTreeMap::<String, Vec<String>>::new();
        for b in bindings {
            by_anchor.entry(b.anchor_id).or_default().push(b.repo_rel);
        }
        for (_k, v) in by_anchor.iter_mut() {
            v.sort();
            v.dedup();
        }

        let mut enriched = Vec::<Value>::with_capacity(anchors_json.len());
        for a in anchors_json {
            let id = a
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let paths = by_anchor.get(&id).cloned().unwrap_or_default();
            enriched.push(json!({
                "id": id,
                "created": a.get("created").and_then(|v| v.as_bool()).unwrap_or(false),
                "bindings": paths
            }));
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "anchors": enriched,
            "count": enriched.len(),
            "created": created,
            "updated": updated,
            "truncated": false
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) =
                enforce_graph_list_budget(&mut result, "anchors", limit);
            if budget_truncated || clamped {
                set_truncated_flag(&mut result, true);
            }
        }

        ai_ok("macro_atlas_apply", result)
    }

    pub(crate) fn tool_branchmind_atlas_bindings_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let max_chars = match optional_usize(args_obj, "max_chars") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let prefix = match optional_string(args_obj, "prefix") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let prefix = match prefix {
            None => None,
            Some(raw) => {
                let normalized = match normalize_repo_rel(&raw) {
                    Ok(v) => v,
                    Err(resp) => return resp,
                };
                if normalized == "." {
                    None
                } else {
                    Some(normalized)
                }
            }
        };

        let anchor_id = match optional_string(args_obj, "anchor") {
            Ok(v) => v.filter(|s| !s.trim().is_empty()),
            Err(resp) => return resp,
        };

        let limit = match optional_usize(args_obj, "limit") {
            Ok(v) => v.unwrap_or(100).clamp(1, 500),
            Err(resp) => return resp,
        };
        let offset = match optional_usize(args_obj, "offset") {
            Ok(v) => v.unwrap_or(0),
            Err(resp) => return resp,
        };

        let list = match self.store.anchor_bindings_index_list(
            &workspace,
            bm_storage::AnchorBindingsIndexListRequest {
                prefix,
                anchor_id,
                limit,
                offset,
            },
        ) {
            Ok(v) => v,
            Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let mut bindings = Vec::<Value>::new();
        for b in list.bindings {
            bindings.push(json!({
                "repo_rel": b.repo_rel,
                "kind": b.kind,
                "anchor": {
                    "id": b.anchor_id,
                    "title": b.anchor_title,
                    "kind": b.anchor_kind
                },
                "created_at_ms": b.created_at_ms,
                "updated_at_ms": b.updated_at_ms
            }));
        }

        let mut result = json!({
            "workspace": workspace.as_str(),
            "bindings": bindings,
            "count": bindings.len(),
            "limit": limit,
            "offset": offset,
            "truncated": list.has_more
        });

        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let (_used, budget_truncated) =
                enforce_graph_list_budget(&mut result, "bindings", limit);
            if budget_truncated || clamped {
                set_truncated_flag(&mut result, true);
            }
        }

        ai_ok("atlas_bindings_list", result)
    }
}
