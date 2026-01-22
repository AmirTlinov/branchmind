#![forbid(unsafe_code)]

use super::ai::warning;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
struct SeqNode {
    thought_number: i64,
    seq: i64,
    ts_ms: Option<i64>,
    is_revision: Option<(i64, bool)>,
    revises_thought: Option<(i64, i64)>,
    branch_from_thought: Option<(i64, i64)>,
    branch_id: Option<(i64, String)>,
}

#[derive(Clone, Debug)]
struct SeqEdge {
    rel: &'static str,
    from: i64,
    to: i64,
    branch_id: Option<String>,
}

fn opt_non_empty_str(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn opt_positive_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64()).filter(|v| *v > 0)
}

pub(crate) fn trace_step_sequential_meta_warnings(meta: Option<&Value>) -> Vec<Value> {
    let Some(Value::Object(obj)) = meta else {
        return Vec::new();
    };

    let sequential_keys = [
        "thoughtNumber",
        "totalThoughts",
        "nextThoughtNeeded",
        "branchFromThought",
        "branchId",
        "isRevision",
        "revisesThought",
    ];
    let has_any_sequential_key = sequential_keys.iter().any(|key| obj.contains_key(*key));
    if !has_any_sequential_key {
        return Vec::new();
    }

    let mut warnings = Vec::new();

    let thought_number_present = obj.contains_key("thoughtNumber");
    let thought_number = obj.get("thoughtNumber").and_then(|v| v.as_i64());
    let thought_number_ok = thought_number.is_some_and(|v| v > 0);

    let branch_from_present = obj.contains_key("branchFromThought");
    let branch_from = obj.get("branchFromThought").and_then(|v| v.as_i64());
    let branch_from_ok = branch_from.is_some_and(|v| v > 0);

    let branch_id_present = obj.contains_key("branchId");
    let branch_id_ok = obj
        .get("branchId")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .is_some_and(|s| !s.is_empty());

    let is_revision_present = obj.contains_key("isRevision");
    let is_revision = obj.get("isRevision").and_then(|v| v.as_bool());
    let revises_thought_present = obj.contains_key("revisesThought");
    let revises_thought = obj.get("revisesThought").and_then(|v| v.as_i64());
    let revises_thought_ok = revises_thought.is_some_and(|v| v > 0);

    let total_thoughts_present = obj.contains_key("totalThoughts");
    let total_thoughts = obj.get("totalThoughts").and_then(|v| v.as_i64());
    let total_thoughts_ok = total_thoughts.is_some_and(|v| v > 0);

    let next_thought_needed_present = obj.contains_key("nextThoughtNeeded");
    let next_thought_needed_ok = obj
        .get("nextThoughtNeeded")
        .and_then(|v| v.as_bool())
        .is_some();

    // If meta contains any sequential-related keys but omits thoughtNumber,
    // the derived sequential graph cannot include this entry.
    let meta_needs_thought_number = !thought_number_present
        && (branch_from_present
            || branch_id_present
            || is_revision_present
            || revises_thought_present
            || total_thoughts_present
            || next_thought_needed_present);
    if meta_needs_thought_number {
        warnings.push(warning(
            "TRACE_SEQ_META_MISSING_THOUGHT_NUMBER",
            "trace_step meta looks sequential but is missing thoughtNumber",
            "Add meta.thoughtNumber (positive integer) or use trace_sequential_step for canonical sequential tracing.",
        ));
    }

    if thought_number_present && !thought_number_ok {
        warnings.push(warning(
            "TRACE_SEQ_META_INVALID_THOUGHT_NUMBER",
            "trace_step meta.thoughtNumber must be a positive integer when provided",
            "Set meta.thoughtNumber to a positive integer (1..N) or remove it.",
        ));
    }

    if branch_from_present && !branch_from_ok {
        warnings.push(warning(
            "TRACE_SEQ_META_INVALID_BRANCH_FROM_THOUGHT",
            "trace_step meta.branchFromThought must be a positive integer when provided",
            "Set meta.branchFromThought to a positive integer (the parent thoughtNumber) or remove it.",
        ));
    }

    if branch_id_present && !branch_id_ok {
        warnings.push(warning(
            "TRACE_SEQ_META_INVALID_BRANCH_ID",
            "trace_step meta.branchId must be a non-empty string when provided",
            "Set meta.branchId to a non-empty string (e.g. \"alt-1\") or remove it.",
        ));
    }

    if is_revision == Some(true) && !revises_thought_present {
        warnings.push(warning(
            "TRACE_SEQ_META_REVISION_MISSING_REVISES_THOUGHT",
            "trace_step meta.isRevision=true requires meta.revisesThought",
            "Add meta.revisesThought (positive integer) or set meta.isRevision=false.",
        ));
    }

    if revises_thought_present && !revises_thought_ok {
        warnings.push(warning(
            "TRACE_SEQ_META_INVALID_REVISES_THOUGHT",
            "trace_step meta.revisesThought must be a positive integer when provided",
            "Set meta.revisesThought to a positive integer (the revised thoughtNumber) or remove it.",
        ));
    }

    if total_thoughts_present && !total_thoughts_ok {
        warnings.push(warning(
            "TRACE_SEQ_META_INVALID_TOTAL_THOUGHTS",
            "trace_step meta.totalThoughts must be a positive integer when provided",
            "Set meta.totalThoughts to a positive integer (>= thoughtNumber) or remove it.",
        ));
    }

    if thought_number_ok && total_thoughts_ok {
        let thought_number = thought_number.unwrap_or(0);
        let total_thoughts = total_thoughts.unwrap_or(0);
        if total_thoughts < thought_number {
            warnings.push(warning(
                "TRACE_SEQ_META_TOTAL_THOUGHTS_LT_THOUGHT_NUMBER",
                "trace_step meta.totalThoughts must be >= meta.thoughtNumber when both are provided",
                "Set meta.totalThoughts >= meta.thoughtNumber or use trace_sequential_step to let the tool enforce invariants.",
            ));
        }
    }

    if next_thought_needed_present && !next_thought_needed_ok {
        warnings.push(warning(
            "TRACE_SEQ_META_INVALID_NEXT_THOUGHT_NEEDED",
            "trace_step meta.nextThoughtNeeded must be a boolean when provided",
            "Set meta.nextThoughtNeeded to true/false or remove it.",
        ));
    }

    // Keep this lint low-noise: `trace_step` is not a validator, and agents should not be forced
    // into multi-warning debugging loops. Prefer the canonical `trace_sequential_step` + `trace_validate`
    // path when deep validation is needed.
    //
    // Deterministic: keep the first warnings in stable priority order.
    warnings.truncate(2);

    warnings
}

