#![forbid(unsafe_code)]

use crate::McpServer;
use serde_json::Value;

impl McpServer {
    pub(super) fn auto_escalate_budget_if_needed(
        &mut self,
        name: &str,
        original_args: &Value,
        args: &Value,
        resp: &Value,
    ) -> Option<(Value, Value)> {
        // Safety: never override explicit budgets.
        let original_obj = original_args.as_object()?;
        if original_obj.contains_key("max_chars") || original_obj.contains_key("context_budget") {
            return None;
        }

        if resp.get("success").and_then(|v| v.as_bool()) != Some(true) {
            return None;
        }
        if !response_has_budget_truncation_warning(resp) {
            return None;
        }

        // Only retry tools that are read-ish and safe to rerun. Even if some of these perform
        // internal idempotent "ensure" writes (workspace/doc/ref), they must not append history.
        if !auto_budget_escalation_allowlist(name) {
            return None;
        }

        // Goal: remove "limit juggling" friction while keeping outputs bounded and deterministic.
        // We retry a small, fixed number of times and stop early once truncation disappears.
        //
        // Important: this must remain safe for tools that are internally "read-ish" but may
        // perform idempotent ensure-writes (e.g. workspace/doc refs). They must never append
        // user-visible history on reads.
        const MAX_RETRIES: usize = 6;
        let cap = auto_budget_escalation_cap_chars(name);

        let mut current_args = args.clone();
        let mut current_resp = resp.clone();
        let mut did_escalate = false;

        for _ in 0..MAX_RETRIES {
            if !response_has_budget_truncation_warning(&current_resp) {
                break;
            }
            let Some((current_max_chars, used_chars)) = extract_budget_snapshot(&current_resp)
            else {
                break;
            };
            if current_max_chars >= cap {
                break;
            }

            let used_chars = used_chars.unwrap_or(current_max_chars);
            let mut next_max_chars = current_max_chars
                .saturating_mul(2)
                .max(used_chars.saturating_mul(2))
                .max(current_max_chars.saturating_add(1));
            if next_max_chars > cap {
                next_max_chars = cap;
            }
            if next_max_chars <= current_max_chars {
                break;
            }

            let mut next_args = current_args.clone();
            let Some(args_obj) = next_args.as_object_mut() else {
                break;
            };
            apply_auto_escalated_budget(args_obj, next_max_chars);

            let Some(next_resp) = crate::tools::dispatch_tool(self, name, next_args.clone()) else {
                break;
            };
            if next_resp.get("success").and_then(|v| v.as_bool()) != Some(true) {
                break;
            }

            current_args = next_args;
            current_resp = next_resp;
            did_escalate = true;
        }

        if did_escalate {
            Some((current_args, current_resp))
        } else {
            None
        }
    }
}

pub(super) fn apply_portal_default_budgets(
    toolset: crate::Toolset,
    dx_mode: bool,
    name: &str,
    args_obj: &mut serde_json::Map<String, Value>,
) {
    // Portal defaults should make truncation warnings rare. If a portal still truncates, the
    // server may auto-escalate budgets for read-ish portals (status/snapshot/anchors_*).
    //
    // Important: keep explicit caller budgets untouched (explicit wins).
    let default_status_max_chars = if dx_mode {
        match toolset {
            crate::Toolset::Core => 6_000,
            crate::Toolset::Daily => 9_000,
            crate::Toolset::Full => 12_000,
        }
    } else {
        match toolset {
            crate::Toolset::Core => 20_000,
            crate::Toolset::Daily => 40_000,
            crate::Toolset::Full => 60_000,
        }
    };
    // NOTE: keep snapshot defaults in the "medium" tier so the capsule remains stable and
    // continuation commands (e.g. notes_cursor) stay predictable in DX tests.
    let default_snapshot_context_budget = match toolset {
        crate::Toolset::Core => 6_000,
        crate::Toolset::Daily => 9_000,
        crate::Toolset::Full => 12_000,
    };
    let default_resume_max_chars = match toolset {
        crate::Toolset::Core => 20_000,
        crate::Toolset::Daily => 40_000,
        crate::Toolset::Full => 60_000,
    };
    let default_anchor_max_chars = match toolset {
        crate::Toolset::Core => 30_000,
        crate::Toolset::Daily => 60_000,
        crate::Toolset::Full => 80_000,
    };

    match name {
        "status" => {
            if !args_obj.contains_key("max_chars") {
                args_obj.insert(
                    "max_chars".to_string(),
                    Value::Number(serde_json::Number::from(default_status_max_chars as u64)),
                );
            }
        }
        "tasks_snapshot" => {
            if !args_obj.contains_key("context_budget") && !args_obj.contains_key("max_chars") {
                args_obj.insert(
                    "context_budget".to_string(),
                    Value::Number(serde_json::Number::from(
                        default_snapshot_context_budget as u64,
                    )),
                );
            }
        }
        "tasks_macro_start" | "tasks_macro_delegate" | "tasks_macro_close_step" => {
            if !args_obj.contains_key("resume_max_chars") {
                args_obj.insert(
                    "resume_max_chars".to_string(),
                    Value::Number(serde_json::Number::from(default_resume_max_chars as u64)),
                );
            }
        }
        "anchors_list" | "anchor_snapshot" | "anchors_export" => {
            if !args_obj.contains_key("max_chars") {
                args_obj.insert(
                    "max_chars".to_string(),
                    Value::Number(serde_json::Number::from(default_anchor_max_chars as u64)),
                );
            }
        }
        _ => {}
    }
}

