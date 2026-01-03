#![forbid(unsafe_code)]

use serde_json::Value;

use super::shared::{drop_fields_at, get_array_mut_at, get_object_mut_at, truncate_string};

pub(crate) fn compact_doc_entries_at(
    value: &mut Value,
    path: &[&str],
    max_content: usize,
    drop_meta: bool,
    drop_title: bool,
    drop_format: bool,
) -> bool {
    let Some(entries) = get_array_mut_at(value, path) else {
        return false;
    };
    let mut changed = false;
    for entry in entries.iter_mut() {
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "note" {
            if let Some(content) = entry.get("content").and_then(|v| v.as_str()) {
                let shorter = truncate_string(content, max_content);
                if shorter != content {
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert("content".to_string(), Value::String(shorter));
                    }
                    changed = true;
                }
            }
            if drop_meta
                && let Some(obj) = entry.as_object_mut()
                && obj.contains_key("meta")
            {
                obj.insert("meta".to_string(), Value::Null);
                changed = true;
            }
            if drop_title {
                changed |= drop_fields_at(entry, &[], &["title"]);
            }
            if drop_format {
                changed |= drop_fields_at(entry, &[], &["format"]);
            }
        }
    }
    changed
}

pub(crate) fn minimalize_doc_entries_at(value: &mut Value, path: &[&str]) -> bool {
    let Some(entries) = get_array_mut_at(value, path) else {
        return false;
    };
    if entries.is_empty() {
        return false;
    }
    let mut changed = false;
    for entry in entries.iter_mut() {
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("note");
        let mut out = serde_json::Map::new();
        if let Some(seq) = entry.get("seq") {
            out.insert("seq".to_string(), seq.clone());
        }
        if let Some(ts_ms) = entry.get("ts_ms") {
            out.insert("ts_ms".to_string(), ts_ms.clone());
        }
        out.insert("kind".to_string(), Value::String(kind.to_string()));
        if kind == "note" {
            if let Some(title) = entry.get("title").and_then(|v| v.as_str()) {
                out.insert(
                    "title".to_string(),
                    Value::String(truncate_string(title, 64)),
                );
            } else if let Some(content) = entry.get("content").and_then(|v| v.as_str()) {
                out.insert(
                    "content".to_string(),
                    Value::String(truncate_string(content, 64)),
                );
            }
        } else {
            if let Some(event_type) = entry.get("event_type").and_then(|v| v.as_str()) {
                out.insert(
                    "event_type".to_string(),
                    Value::String(truncate_string(event_type, 64)),
                );
            }
            if let Some(task_id) = entry.get("task_id").and_then(|v| v.as_str()) {
                out.insert("task_id".to_string(), Value::String(task_id.to_string()));
            }
            if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                out.insert("path".to_string(), Value::String(path.to_string()));
            }
        }
        *entry = Value::Object(out);
        changed = true;
    }
    changed
}

pub(crate) fn compact_event_payloads_at(value: &mut Value, path: &[&str]) -> bool {
    let Some(events) = get_array_mut_at(value, path) else {
        return false;
    };
    let mut changed = false;
    for event in events.iter_mut() {
        if let Some(obj) = event.as_object_mut() {
            if obj.remove("payload").is_some() {
                changed = true;
            }
            if obj.remove("ts").is_some() {
                changed = true;
            }
        }
    }
    changed
}

fn retain_keys(obj: &mut serde_json::Map<String, Value>, keep: &[&str]) -> bool {
    let mut changed = false;
    let keys: Vec<String> = obj.keys().cloned().collect();
    for key in keys {
        if !keep.iter().any(|k| *k == key) {
            obj.remove(&key);
            changed = true;
        }
    }
    changed
}

