#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

pub(super) struct FocusOnlyStepRef {
    pub(super) step_id: String,
    pub(super) first_open: Value,
}

pub(super) fn parse_first_open_step(steps_summary: Option<&Value>) -> Option<FocusOnlyStepRef> {
    let steps = steps_summary?;
    let first = steps.get("first_open")?;
    let step_id = first.get("step_id")?.as_str()?.trim();
    if step_id.is_empty() {
        return None;
    }
    Some(FocusOnlyStepRef {
        step_id: step_id.to_string(),
        first_open: first.clone(),
    })
}

pub(super) fn build_step_focus_detail(
    detail: bm_storage::StepDetail,
    first_open: Option<&Value>,
) -> Value {
    let require_security = first_open
        .and_then(|v| v.get("require_security"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let require_perf = first_open
        .and_then(|v| v.get("require_perf"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let require_docs = first_open
        .and_then(|v| v.get("require_docs"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let proof_tests_mode = first_open
        .and_then(|v| v.get("proof_tests_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("off");
    let proof_security_mode = first_open
        .and_then(|v| v.get("proof_security_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("off");
    let proof_perf_mode = first_open
        .and_then(|v| v.get("proof_perf_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("off");
    let proof_docs_mode = first_open
        .and_then(|v| v.get("proof_docs_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("off");

    let proof_tests_present = first_open
        .and_then(|v| v.get("proof_tests_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proof_security_present = first_open
        .and_then(|v| v.get("proof_security_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proof_perf_present = first_open
        .and_then(|v| v.get("proof_perf_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let proof_docs_present = first_open
        .and_then(|v| v.get("proof_docs_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    json!({
        "step": {
            "step_id": detail.step_id,
            "path": detail.path,
            "title": detail.title,
            "completed": detail.completed,
            "blocked": detail.blocked,
            "block_reason": detail.block_reason
        },
        "detail": {
            "success_criteria": detail.success_criteria,
            "tests": detail.tests,
            "blockers": detail.blockers,
            "criteria_confirmed": detail.criteria_confirmed,
            "tests_confirmed": detail.tests_confirmed,
            "security_confirmed": detail.security_confirmed,
            "perf_confirmed": detail.perf_confirmed,
            "docs_confirmed": detail.docs_confirmed,
            "require_security": require_security,
            "require_perf": require_perf,
            "require_docs": require_docs,
            "proof_tests_mode": proof_tests_mode,
            "proof_security_mode": proof_security_mode,
            "proof_perf_mode": proof_perf_mode,
            "proof_docs_mode": proof_docs_mode,
            "proof_tests_present": proof_tests_present,
            "proof_security_present": proof_security_present,
            "proof_perf_present": proof_perf_present,
            "proof_docs_present": proof_docs_present
        }
    })
}

pub(super) fn apply_focus_only_shaping(
    result: &mut Value,
    step_path: Option<&str>,
    focus_step_tag: Option<&str>,
    engine_max_cards: usize,
    keep_graph_diff: bool,
) {
    // 1) Filter timeline to current step path (segment-safe prefix match).
    if let Some(path) = step_path {
        let nested_prefix = format!("{path}.");
        if let Some(events) = result
            .get_mut("timeline")
            .and_then(|v| v.get_mut("events"))
            .and_then(|v| v.as_array_mut())
        {
            events.retain(|event| {
                let Some(p) = event.get("path").and_then(|v| v.as_str()) else {
                    return false;
                };
                p == path || p.starts_with(&nested_prefix)
            });
            if events.len() > 12 {
                let keep_from = events.len().saturating_sub(12);
                events.drain(0..keep_from);
            }
        }
    }

    // 2) Drop graph_diff by default (focus view favors relevance).
    if !keep_graph_diff && let Some(obj) = result.as_object_mut() {
        obj.remove("graph_diff");
    }

    // 3) Compress memory: keep only relevant cards.
    let mut keep_ids = std::collections::BTreeSet::<String>::new();
    if let Some(cards) = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
    {
        for card in cards {
            let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let tags = card.get("tags").and_then(|v| v.as_array());
            let pinned = tags
                .map(|tags| tags.iter().any(|t| t.as_str() == Some(PIN_TAG)))
                .unwrap_or(false);
            let step_scoped = focus_step_tag
                .and_then(|tag| tags.map(|tags| tags.iter().any(|t| t.as_str() == Some(tag))))
                .unwrap_or(false);
            if pinned || step_scoped {
                keep_ids.insert(id.to_string());
            }
        }

        // Keep a small set of open cards (recency-biased).
        let mut open_ids = Vec::<String>::new();
        for card in cards {
            let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let status = card.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status != "open" {
                continue;
            }
            let ty = card.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(ty, "hypothesis" | "question" | "test") {
                open_ids.push(id.to_string());
            }
        }
        for id in open_ids.into_iter().take(engine_max_cards.max(1)) {
            keep_ids.insert(id);
        }
    }

    // Keep cards referenced by engine (if present).
    if let Some(engine) = result.get("engine") {
        for list_key in ["signals", "actions"] {
            if let Some(list) = engine.get(list_key).and_then(|v| v.as_array()) {
                for item in list {
                    if let Some(refs) = item.get("refs").and_then(|v| v.as_array()) {
                        for r in refs {
                            if r.get("kind").and_then(|v| v.as_str()) != Some("card") {
                                continue;
                            }
                            if let Some(id) = r.get("id").and_then(|v| v.as_str()) {
                                keep_ids.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(cards) = result
        .get_mut("memory")
        .and_then(|v| v.get_mut("cards"))
        .and_then(|v| v.as_array_mut())
    {
        if !keep_ids.is_empty() {
            cards.retain(|card| {
                let Some(id) = card.get("id").and_then(|v| v.as_str()) else {
                    return false;
                };
                keep_ids.contains(id)
            });
        }
        let max_cards = engine_max_cards.clamp(1, 20);
        if cards.len() > max_cards {
            cards.truncate(max_cards);
        }
    }
    recompute_card_stats_at(result, &["memory", "cards"], &["memory", "stats"]);
    refresh_pagination_count(
        result,
        &["memory", "cards"],
        &["memory", "cards_pagination"],
    );
    if let Some(pagination) = get_object_mut(result, &["memory", "cards_pagination"]) {
        pagination.insert("cursor".to_string(), Value::Null);
        pagination.insert("next_cursor".to_string(), Value::Null);
        pagination.insert("has_more".to_string(), Value::Bool(false));
    }

    // 4) Minimize notes/trace payloads in focus view.
    for (entries_path, pagination_path) in [
        (
            &["memory", "notes", "entries"][..],
            &["memory", "notes", "pagination"][..],
        ),
        (
            &["memory", "trace", "entries"][..],
            &["memory", "trace", "pagination"][..],
        ),
    ] {
        if let Some(entries) = get_array_mut(result, entries_path) {
            entries.clear();
        }
        if let Some(pagination) = get_object_mut(result, pagination_path) {
            pagination.insert(
                "count".to_string(),
                Value::Number(serde_json::Number::from(0)),
            );
            pagination.insert("has_more".to_string(), Value::Bool(false));
            pagination.insert("next_cursor".to_string(), Value::Null);
        }
    }
    if let Some(trace_obj) = get_object_mut(result, &["memory", "trace"]) {
        trace_obj.remove("sequential");
    }

    // Keep engine consistent with the trimmed cards slice even when max_chars is not set.
    let cards_snapshot = result
        .get("memory")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(engine) = result.get_mut("engine") {
        filter_engine_to_cards(engine, &cards_snapshot);
    }
}

fn get_array_mut<'a>(root: &'a mut Value, path: &[&str]) -> Option<&'a mut Vec<Value>> {
    let mut cur = root;
    for key in &path[..path.len().saturating_sub(1)] {
        cur = cur.get_mut(*key)?;
    }
    cur.get_mut(*path.last()?)?.as_array_mut()
}

fn get_object_mut<'a>(
    root: &'a mut Value,
    path: &[&str],
) -> Option<&'a mut serde_json::Map<String, Value>> {
    let mut cur = root;
    for key in &path[..path.len().saturating_sub(1)] {
        cur = cur.get_mut(*key)?;
    }
    cur.get_mut(*path.last()?)?.as_object_mut()
}