pub(super) fn apply_read_tool_default_budgets(
    name: &str,
    args_obj: &mut serde_json::Map<String, Value>,
) {
    // Keep budgets opt-in for BM-L1 line outputs (they render warnings as extra lines).
    let fmt = args_obj.get("fmt").and_then(|v| v.as_str());
    if crate::is_lines_fmt(fmt) {
        return;
    }
    if args_obj.contains_key("max_chars") || args_obj.contains_key("context_budget") {
        return;
    }

    if !read_tool_accepts_budget(name) {
        return;
    }

    // Default budgets are intentionally generous but bounded. The goal is to remove
    // "limit juggling" for the common case, while still keeping the output deterministic.
    let default_context_budget = match name {
        // "Pack" tools are likely to be pasted directly into an agent context window.
        "tasks_resume_super" | "context_pack" | "think_pack" | "think_watch" => 20_000usize,

        // Read views that can become large quickly in active projects.
        "tasks_context"
        | "tasks_resume_pack"
        | "tasks_context_pack"
        | "tasks_radar"
        | "tasks_handoff"
        | "think_context"
        | "think_frontier"
        | "think_query"
        | "think_next"
        | "show"
        | "diff"
        | "log"
        | "docs_list"
        | "tag_list"
        | "reflog"
        | "branch_list"
        | "graph_query"
        | "graph_validate"
        | "graph_diff"
        | "graph_conflicts"
        | "graph_conflict_show"
        | "trace_hydrate"
        | "trace_validate"
        | "transcripts_open"
        | "transcripts_digest"
        | "transcripts_search"
        | "help"
        | "diagnostics" => 16_000usize,

        // Safe default for other read-ish tools that accept max_chars.
        _ => 12_000usize,
    };

    // Prefer context_budget when available (it behaves as a max_chars alias and can
    // deterministically shift default views toward smart retrieval).
    if read_tool_supports_context_budget(name) {
        args_obj.insert(
            "context_budget".to_string(),
            Value::Number(serde_json::Number::from(default_context_budget as u64)),
        );
    } else {
        args_obj.insert(
            "max_chars".to_string(),
            Value::Number(serde_json::Number::from(default_context_budget as u64)),
        );
    }
}

pub(super) fn read_tool_accepts_budget(name: &str) -> bool {
    matches!(
        name,
        // Tasks reads
        "tasks_context"
            | "tasks_delta"
            | "tasks_radar"
            | "tasks_handoff"
            | "tasks_context_pack"
            | "tasks_resume_pack"
            | "tasks_resume_super"
            | "tasks_mindpack"
            // Core reasoning reads / packs
            | "help"
            | "diagnostics"
            | "context_pack"
            // Reasoning packs & reads
            | "think_pack"
            | "think_context"
            | "think_frontier"
            | "think_query"
            | "think_next"
            | "think_watch"
            | "show"
            | "diff"
            | "log"
            | "docs_list"
            | "tag_list"
            | "reflog"
            | "graph_query"
            | "graph_validate"
            | "graph_diff"
            | "graph_conflicts"
            | "graph_conflict_show"
            | "branch_list"
            | "trace_hydrate"
            | "trace_validate"
            | "transcripts_open"
            | "transcripts_digest"
            | "transcripts_search"
    )
}

