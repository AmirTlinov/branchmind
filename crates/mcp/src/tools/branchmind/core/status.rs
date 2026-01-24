#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

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

fn compact_status_result(result: &Value) -> Value {
    let mut out = serde_json::Map::new();

    if let Some(server) = result.get("server") {
        let mut server_out = serde_json::Map::new();
        if let Some(name) = server.get("name") {
            server_out.insert("name".to_string(), name.clone());
        }
        if let Some(version) = server.get("version") {
            server_out.insert("version".to_string(), version.clone());
        }
        if let Some(build) = server.get("build_fingerprint") {
            server_out.insert("build_fingerprint".to_string(), build.clone());
        }
        if !server_out.is_empty() {
            out.insert("server".to_string(), Value::Object(server_out));
        }
    }

    if let Some(workspace) = result.get("workspace") {
        out.insert("workspace".to_string(), workspace.clone());
    }
    if let Some(toolset) = result.get("toolset") {
        out.insert("toolset".to_string(), toolset.clone());
    }
    if let Some(workspace_exists) = result.get("workspace_exists") {
        out.insert("workspace_exists".to_string(), workspace_exists.clone());
    }
    if let Some(checkout) = result.get("checkout") {
        out.insert("checkout".to_string(), checkout.clone());
    }
    if let Some(last_task_event) = result.get("last_task_event") {
        out.insert("last_task_event".to_string(), last_task_event.clone());
    } else if let Some(last_event) = result.get("last_event") {
        out.insert("last_event".to_string(), last_event.clone());
    }
    if let Some(last_doc_entry) = result.get("last_doc_entry") {
        out.insert("last_doc_entry".to_string(), last_doc_entry.clone());
    }

    if let Some(policy) = result.get("workspace_policy").and_then(|v| v.as_object()) {
        let mut policy_out = serde_json::Map::new();
        for key in [
            "workspace_effective",
            "workspace_mode",
            "workspace_allowlist_alias",
            "workspace_lock",
            "project_guard_configured",
            "project_guard_stored",
            "default_agent_id",
        ] {
            if let Some(value) = policy.get(key) {
                policy_out.insert(key.to_string(), value.clone());
            }
        }
        if !policy_out.is_empty() {
            out.insert("workspace_policy".to_string(), Value::Object(policy_out));
        }
    }

    out.insert(
        "next_action".to_string(),
        json!({ "tool": "tasks_snapshot" }),
    );

    Value::Object(out)
}