fn bump_field_bool(field: &mut Option<(i64, bool)>, seq: i64, value: Option<bool>) {
    let Some(value) = value else { return };
    match field.as_ref().map(|(s, _)| *s) {
        None => *field = Some((seq, value)),
        Some(existing_seq) if seq > existing_seq => *field = Some((seq, value)),
        _ => {}
    }
}

fn bump_field_i64(field: &mut Option<(i64, i64)>, seq: i64, value: Option<i64>) {
    let Some(value) = value else { return };
    match field.as_ref().map(|(s, _)| *s) {
        None => *field = Some((seq, value)),
        Some(existing_seq) if seq > existing_seq => *field = Some((seq, value)),
        _ => {}
    }
}

fn bump_field_string(field: &mut Option<(i64, String)>, seq: i64, value: Option<String>) {
    let Some(value) = value else { return };
    match field.as_ref().map(|(s, _)| *s) {
        None => *field = Some((seq, value)),
        Some(existing_seq) if seq > existing_seq => *field = Some((seq, value)),
        _ => {}
    }
}

pub(crate) fn derive_trace_sequential_graph(entries: &[Value]) -> Option<Value> {
    // Build a compact, deterministic graph for sequential trace entries.
    //
    // This is derived (not stored) so the server can keep raw trace entries low-ceremony while
    // still giving agents a ready-to-consume branch structure.
    let mut nodes_by_thought = std::collections::BTreeMap::<i64, SeqNode>::new();

    // First pass: collect one node per thought number.
    //
    // We accept both:
    // - `trace_sequential_step` (canonical tool)
    // - `trace_step` with sequential meta keys (low-ceremony / user-provided meta)
    for entry in entries {
        let format = entry.get("format").and_then(|v| v.as_str()).unwrap_or("");
        if format != "trace_sequential_step" && format != "trace_step" {
            continue;
        }
        let Some(meta) = entry.get("meta").and_then(|v| v.as_object()) else {
            continue;
        };

        let Some(thought_number) = meta
            .get("thoughtNumber")
            .and_then(|v| v.as_i64())
            .filter(|v| *v > 0)
        else {
            continue;
        };

        let seq = entry
            .get("seq")
            .and_then(|v| v.as_i64())
            .filter(|v| *v > 0)
            .unwrap_or(0);
        let ts_ms = entry.get("ts_ms").and_then(|v| v.as_i64());

        let node = nodes_by_thought.entry(thought_number).or_insert(SeqNode {
            thought_number,
            seq: 0,
            ts_ms: None,
            is_revision: None,
            revises_thought: None,
            branch_from_thought: None,
            branch_id: None,
        });

        if seq > node.seq {
            node.seq = seq;
            if ts_ms.is_some() {
                node.ts_ms = ts_ms;
            }
        }

        bump_field_bool(
            &mut node.is_revision,
            seq,
            meta.get("isRevision").and_then(|v| v.as_bool()),
        );
        bump_field_i64(
            &mut node.revises_thought,
            seq,
            opt_positive_i64(meta.get("revisesThought")),
        );
        bump_field_i64(
            &mut node.branch_from_thought,
            seq,
            opt_positive_i64(meta.get("branchFromThought")),
        );
        bump_field_string(
            &mut node.branch_id,
            seq,
            opt_non_empty_str(meta.get("branchId")),
        );
    }

    if nodes_by_thought.is_empty() {
        return None;
    }

    // Second pass: derive edges + missing refs.
    let mut edges = Vec::<SeqEdge>::new();
    let mut missing_branch_parents = std::collections::BTreeSet::<i64>::new();
    let mut missing_revision_targets = std::collections::BTreeSet::<i64>::new();

    for node in nodes_by_thought.values() {
        if let Some(from) = node.branch_from_thought.as_ref().map(|(_, v)| *v) {
            edges.push(SeqEdge {
                rel: "branch",
                from,
                to: node.thought_number,
                branch_id: node.branch_id.as_ref().map(|(_, v)| v.clone()),
            });
            if !nodes_by_thought.contains_key(&from) {
                missing_branch_parents.insert(from);
            }
        }

        if node.is_revision.as_ref().map(|(_, v)| *v).unwrap_or(false)
            && let Some(from) = node.revises_thought.as_ref().map(|(_, v)| *v)
        {
            edges.push(SeqEdge {
                rel: "revision",
                from,
                to: node.thought_number,
                branch_id: None,
            });
            if !nodes_by_thought.contains_key(&from) {
                missing_revision_targets.insert(from);
            }
        }
    }

    // Deterministic ordering.
    edges.sort_by(|a, b| {
        (a.rel, a.from, a.to, a.branch_id.as_deref().unwrap_or("")).cmp(&(
            b.rel,
            b.from,
            b.to,
            b.branch_id.as_deref().unwrap_or(""),
        ))
    });

    let nodes_json = nodes_by_thought
        .values()
        .map(|node| {
            json!({
                "thoughtNumber": node.thought_number,
                "seq": node.seq,
                "ts_ms": node.ts_ms,
                "branchFromThought": node.branch_from_thought.as_ref().map(|(_, v)| *v),
                "branchId": node.branch_id.as_ref().map(|(_, v)| v.clone()),
                "isRevision": node.is_revision.as_ref().map(|(_, v)| *v),
                "revisesThought": node.revises_thought.as_ref().map(|(_, v)| *v)
            })
        })
        .collect::<Vec<_>>();
    let edges_json = edges
        .into_iter()
        .map(|edge| {
            json!({
                "rel": edge.rel,
                "from": edge.from,
                "to": edge.to,
                "branchId": edge.branch_id
            })
        })
        .collect::<Vec<_>>();

    Some(json!({
        "nodes": nodes_json,
        "edges": edges_json,
        "missing": {
            "branchFromThought": missing_branch_parents.into_iter().collect::<Vec<_>>(),
            "revisesThought": missing_revision_targets.into_iter().collect::<Vec<_>>()
        }
    }))
}

