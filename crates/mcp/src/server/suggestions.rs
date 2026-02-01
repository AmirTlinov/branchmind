#![forbid(unsafe_code)]

use serde_json::Value;
use std::collections::HashSet;

/// Tool names that are advertised by the server for a given toolset.
///
/// Used for sanitizing embedded "engine actions" (legacy call-tool style) so they do not
/// reference hidden/unavailable tools without also disclosing the next tier.
pub(super) fn advertised_tool_names(toolset: crate::Toolset) -> HashSet<String> {
    crate::tools::tool_definitions(toolset)
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<HashSet<_>>()
}

pub(super) fn sanitize_engine_calls_in_value(
    value: &mut Value,
    advertised: &HashSet<String>,
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) {
    match value {
        Value::Object(obj) => {
            for (key, child) in obj.iter_mut() {
                if key == "engine" {
                    sanitize_engine_calls_in_engine(child, advertised, core_tools, daily_tools);
                } else {
                    sanitize_engine_calls_in_value(child, advertised, core_tools, daily_tools);
                }
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                sanitize_engine_calls_in_value(child, advertised, core_tools, daily_tools);
            }
        }
        _ => {}
    }
}

fn sanitize_engine_calls_in_engine(
    engine: &mut Value,
    advertised: &HashSet<String>,
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) {
    let Some(engine_obj) = engine.as_object_mut() else {
        return;
    };
    let Some(actions) = engine_obj.get_mut("actions").and_then(|v| v.as_array_mut()) else {
        return;
    };

    for action in actions.iter_mut() {
        let Some(action_obj) = action.as_object_mut() else {
            continue;
        };
        let Some(calls) = action_obj.get_mut("calls").and_then(|v| v.as_array_mut()) else {
            continue;
        };

        let mut hidden_targets = Vec::new();
        for call in calls.iter() {
            if call.get("action").and_then(|v| v.as_str()) != Some("call_tool") {
                continue;
            }
            let target = call
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if target.is_empty() {
                continue;
            }
            if !advertised.contains(target) {
                hidden_targets.push(target.to_string());
            }
        }

        let Some(escalation_toolset) =
            escalation_toolset_for_hidden(&hidden_targets, core_tools, daily_tools)
        else {
            continue;
        };

        let already_has_disclosure = calls.iter().any(|s| {
            s.get("action").and_then(|v| v.as_str()) == Some("call_method")
                && s.get("method").and_then(|v| v.as_str()) == Some("tools/list")
        });
        if !already_has_disclosure {
            calls.insert(
                0,
                crate::suggest_method(
                    "tools/list",
                    "Reveal the next toolset tier for this engine action.",
                    "high",
                    serde_json::json!({ "toolset": escalation_toolset }),
                ),
            );
        }

        let mut seen = HashSet::new();
        calls.retain(|s| match serde_json::to_string(s) {
            Ok(key) => seen.insert(key),
            Err(_) => true,
        });
    }
}

fn escalation_toolset_for_hidden(
    hidden_targets: &[String],
    core_tools: &HashSet<String>,
    daily_tools: &HashSet<String>,
) -> Option<&'static str> {
    let mut needs_daily = false;
    let mut needs_full = false;
    for target in hidden_targets {
        if core_tools.contains(target) {
            continue;
        }
        if daily_tools.contains(target) {
            needs_daily = true;
        } else {
            needs_full = true;
        }
    }

    if needs_full {
        Some("full")
    } else if needs_daily {
        Some("daily")
    } else {
        None
    }
}
