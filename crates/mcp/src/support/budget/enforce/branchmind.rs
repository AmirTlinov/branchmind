use serde_json::Value;

use super::super::shared::{json_len_chars, payload_len_chars, truncate_string};

pub(crate) fn enforce_branchmind_show_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = payload_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    if value.get("entries").is_some() {
        if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
            for entry in entries.iter_mut() {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    continue;
                }
                let Some(content) = entry
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                else {
                    continue;
                };
                let shorter = truncate_string(&content, 256);
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("content".to_string(), Value::String(shorter));
                }
            }
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }

        if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
            for entry in entries.iter_mut() {
                if entry.get("kind").and_then(|v| v.as_str()) != Some("note") {
                    continue;
                }
                if let Some(obj) = entry.as_object_mut()
                    && obj.contains_key("meta")
                {
                    obj.insert("meta".to_string(), Value::Null);
                }
            }
        }
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }

        loop {
            used = json_len_chars(value);
            if used <= max_chars {
                return (used, truncated);
            }
            let removed =
                if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
                    if entries.is_empty() {
                        false
                    } else {
                        entries.remove(0);
                        true
                    }
                } else {
                    false
                };
            if !removed {
                break;
            }
            truncated = true;
        }
    }

    if value.get("last_doc_entry").is_some() {
        if let Some(obj) = value.as_object_mut() {
            obj.remove("last_doc_entry");
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    if value.get("last_event").is_some() {
        if let Some(obj) = value.as_object_mut() {
            obj.remove("last_event");
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    (used, truncated)
}

pub(crate) fn enforce_branchmind_branch_list_budget(
    value: &mut Value,
    max_chars: usize,
) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    if value.get("branches").is_some() {
        loop {
            used = json_len_chars(value);
            if used <= max_chars {
                break;
            }
            let removed =
                if let Some(branches) = value.get_mut("branches").and_then(|v| v.as_array_mut()) {
                    if branches.is_empty() {
                        false
                    } else {
                        branches.remove(0);
                        true
                    }
                } else {
                    false
                };
            if !removed {
                break;
            }
            truncated = true;
        }
    }

    (used, truncated)
}
