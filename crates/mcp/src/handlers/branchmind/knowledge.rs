#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

fn normalize_anchor_tag(raw: &str) -> Result<String, Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ai_error("INVALID_INPUT", "anchor must not be empty"));
    }
    let candidate = if raw.starts_with(ANCHOR_TAG_PREFIX) {
        raw.to_string()
    } else {
        format!("{ANCHOR_TAG_PREFIX}{raw}")
    };
    normalize_anchor_id_tag(&candidate)
        .ok_or_else(|| ai_error("INVALID_INPUT", "anchor must be a valid slug (a:<slug>)"))
}

impl McpServer {
    pub(crate) fn tool_branchmind_knowledge_list(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let _workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let include_drafts = match optional_bool(args_obj, "include_drafts") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let include_history = match optional_bool(args_obj, "include_history") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let all_lanes = match optional_bool(args_obj, "all_lanes") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };

        let mut tags_all = match optional_string_values(args_obj, "tags_all") {
            Ok(v) => v.unwrap_or_default(),
            Err(resp) => return resp,
        };
        let anchor = match optional_string(args_obj, "anchor") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let key = match optional_string(args_obj, "key") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if let Some(anchor) = anchor {
            let tag = match normalize_anchor_tag(&anchor) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            if !tags_all.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                tags_all.push(tag);
            }
        }
        if let Some(key) = key {
            let key = key.trim();
            if key.is_empty() {
                return ai_error("INVALID_INPUT", "key must not be empty");
            }
            let candidate = if key.starts_with(KEY_TAG_PREFIX) {
                key.to_string()
            } else {
                format!("{KEY_TAG_PREFIX}{key}")
            };
            let Some(tag) = normalize_key_id_tag(&candidate) else {
                return ai_error("INVALID_INPUT", "key must be a valid slug (k:<slug>)");
            };
            if !tags_all.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                tags_all.push(tag);
            }
        }

        let include_drafts = include_drafts || all_lanes;
        if tags_all.is_empty() && !include_drafts {
            tags_all.push(VIS_TAG_CANON.to_string());
        }

        // Latest-only is the low-noise default for knowledge management. To avoid returning fewer
        // unique items than `limit` when there are multiple historical versions per key/title,
        // overscan a bounded multiplier and dedup in-memory.
        let raw_limit = args_obj
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let query_limit = if include_history {
            raw_limit
        } else {
            (raw_limit.saturating_mul(4)).clamp(1, 200)
        };

        let mut obj = args_obj.clone();
        obj.insert("types".to_string(), json!(["knowledge"]));
        obj.insert("include_drafts".to_string(), Value::Bool(include_drafts));
        if query_limit != raw_limit {
            obj.insert("limit".to_string(), Value::Number(query_limit.into()));
        }
        if !tags_all.is_empty() {
            obj.insert(
                "tags_all".to_string(),
                Value::Array(tags_all.into_iter().map(Value::String).collect()),
            );
        }

        let mut response = self.tool_branchmind_think_query(Value::Object(obj));
        if include_history {
            return response;
        }

        // Dedup versions: prefer a stable identity when possible (anchor+key tags), otherwise
        // fall back to (anchor,title). Keep the most recent version (`last_seq`).
        let Some(result_obj) = response.get_mut("result").and_then(|v| v.as_object_mut()) else {
            return response;
        };

        fn pick_tag(tags: &[Value], prefix: &str) -> Option<String> {
            for tag in tags {
                let Some(tag) = tag.as_str() else { continue };
                let trimmed = tag.trim();
                if trimmed.len() >= prefix.len()
                    && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix)
                {
                    return Some(trimmed.to_ascii_lowercase());
                }
            }
            None
        }

        fn normalized_title(card: &Value) -> Option<String> {
            let title = card.get("title").and_then(|v| v.as_str())?.trim();
            if title.is_empty() {
                return None;
            }
            Some(title.to_ascii_lowercase())
        }

        fn card_last_seq(card: &Value) -> i64 {
            card.get("last_seq").and_then(|v| v.as_i64()).unwrap_or(0)
        }

        let new_count = {
            let Some(cards) = result_obj.get_mut("cards").and_then(|v| v.as_array_mut()) else {
                return response;
            };

            let mut by_group = std::collections::BTreeMap::<String, Value>::new();
            for card in cards.iter() {
                let tags = card
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let anchor = pick_tag(&tags, ANCHOR_TAG_PREFIX);
                let key = pick_tag(&tags, KEY_TAG_PREFIX);
                let title = normalized_title(card);

                let group = if let Some(key) = key {
                    if let Some(anchor) = anchor {
                        format!("{anchor}|{key}")
                    } else {
                        key
                    }
                } else if let Some(title) = title {
                    if let Some(anchor) = anchor {
                        format!("{anchor}|t:{title}")
                    } else {
                        format!("t:{title}")
                    }
                } else {
                    // Worst-case: do not drop cards that don't have stable identity signals.
                    card.get("id")
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| format!("unknown:{}", card_last_seq(card)))
                };

                match by_group.get(&group) {
                    None => {
                        by_group.insert(group, card.clone());
                    }
                    Some(existing) => {
                        if card_last_seq(card) > card_last_seq(existing) {
                            by_group.insert(group, card.clone());
                        }
                    }
                }
            }

            let mut unique = by_group.into_values().collect::<Vec<_>>();
            unique.sort_by(|a, b| {
                card_last_seq(b).cmp(&card_last_seq(a)).then_with(|| {
                    let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    a_id.cmp(b_id)
                })
            });
            if raw_limit > 0 && unique.len() > raw_limit {
                unique.truncate(raw_limit);
            }

            cards.clear();
            cards.extend(unique);
            cards.len()
        };
        if let Some(pagination) = result_obj
            .get_mut("pagination")
            .and_then(|v| v.as_object_mut())
        {
            pagination.insert(
                "limit".to_string(),
                Value::Number(serde_json::Number::from(raw_limit as u64)),
            );
            pagination.insert(
                "count".to_string(),
                Value::Number(serde_json::Number::from(new_count as u64)),
            );
        }

        response
    }
}
