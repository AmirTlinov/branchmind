#![forbid(unsafe_code)]

use serde_json::{Value, json};

const MIN_BUDGET_CHARS: usize = 2;

pub(crate) fn truncate_string_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}
pub(crate) fn json_len_chars(value: &Value) -> usize {
    serde_json::to_string(value).map(|s| s.len()).unwrap_or(0)
}
pub(super) fn payload_len_chars(value: &Value) -> usize {
    match value {
        Value::Object(map) => {
            if !map.contains_key("budget") {
                return json_len_chars(value);
            }
            let mut cloned = map.clone();
            cloned.remove("budget");
            json_len_chars(&Value::Object(cloned))
        }
        _ => json_len_chars(value),
    }
}
pub(crate) fn clamp_budget_max(max_chars: usize) -> (usize, bool) {
    if max_chars < MIN_BUDGET_CHARS {
        (MIN_BUDGET_CHARS, true)
    } else {
        (max_chars, false)
    }
}
pub(crate) fn truncate_string(value: &str, max_chars: usize) -> String {
    if value.len() <= max_chars {
        return value.to_string();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}
fn get_mut_at<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for key in path {
        let next = current.as_object_mut()?.get_mut(*key)?;
        current = next;
    }
    Some(current)
}
pub(super) fn get_object_mut_at<'a>(
    value: &'a mut Value,
    path: &[&str],
) -> Option<&'a mut serde_json::Map<String, Value>> {
    get_mut_at(value, path)?.as_object_mut()
}
pub(super) fn get_array_mut_at<'a>(
    value: &'a mut Value,
    path: &[&str],
) -> Option<&'a mut Vec<Value>> {
    get_mut_at(value, path)?.as_array_mut()
}
pub(super) fn get_array_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Vec<Value>> {
    let mut current = value;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    current.as_array()
}
pub(crate) fn drop_fields_at(value: &mut Value, path: &[&str], fields: &[&str]) -> bool {
    let Some(obj) = get_object_mut_at(value, path) else {
        return false;
    };
    let mut changed = false;
    for field in fields {
        if obj.remove(*field).is_some() {
            changed = true;
        }
    }
    changed
}
pub(crate) fn retain_one_at(value: &mut Value, path: &[&str], keep_last: bool) -> bool {
    let Some(arr) = get_array_mut_at(value, path) else {
        return false;
    };
    if arr.len() <= 1 {
        return false;
    }
    let kept = if keep_last {
        arr.pop().unwrap()
    } else {
        arr.remove(0)
    };
    arr.clear();
    arr.push(kept);
    true
}
pub(crate) fn mark_trimmed(list: &mut Vec<String>, field: &str) {
    if !list.iter().any(|item| item == field) {
        list.push(field.to_string());
    }
}
pub(crate) fn attach_budget(value: &mut Value, max_chars: usize, truncated: bool) -> usize {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "budget".to_string(),
            json!({
                "max_chars": max_chars,
                "used_chars": 0,
                "truncated": truncated
            }),
        );
    }

    let mut used = json_len_chars(value);
    for _ in 0..4 {
        if let Some(budget) = value
            .as_object_mut()
            .and_then(|obj| obj.get_mut("budget"))
            .and_then(|v| v.as_object_mut())
        {
            budget.insert(
                "used_chars".to_string(),
                Value::Number(serde_json::Number::from(used as u64)),
            );
            budget.insert("truncated".to_string(), Value::Bool(truncated));
        }
        let next = payload_len_chars(value);
        if next == used {
            break;
        }
        used = next;
    }

    used
}