pub(super) fn read_tool_supports_context_budget(name: &str) -> bool {
    matches!(
        name,
        "tasks_resume_super"
            | "tasks_snapshot"
            | "think_pack"
            | "think_context"
            | "think_frontier"
            | "think_query"
            | "think_next"
            | "think_watch"
            | "context_pack"
    )
}

pub(super) fn response_has_budget_truncation_warning(resp: &Value) -> bool {
    let Some(warnings) = resp.get("warnings").and_then(|v| v.as_array()) else {
        return false;
    };
    warnings.iter().any(|w| {
        matches!(
            w.get("code").and_then(|v| v.as_str()),
            Some("BUDGET_TRUNCATED") | Some("BUDGET_MINIMAL")
        )
    })
}

pub(super) fn extract_budget_snapshot(resp: &Value) -> Option<(usize, Option<usize>)> {
    let budget = resp.get("result")?.get("budget")?;
    let max_chars = budget.get("max_chars")?.as_u64()? as usize;
    let used_chars = budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    Some((max_chars, used_chars))
}

pub(super) fn auto_budget_escalation_allowlist(name: &str) -> bool {
    matches!(
        name,
        // Portal read tools (fmt=lines is enforced). These are safe to rerun because they are
        // read-mostly and any internal "ensure" writes must remain idempotent and history-free.
        "status"
            | "tasks_snapshot"
            | "anchors_list"
            | "anchor_snapshot"
            | "anchors_export"
            // Read tools (JSON or lines depending on toolset).
            | "tasks_context"
            | "tasks_resume_pack"
            | "tasks_resume_super"
            | "tasks_context_pack"
            | "tasks_delta"
            | "tasks_radar"
            | "tasks_handoff"
            | "help"
            | "diagnostics"
            | "context_pack"
            | "think_pack"
            | "think_context"
            | "think_frontier"
            | "think_query"
            | "think_next"
            | "think_watch"
            | "show"
            | "diff"
            | "log"
            | "docs_list"
            | "tag_list"
            | "reflog"
            | "graph_query"
            | "graph_validate"
            | "graph_diff"
            | "graph_conflicts"
            | "graph_conflict_show"
            | "branch_list"
            | "trace_hydrate"
            | "trace_validate"
            | "transcripts_open"
            | "transcripts_digest"
            | "transcripts_search"
    )
}

pub(super) fn auto_budget_escalation_cap_chars(_name: &str) -> usize {
    // Hard cap to prevent runaway responses even under repeated retries.
    //
    // Goal: keep the “no limit juggling” experience while still bounding worst-case outputs.
    1_000_000
}

pub(super) fn apply_auto_escalated_budget(
    args_obj: &mut serde_json::Map<String, Value>,
    max_chars: usize,
) {
    let next = Value::Number(serde_json::Number::from(max_chars as u64));
    let mut applied = false;
    if args_obj.contains_key("context_budget") {
        args_obj.insert("context_budget".to_string(), next.clone());
        applied = true;
    }
    if args_obj.contains_key("max_chars") {
        args_obj.insert("max_chars".to_string(), next.clone());
        applied = true;
    }
    if !applied {
        args_obj.insert("max_chars".to_string(), next);
    }
}

pub(super) fn response_obj_has_budget_truncation_warning(
    resp_obj: &serde_json::Map<String, Value>,
) -> bool {
    let Some(warnings) = resp_obj.get("warnings").and_then(|v| v.as_array()) else {
        return false;
    };
    warnings.iter().any(|w| {
        matches!(
            w.get("code").and_then(|v| v.as_str()),
            Some("BUDGET_TRUNCATED") | Some("BUDGET_MINIMAL")
        )
    })
}

pub(super) fn extract_budget_snapshot_from_obj(
    resp_obj: &serde_json::Map<String, Value>,
) -> Option<(usize, Option<usize>)> {
    let budget = resp_obj.get("result")?.get("budget")?;
    let max_chars = budget.get("max_chars")?.as_u64()? as usize;
    let used_chars = budget
        .get("used_chars")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    Some((max_chars, used_chars))
}

pub(super) fn extract_result_next_cursor(resp_obj: &serde_json::Map<String, Value>) -> Option<i64> {
    let pagination = resp_obj.get("result")?.get("pagination")?;
    let has_more = pagination
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !has_more {
        return None;
    }
    pagination.get("next_cursor")?.as_i64()
}
