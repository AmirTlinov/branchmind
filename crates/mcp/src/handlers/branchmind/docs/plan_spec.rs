#![forbid(unsafe_code)]

use crate::*;
use bm_storage::DocEntryRow;
use serde_json::{Map, Value, json};

pub(crate) fn plan_spec_doc_for_target(target_id: &str) -> String {
    format!("plan_spec:{target_id}")
}

pub(crate) fn load_latest_plan_spec(
    store: &mut bm_storage::SqliteStore,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
) -> Result<Option<(DocEntryRow, Value)>, Value> {
    let slice = store
        .doc_show_tail(workspace, branch, doc, None, 1)
        .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

    let Some(entry) = slice.entries.into_iter().last() else {
        return Ok(None);
    };

    let Some(content) = entry.content.as_deref() else {
        return Ok(None);
    };

    let parsed: Value = serde_json::from_str(content).map_err(|_| {
        ai_error_with(
            "INVALID_INPUT",
            "latest plan_spec entry is not valid JSON",
            Some("Rewrite plan_spec entry with canonical JSON object content."),
            Vec::new(),
        )
    })?;

    let canonical = canonicalize_json(parsed);
    let Some(_) = canonical.as_object() else {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "plan_spec content must be a JSON object",
            Some("Use canonical plan_spec object with slices/tasks fields."),
            Vec::new(),
        ));
    };

    Ok(Some((entry, canonical)))
}

pub(crate) fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut keys = obj.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut out = Map::<String, Value>::new();
            for key in keys {
                if let Some(v) = obj.get(&key) {
                    out.insert(key, canonicalize_json(v.clone()));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

pub(crate) fn structural_diff_paths(left: &Value, right: &Value) -> Vec<String> {
    let mut out = Vec::<String>::new();
    collect_diff_paths("$", left, right, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_diff_paths(path: &str, left: &Value, right: &Value, out: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys = a.keys().cloned().collect::<Vec<_>>();
            for key in b.keys() {
                if !a.contains_key(key) {
                    keys.push(key.clone());
                }
            }
            keys.sort();
            keys.dedup();

            for key in keys {
                let next = format!("{path}.{}", key);
                match (a.get(&key), b.get(&key)) {
                    (Some(va), Some(vb)) => collect_diff_paths(&next, va, vb, out),
                    _ => out.push(next),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}.length"));
            }
            let common = std::cmp::min(a.len(), b.len());
            for idx in 0..common {
                let next = format!("{path}[{idx}]");
                collect_diff_paths(&next, &a[idx], &b[idx], out);
            }
        }
        _ => {
            if left != right {
                out.push(path.to_string());
            }
        }
    }
}

pub(super) fn plan_spec_diff_block(
    from_branch: &str,
    to_branch: &str,
    doc: &str,
    from: Option<(DocEntryRow, Value)>,
    to: Option<(DocEntryRow, Value)>,
) -> Value {
    match (from, to) {
        (Some((from_entry, from_value)), Some((to_entry, to_value))) => {
            let changed_paths = structural_diff_paths(&from_value, &to_value);
            json!({
                "doc_kind": "plan_spec",
                "doc": doc,
                "from": {
                    "branch": from_branch,
                    "seq": from_entry.seq,
                    "ts_ms": from_entry.ts_ms
                },
                "to": {
                    "branch": to_branch,
                    "seq": to_entry.seq,
                    "ts_ms": to_entry.ts_ms
                },
                "status": if changed_paths.is_empty() { "identical" } else { "different" },
                "changed_paths": changed_paths,
                "changed_count": changed_paths.len()
            })
        }
        (Some((from_entry, _)), None) => json!({
            "doc_kind": "plan_spec",
            "doc": doc,
            "from": { "branch": from_branch, "seq": from_entry.seq, "ts_ms": from_entry.ts_ms },
            "to": { "branch": to_branch },
            "status": "missing_to",
            "changed_paths": ["$"],
            "changed_count": 1
        }),
        (None, Some((to_entry, _))) => json!({
            "doc_kind": "plan_spec",
            "doc": doc,
            "from": { "branch": from_branch },
            "to": { "branch": to_branch, "seq": to_entry.seq, "ts_ms": to_entry.ts_ms },
            "status": "missing_from",
            "changed_paths": ["$"],
            "changed_count": 1
        }),
        (None, None) => json!({
            "doc_kind": "plan_spec",
            "doc": doc,
            "from": { "branch": from_branch },
            "to": { "branch": to_branch },
            "status": "missing_both",
            "changed_paths": [],
            "changed_count": 0
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_diff_paths_detects_nested_drift() {
        let left = json!({
            "goal": "A",
            "slices": [
                {"title": "S1", "tasks": [{"title":"T1", "steps":["a","b"]}]}
            ]
        });
        let right = json!({
            "goal": "B",
            "slices": [
                {"title": "S1", "tasks": [{"title":"T1", "steps":["a","c"]}]},
                {"title": "S2", "tasks": []}
            ]
        });

        let paths = structural_diff_paths(&left, &right);
        assert!(paths.iter().any(|p| p == "$.goal"));
        assert!(paths.iter().any(|p| p == "$.slices.length"));
        assert!(paths.iter().any(|p| p == "$.slices[0].tasks[0].steps[1]"));
    }
}
