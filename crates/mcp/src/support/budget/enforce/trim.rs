use serde_json::Value;

use super::super::shared::{json_len_chars, truncate_string};

fn pop_array_at(value: &mut Value, path: &[&str], from_front: bool) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.len() == 1 {
        let Some(arr) = value.get_mut(path[0]).and_then(|v| v.as_array_mut()) else {
            return false;
        };
        if arr.is_empty() {
            return false;
        }
        if from_front {
            arr.remove(0);
        } else {
            arr.pop();
        }
        return true;
    }
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let Some(next) = obj.get_mut(path[0]) else {
        return false;
    };
    pop_array_at(next, &path[1..], from_front)
}

pub(crate) fn trim_array_to_budget(
    value: &mut Value,
    path: &[&str],
    max_chars: usize,
    from_front: bool,
) -> bool {
    if max_chars == 0 {
        return false;
    }
    let mut truncated = false;
    while json_len_chars(value) > max_chars {
        if !pop_array_at(value, path, from_front) {
            break;
        }
        truncated = true;
    }
    truncated
}

pub(crate) fn enforce_max_chars_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    if let Some(why) = value
        .get_mut("radar")
        .and_then(|v| v.get_mut("why"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        let shorter = truncate_string(&why, 256);
        if let Some(obj) = value.get_mut("radar").and_then(|v| v.as_object_mut()) {
            obj.insert("why".to_string(), Value::String(shorter));
        }
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    if let Some(target) = value.get_mut("target").and_then(|v| v.as_object_mut()) {
        target.remove("contract_data");
        target.remove("contract");
        target.remove("description");
        truncated = true;
        used = json_len_chars(value);
        if used <= max_chars {
            return (used, truncated);
        }
    }

    (used, truncated)
}