pub(crate) fn filter_trace_sequential_graph_to_entries(graph: &mut Value, entries: &[Value]) {
    let mut seqs = std::collections::BTreeSet::<i64>::new();
    for entry in entries {
        if let Some(seq) = entry.get("seq").and_then(|v| v.as_i64()) {
            seqs.insert(seq);
        }
    }

    let Some(obj) = graph.as_object_mut() else {
        return;
    };
    if seqs.is_empty() {
        // The caller's entries were reduced to "summary" stubs; keep the graph minimal too.
        obj.insert("nodes".to_string(), Value::Array(Vec::new()));
        obj.insert("edges".to_string(), Value::Array(Vec::new()));
        return;
    }

    let Some(nodes) = obj.get_mut("nodes").and_then(|v| v.as_array_mut()) else {
        return;
    };
    nodes.retain(|node| {
        node.get("seq")
            .and_then(|v| v.as_i64())
            .is_some_and(|seq| seqs.contains(&seq))
    });

    let mut thought_numbers = std::collections::BTreeSet::<i64>::new();
    for node in nodes.iter() {
        if let Some(n) = node.get("thoughtNumber").and_then(|v| v.as_i64()) {
            thought_numbers.insert(n);
        }
    }

    if let Some(edges) = obj.get_mut("edges").and_then(|v| v.as_array_mut()) {
        edges.retain(|edge| {
            let from = edge.get("from").and_then(|v| v.as_i64());
            let to = edge.get("to").and_then(|v| v.as_i64());
            match (from, to) {
                (Some(from), Some(to)) => {
                    thought_numbers.contains(&from) && thought_numbers.contains(&to)
                }
                _ => false,
            }
        });
    }
}