impl McpServer {
    pub(crate) fn tool_branchmind_status(&mut self, args: Value) -> Value {
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
        let verbosity = match parse_response_verbosity(
            args_obj,
            if self.dx_mode {
                ResponseVerbosity::Compact
            } else {
                self.response_verbosity
            },
        ) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut workspace_exists = match self.store.workspace_exists(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        if !workspace_exists {
            if let Err(err) = self.store.workspace_init(&workspace) {
                return ai_error("STORE_ERROR", &format_store_error(err));
            }
            workspace_exists = true;
        }
        let last_event = match self.store.workspace_last_event_head(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let last_doc_entry = match self.store.workspace_last_doc_entry_head(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let checkout = match self.store.branch_checkout_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };
        let project_guard_stored = match self.store.workspace_project_guard_get(&workspace) {
            Ok(v) => v,
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let defaults = json!({
            "branch": self.store.default_branch_name(),
            "docs": {
                "notes": DEFAULT_NOTES_DOC,
                "graph": DEFAULT_GRAPH_DOC,
                "trace": DEFAULT_TRACE_DOC
            }
        });

        let recommended_templates = json!({
            "plan": find_task_template("principal-plan", TaskKind::Plan).map(|t| json!({
                "id": t.id,
                "title": t.title,
                "description": t.description
            })).unwrap_or(Value::Null),
            "task": find_task_template("principal-task", TaskKind::Task).map(|t| json!({
                "id": t.id,
                "title": t.title,
                "description": t.description
            })).unwrap_or(Value::Null),
            "note": built_in_note_templates().into_iter().find(|t| t.id == "initiative").map(|t| json!({
                "id": t.id,
                "title": t.title,
                "description": t.description
            })).unwrap_or(Value::Null)
        });

        let portals = json!({
            "core": ["status", "workspace_use", "tasks_macro_start", "tasks_snapshot"],
            "daily": [
                "status",
                "workspace_use",
                "workspace_reset",
                "macro_branch_note",
                "tasks_macro_start",
                "tasks_macro_close_step",
                "tasks_snapshot",
                "open",
                "think_card",
                "think_playbook"
            ]
        });

        let build_profile = build_profile_label();
        let git_sha = build_git_sha();
        let workspace_effective = self
            .workspace_override
            .as_deref()
            .or(self.default_workspace.as_deref())
            .map(|v| v.to_string());
        let workspace_mode = if self.workspace_allowlist.is_some() {
            "allowlist"
        } else if self.workspace_explicit {
            "explicit"
        } else {
            "auto"
        };
        let (workspace_allowlist, allowlist_count, allowlist_truncated, allowlist_alias) =
            if let Some(list) = &self.workspace_allowlist {
                let mut list = list.clone();
                list.sort();
                let count = list.len();
                let alias = if list.len() <= 3 {
                    list.join(",")
                } else {
                    format!("{}+{}", list[0], list.len() - 1)
                };
                let truncated = count > 20;
                if truncated {
                    list.truncate(20);
                }
                (Some(list), count, truncated, Some(alias))
            } else {
                (None, 0, false, None)
            };

        let (disclosure_toolset, disclosure_hint) = match self.toolset {
            crate::Toolset::Core => (
                "daily",
                "To reveal daily portal tools without restarting the server, call tools/list with params.toolset=\"daily\".",
            ),
            crate::Toolset::Daily => (
                "full",
                "To reveal the full tool surface without restarting the server, call tools/list with params.toolset=\"full\".",
            ),
            crate::Toolset::Full => (
                "full",
                "To reveal the full tool surface without restarting the server, call tools/list with params.toolset=\"full\".",
            ),
        };
        let progressive_disclosure = json!({
            "tools_list_params": { "toolset": disclosure_toolset },
            "hint": disclosure_hint
        });

        let golden_path = match self.toolset {
            crate::Toolset::Core => json!([
                { "tool": "tasks_macro_start", "purpose": "create a task with steps and open a resume capsule" },
                { "tool": "tasks_snapshot", "purpose": "refresh unified snapshot (tasks + reasoning + diff)" }
            ]),
            crate::Toolset::Daily => json!([
                { "tool": "macro_branch_note", "purpose": "start an initiative branch + seed a first note" },
                { "tool": "tasks_macro_start", "purpose": "create a task with steps and open a resume capsule" },
                { "tool": "tasks_macro_close_step", "purpose": "confirm checkpoints + close step + return resume" },
                { "tool": "think_playbook", "purpose": "load deterministic prompts (strict skepticism / breakthrough)"},
                { "tool": "think_card", "purpose": "commit a structured reasoning card (hypothesis/test/evidence/decision)"},
                { "tool": "tasks_snapshot", "purpose": "refresh unified snapshot (tasks + reasoning + diff)" }
            ]),
            crate::Toolset::Full => json!([
                { "tool": "macro_branch_note", "purpose": "start an initiative branch + seed a first note" },
                { "tool": "tasks_macro_start", "purpose": "create a task with steps and open a resume capsule" },
                { "tool": "tasks_snapshot", "purpose": "refresh unified snapshot (tasks + reasoning + diff)" }
            ]),
        };

        let mut result = json!({
            "server": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
                "build_profile": build_profile,
                "git_sha": git_sha,
                "build_fingerprint": build_fingerprint()
            },
            "workspace": workspace.as_str(),
            "schema_version": "v0",
            "workspace_exists": workspace_exists,
            "checkout": checkout,
            "defaults": defaults,
            "workspace_policy": {
                "default_workspace": self.default_workspace.clone(),
                "workspace_override": self.workspace_override.clone(),
                "workspace_effective": workspace_effective,
                "workspace_mode": workspace_mode,
                "workspace_allowlist": workspace_allowlist,
                "workspace_allowlist_count": allowlist_count,
                "workspace_allowlist_truncated": allowlist_truncated,
                "workspace_allowlist_alias": allowlist_alias,
                "workspace_lock": self.workspace_lock,
                "project_guard_configured": self.project_guard.is_some(),
                "project_guard_stored": project_guard_stored,
                "default_agent_id": self.default_agent_id.clone()
            },
            "toolset": self.toolset.as_str(),
            "portals": portals,
            "recommended_templates": recommended_templates,
            "progressive_disclosure": progressive_disclosure,
            "golden_path": golden_path,
            "last_task_event": last_event.map(|(seq, ts_ms)| json!({
                "event_id": format!("evt_{:016}", seq),
                "ts": ts_ms_to_rfc3339(ts_ms),
                "ts_ms": ts_ms
            })),
            "last_event": last_event.map(|(seq, ts_ms)| json!({
                "event_id": format!("evt_{:016}", seq),
                "ts": ts_ms_to_rfc3339(ts_ms),
                "ts_ms": ts_ms
            })),
            "last_doc_entry": last_doc_entry.map(|head| json!({
                "seq": head.seq,
                "ts": ts_ms_to_rfc3339(head.ts_ms),
                "ts_ms": head.ts_ms,
                "branch": head.branch,
                "doc": head.doc,
                "kind": head.kind
            })),
        });

        let mut suggestions = Vec::new();
        if !workspace_exists {
            suggestions.push(suggest_call(
                "init",
                "Initialize the workspace and bootstrap a default branch.",
                "high",
                json!({ "workspace": workspace.as_str() }),
            ));
        } else if checkout.is_none() {
            suggestions.push(suggest_call(
                "branch_list",
                "List known branches for this workspace.",
                "medium",
                json!({ "workspace": workspace.as_str() }),
            ));
        }

        if verbosity == ResponseVerbosity::Compact {
            result = compact_status_result(&result);
        }

        let mut warnings = Vec::new();
        if let Some(limit) = max_chars {
            let (limit, clamped) = clamp_budget_max(limit);
            let mut truncated = false;
            let mut minimal = false;

            let _used =
                ensure_budget_limit(&mut result, limit, &mut truncated, &mut minimal, |value| {
                    let mut changed = false;
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["last_doc_entry"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["last_event"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["defaults"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["workspace_policy"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["progressive_disclosure"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["recommended_templates"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["portals"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["golden_path"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["checkout"]);
                    }
                    if json_len_chars(value) > limit {
                        changed |= drop_fields_at(value, &[], &["schema_version"]);
                    }
                    changed
                });

            warnings = budget_warnings(truncated, minimal, clamped);
        }

        if warnings.is_empty() {
            ai_ok_with("status", result, suggestions)
        } else {
            ai_ok_with_warnings("status", result, warnings, suggestions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_storage::SqliteStore;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bm_status_compact_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn build_server(dx_mode: bool) -> (McpServer, PathBuf) {
        let dir = temp_dir();
        let store = SqliteStore::open(&dir).expect("open store");
        let runner_autostart_enabled =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let runner_autostart_state =
            std::sync::Arc::new(std::sync::Mutex::new(crate::RunnerAutostartState::default()));
        (
            McpServer::new(
                store,
                crate::McpServerConfig {
                    toolset: crate::Toolset::Daily,
                    response_verbosity: crate::ResponseVerbosity::Full,
                    dx_mode,
                    default_workspace: Some("demo".to_string()),
                    workspace_explicit: false,
                    workspace_allowlist: None,
                    workspace_lock: true,
                    project_guard: None,
                    project_guard_rebind_enabled: false,
                    default_agent_id: None,
                    runner_autostart_enabled,
                    runner_autostart_dry_run: false,
                    runner_autostart: runner_autostart_state,
                },
            ),
            dir,
        )
    }

    #[test]
    fn status_defaults_compact_in_dx_mode() {
        let (mut server, dir) = build_server(true);
        let resp = server.tool_branchmind_status(json!({ "workspace": "demo" }));
        assert_eq!(
            resp.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "status should succeed"
        );
        let result = resp.get("result").expect("result");
        assert!(result.get("defaults").is_none(), "compact drops defaults");
        assert!(
            result.get("golden_path").is_none(),
            "compact drops golden_path"
        );
        assert!(result.get("workspace").is_some(), "compact keeps workspace");
        assert_eq!(
            result
                .get("next_action")
                .and_then(|v| v.get("tool"))
                .and_then(|v| v.as_str()),
            Some("tasks_snapshot")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
