#![forbid(unsafe_code)]

use super::super::super::step_context::ResolvedStepContext;
use crate::*;
use serde_json::{Value, json};

pub(super) struct WatchCapsuleArgs<'a> {
    pub(super) workspace: &'a WorkspaceId,
    pub(super) branch: &'a str,
    pub(super) graph_doc: &'a str,
    pub(super) trace_doc: &'a str,
    pub(super) agent_id: Option<&'a str>,
    pub(super) all_lanes: bool,
    pub(super) step_ctx: Option<&'a ResolvedStepContext>,
    pub(super) engine: Option<&'a Value>,
}

fn extract_engine_actions(engine: Option<&Value>) -> (Value, Value) {
    let Some(engine) = engine else {
        return (Value::Null, Value::Null);
    };
    let Some(actions) = engine.get("actions").and_then(|v| v.as_array()) else {
        return (Value::Null, Value::Null);
    };
    let primary = actions.first().cloned().unwrap_or(Value::Null);
    let backup = actions.get(1).cloned().unwrap_or(Value::Null);
    (primary, backup)
}

fn extract_engine_signals(engine: Option<&Value>, limit: usize) -> Vec<Value> {
    let Some(engine) = engine else {
        return Vec::new();
    };
    let Some(signals) = engine.get("signals").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    if limit == 0 {
        return Vec::new();
    }
    signals.iter().take(limit).cloned().collect::<Vec<_>>()
}

pub(super) fn build_watch_capsule(args: WatchCapsuleArgs<'_>) -> Value {
    let step = args.step_ctx.map(|ctx| {
        json!({
            "task_id": ctx.task_id,
            "step_id": ctx.step.step_id,
            "path": ctx.step.path,
            "tag": ctx.step_tag
        })
    });

    let (primary, backup) = extract_engine_actions(args.engine);
    let signals = extract_engine_signals(args.engine, 3);
    let lane = if args.all_lanes {
        json!({ "kind": "all" })
    } else {
        lane_meta_value(args.agent_id)
    };

    json!({
        "type": "watch_capsule",
        "version": 1,
        "where": {
            "workspace": args.workspace.as_str(),
            "branch": args.branch,
            "docs": { "graph": args.graph_doc, "trace": args.trace_doc },
            "lane": lane,
            "step": step.unwrap_or(Value::Null)
        },
        "why": {
            "signals": signals
        },
        "next": {
            "primary": primary,
            "backup": backup
        }
    })
}

pub(super) fn filter_watch_capsule_to_cards(capsule: &mut Value, cards_snapshot: &[Value]) {
    let Some(capsule_obj) = capsule.as_object_mut() else {
        return;
    };
    let Some(next) = capsule_obj.get_mut("next").and_then(|v| v.as_object_mut()) else {
        return;
    };

    let mut kept = std::collections::BTreeSet::<String>::new();
    for card in cards_snapshot {
        if let Some(id) = card.get("id").and_then(|v| v.as_str()) {
            kept.insert(id.to_string());
        }
    }

    for key in ["primary", "backup"] {
        let Some(action) = next.get_mut(key) else {
            continue;
        };
        if action.is_null() {
            continue;
        }
        let refs = action
            .get("refs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut ok = false;
        for r in &refs {
            if r.get("kind").and_then(|v| v.as_str()) != Some("card") {
                continue;
            }
            let Some(id) = r.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if kept.contains(id) {
                ok = true;
                break;
            }
        }
        if !refs.is_empty() && !ok {
            *action = Value::Null;
        }
    }
}