pub(crate) fn compact_tasks_context_items(value: &mut Value) -> bool {
    let mut changed = false;
    if let Some(plans) = value.get_mut("plans").and_then(|v| v.as_array_mut()) {
        for plan in plans {
            if let Some(obj) = plan.as_object_mut() {
                changed |= retain_keys(obj, &["id", "kind", "title", "revision", "plan_progress"]);
            }
        }
    }
    if let Some(tasks) = value.get_mut("tasks").and_then(|v| v.as_array_mut()) {
        for task in tasks {
            if let Some(obj) = task.as_object_mut() {
                changed |= retain_keys(
                    obj,
                    &["id", "kind", "title", "status", "progress", "parent"],
                );
            }
        }
    }
    changed
}

pub(crate) fn compact_tasks_context_pagination(value: &mut Value) -> bool {
    let mut changed = false;
    for key in ["plans_pagination", "tasks_pagination"] {
        if let Some(obj) = value.get_mut(key).and_then(|v| v.as_object_mut()) {
            changed |= retain_keys(obj, &["cursor", "next_cursor", "count", "limit"]);
        }
    }
    changed
}

pub(crate) fn minimalize_task_events_at(value: &mut Value, path: &[&str]) -> bool {
    let Some(events) = get_array_mut_at(value, path) else {
        return false;
    };
    if events.is_empty() {
        return false;
    }
    let mut changed = false;
    for event in events.iter_mut() {
        if let Some(obj) = event.as_object_mut() {
            changed |= retain_keys(obj, &["event_id", "ts_ms", "task", "type", "path"]);
        }
    }
    changed
}

pub(crate) fn refresh_pagination_count(
    value: &mut Value,
    entries_path: &[&str],
    pagination_path: &[&str],
) {
    let count = value
        .get(entries_path[0])
        .and_then(|v| {
            let mut current = v;
            for key in &entries_path[1..] {
                current = current.get(*key)?;
            }
            current.as_array()
        })
        .map(|arr| arr.len());
    let Some(count) = count else {
        return;
    };
    let Some(pagination) = get_object_mut_at(value, pagination_path) else {
        return;
    };
    pagination.insert(
        "count".to_string(),
        Value::Number(serde_json::Number::from(count as u64)),
    );
}

pub(crate) fn set_pagination_total_at(
    value: &mut Value,
    pagination_path: &[&str],
    total: usize,
) -> bool {
    let Some(pagination) = get_object_mut_at(value, pagination_path) else {
        return false;
    };
    pagination.insert(
        "count".to_string(),
        Value::Number(serde_json::Number::from(total as u64)),
    );
    pagination.insert("has_more".to_string(), Value::Bool(true));
    true
}

pub(crate) fn refresh_trace_pagination_count(value: &mut Value) {
    let count = value
        .get("trace")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.len());
    let Some(count) = count else {
        return;
    };
    if let Some(pagination) = value
        .get_mut("trace")
        .and_then(|v| v.get_mut("pagination"))
        .and_then(|v| v.as_object_mut())
    {
        pagination.insert(
            "count".to_string(),
            Value::Number(serde_json::Number::from(count as u64)),
        );
    }
}

pub(crate) fn compact_trace_pagination(value: &mut Value) -> bool {
    let Some(pagination) = value
        .get_mut("trace")
        .and_then(|v| v.get_mut("pagination"))
        .and_then(|v| v.as_object_mut())
    else {
        return false;
    };
    let mut changed = false;
    for key in ["cursor", "next_cursor", "has_more", "limit"] {
        if pagination.remove(key).is_some() {
            changed = true;
        }
    }
    changed
}

pub(crate) fn compact_radar_for_budget(value: &mut Value) -> bool {
    let Some(radar) = value.get_mut("radar").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let mut changed = false;
    for key in ["verify", "next", "blockers"] {
        if radar.remove(key).is_some() {
            changed = true;
        }
    }
    changed
}

pub(crate) fn compact_target_for_budget(value: &mut Value) -> bool {
    let Some(target) = value.get_mut("target").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let mut changed = false;
    for key in ["created_at_ms", "updated_at_ms", "parent"] {
        if target.remove(key).is_some() {
            changed = true;
        }
    }
    changed
}
