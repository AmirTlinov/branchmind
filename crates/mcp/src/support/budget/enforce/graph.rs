use serde_json::Value;

use super::super::shared::json_len_chars;

pub(crate) fn enforce_graph_list_budget(
    value: &mut Value,
    list_key: &str,
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
    if value.get(list_key).is_some() {
        loop {
            used = json_len_chars(value);
            if used <= max_chars {
                break;
            }
            let removed = if let Some(arr) = value.get_mut(list_key).and_then(|v| v.as_array_mut())
            {
                arr.pop().is_some()
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

pub(crate) fn enforce_graph_query_budget(value: &mut Value, max_chars: usize) -> (usize, bool) {
    use std::collections::HashSet;

    if max_chars == 0 {
        return (json_len_chars(value), false);
    }

    let mut used = json_len_chars(value);
    if used <= max_chars {
        return (used, false);
    }

    let mut truncated = false;

    loop {
        used = json_len_chars(value);
        if used <= max_chars {
            break;
        }

        let removed_edge =
            if let Some(edges) = value.get_mut("edges").and_then(|v| v.as_array_mut()) {
                edges.pop().is_some()
            } else {
                false
            };
        if removed_edge {
            truncated = true;
            continue;
        }

        let removed_node =
            if let Some(nodes) = value.get_mut("nodes").and_then(|v| v.as_array_mut()) {
                nodes.pop().is_some()
            } else {
                false
            };
        if removed_node {
            truncated = true;

            let mut node_ids = HashSet::new();
            if let Some(nodes) = value.get("nodes").and_then(|v| v.as_array()) {
                for node in nodes {
                    if let Some(id) = node.get("id").and_then(|v| v.as_str()) {
                        node_ids.insert(id.to_string());
                    }
                }
            }

            if let Some(edges) = value.get_mut("edges").and_then(|v| v.as_array_mut()) {
                edges.retain(|edge| {
                    let from = edge.get("from").and_then(|v| v.as_str()).unwrap_or("");
                    let to = edge.get("to").and_then(|v| v.as_str()).unwrap_or("");
                    node_ids.contains(from) && node_ids.contains(to)
                });
            }

            continue;
        }

        break;
    }

    (used, truncated)
}
