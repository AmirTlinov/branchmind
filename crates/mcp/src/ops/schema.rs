#![forbid(unsafe_code)]

use crate::ops::{
    Action, ActionPriority, BudgetProfile, CommandRegistry, DocRef, OpError, OpResponse, Safety,
    SchemaSource, Stability, Tier,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub(crate) struct SchemaBundle {
    pub(crate) cmd: String,
    pub(crate) args_schema: Value,
    pub(crate) example_minimal_args: Value,
    pub(crate) example_valid_call: Value,
    pub(crate) doc_ref: DocRef,
    pub(crate) default_budget_profile: BudgetProfile,
    pub(crate) tier: Tier,
    pub(crate) stability: Stability,
    pub(crate) safety: Safety,
}

pub(crate) fn handler_input_schema(handler_name: &str) -> Option<Value> {
    handler_schemas().get(handler_name).cloned()
}

fn handler_schemas() -> &'static BTreeMap<String, Value> {
    static SCHEMAS: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    SCHEMAS.get_or_init(|| {
        let mut map = BTreeMap::<String, Value>::new();
        for def in crate::handlers::handler_definitions() {
            let Some(name) = def.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let schema = def.get("inputSchema").cloned().unwrap_or_else(|| json!({}));
            map.insert(name.to_string(), schema);
        }
        map
    })
}

pub(crate) fn schema_bundle_for_cmd(
    cmd: &str,
    workspace: Option<&str>,
) -> Result<SchemaBundle, OpError> {
    let registry = CommandRegistry::global();
    let Some(spec) = registry.find_by_cmd(cmd) else {
        return Err(OpError {
            code: "UNKNOWN_CMD".to_string(),
            message: format!("Unknown cmd: {cmd}"),
            recovery: Some(
                "Use system op=cmd.list to discover cmds (and system op=schema.get for exact schemas)."
                    .to_string(),
            ),
        });
    };

    let (args_schema, example_minimal_args) = match &spec.schema {
        SchemaSource::Custom {
            args_schema,
            example_minimal_args,
        } => (args_schema.clone(), example_minimal_args.clone()),
        SchemaSource::Handler => {
            let handler_name = spec.handler_name.as_deref().ok_or_else(|| OpError {
                code: "INTERNAL_ERROR".to_string(),
                message: format!("cmd {cmd} is Handler schema but has no handler_name"),
                recovery: None,
            })?;
            let mut schema = handler_input_schema(handler_name).ok_or_else(|| OpError {
                code: "INTERNAL_ERROR".to_string(),
                message: format!("missing handler schema for {handler_name}"),
                recovery: Some("Check registry handler_name mapping for this cmd.".to_string()),
            })?;

            // v1 envelope: `workspace` lives outside of args. Most handlers still declare
            // workspace in their inputSchema; strip it to keep schema-on-demand examples minimal
            // and aligned with the portal contract.
            if let Some(obj) = schema.as_object_mut() {
                if let Some(required) = obj.get_mut("required").and_then(|v| v.as_array_mut()) {
                    required.retain(|v| v.as_str() != Some("workspace"));
                }
                if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
                    props.remove("workspace");
                }
            }

            let mut example = generate_example_from_schema(&schema, None);
            if let Some(obj) = example.as_object_mut() {
                obj.remove("workspace");
            }
            (schema, example)
        }
    };

    let example_valid_call = {
        let mut env = serde_json::Map::new();
        if let Some(ws) = workspace {
            env.insert("workspace".to_string(), Value::String(ws.to_string()));
        }
        env.insert("op".to_string(), Value::String("call".to_string()));
        env.insert("cmd".to_string(), Value::String(cmd.to_string()));
        env.insert("args".to_string(), example_minimal_args.clone());
        env.insert(
            "budget_profile".to_string(),
            Value::String(spec.budget.default_profile.as_str().to_string()),
        );
        env.insert(
            "portal_view".to_string(),
            Value::String("compact".to_string()),
        );

        json!({
            "tool": spec.domain_tool.as_str(),
            "args": Value::Object(env),
        })
    };

    Ok(SchemaBundle {
        cmd: cmd.to_string(),
        args_schema,
        example_minimal_args,
        example_valid_call,
        doc_ref: spec.doc_ref.clone(),
        default_budget_profile: spec.budget.default_profile,
        tier: spec.tier,
        stability: spec.stability,
        safety: spec.safety,
    })
}

pub(crate) fn append_schema_actions(resp: &mut OpResponse, cmd: &str, workspace: Option<&str>) {
    let bundle = match schema_bundle_for_cmd(cmd, workspace) {
        Ok(v) => v,
        Err(_) => return,
    };

    let mut seen = BTreeSet::<String>::new();
    for a in resp.actions.iter() {
        seen.insert(a.action_id.clone());
    }

    let schema_action_id = format!("recover.schema.get::{cmd}");
    if seen.insert(schema_action_id.clone()) {
        resp.actions.push(Action {
            action_id: schema_action_id,
            priority: ActionPriority::High,
            tool: "system".to_string(),
            args: json!({
                "op": "schema.get",
                "args": { "cmd": cmd },
                "budget_profile": "portal"
            }),
            why: format!("Нужна точная схема args для {cmd}."),
            risk: "Низкий".to_string(),
        });
    }

    let Some(spec) = CommandRegistry::global().find_by_cmd(cmd) else {
        return;
    };
    let example_action_id = format!("recover.example.call::{cmd}");
    if seen.insert(example_action_id.clone()) {
        let mut env = serde_json::Map::new();
        if let Some(ws) = workspace {
            env.insert("workspace".to_string(), Value::String(ws.to_string()));
        }
        env.insert("op".to_string(), Value::String("call".to_string()));
        env.insert("cmd".to_string(), Value::String(cmd.to_string()));
        env.insert("args".to_string(), bundle.example_minimal_args.clone());
        env.insert(
            "budget_profile".to_string(),
            Value::String(spec.budget.default_profile.as_str().to_string()),
        );
        env.insert(
            "portal_view".to_string(),
            Value::String("compact".to_string()),
        );

        resp.actions.push(Action {
            action_id: example_action_id,
            priority: ActionPriority::High,
            tool: spec.domain_tool.as_str().to_string(),
            args: Value::Object(env),
            why: "Минимальный валидный пример вызова (placeholders).".to_string(),
            risk: "Низкий".to_string(),
        });
    }
}

fn generate_example_from_schema(schema: &Value, key_hint: Option<&str>) -> Value {
    if let Some(v) = schema.get("const") {
        return v.clone();
    }
    if let Some(arr) = schema.get("enum").and_then(|v| v.as_array())
        && let Some(first) = arr.first()
    {
        return first.clone();
    }

    // Common pattern in this repo: object schema with oneOf specifying alternative required sets.
    if is_objectish(schema) && schema.get("oneOf").is_some() {
        let extra_required = schema
            .get("oneOf")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("required"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        return generate_object_example(schema, &extra_required);
    }

    // For object schemas with additional guards (allOf/anyOf), keep base examples actionable.
    // Otherwise falling through to allOf first-subschema can degrade examples to "<value>".
    if is_objectish(schema) && schema.get("properties").is_some() {
        return generate_object_example(schema, &[]);
    }

    if let Some(arr) = schema.get("oneOf").and_then(|v| v.as_array())
        && let Some(first) = arr.first()
    {
        return generate_example_from_schema(first, key_hint);
    }
    if let Some(arr) = schema.get("anyOf").and_then(|v| v.as_array())
        && let Some(first) = arr.first()
    {
        return generate_example_from_schema(first, key_hint);
    }
    if let Some(arr) = schema.get("allOf").and_then(|v| v.as_array())
        && let Some(first) = arr.first()
    {
        // Best-effort: merge by selecting the first subschema.
        return generate_example_from_schema(first, key_hint);
    }

    let ty = schema
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            if schema.get("properties").is_some() {
                Some("object".to_string())
            } else if schema.get("items").is_some() {
                Some("array".to_string())
            } else {
                None
            }
        });

    match ty.as_deref() {
        Some("object") => generate_object_example(schema, &[]),
        Some("array") => {
            let items = schema.get("items").cloned().unwrap_or(Value::Null);
            let min_items = schema.get("minItems").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let count = min_items.max(0);
            let mut out = Vec::new();
            for _ in 0..count {
                out.push(generate_example_from_schema(&items, None));
            }
            Value::Array(out)
        }
        Some("boolean") => Value::Bool(false),
        Some("integer") => schema
            .get("minimum")
            .and_then(|v| v.as_i64())
            .map(|n| Value::Number(n.into()))
            .unwrap_or_else(|| Value::Number(0.into())),
        Some("number") => schema
            .get("minimum")
            .and_then(|v| v.as_f64())
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::Number(serde_json::Number::from_f64(0.0).unwrap())),
        Some("string") => {
            if key_hint == Some("workspace") {
                return Value::String("<workspace>".to_string());
            }
            if let Some(hint) = key_hint {
                return Value::String(format!("<{hint}>"));
            }
            Value::String("<value>".to_string())
        }
        _ => Value::String(
            key_hint
                .map(|k| format!("<{k}>"))
                .unwrap_or_else(|| "<value>".to_string()),
        ),
    }
}

fn is_objectish(schema: &Value) -> bool {
    if schema.get("properties").is_some() {
        return true;
    }
    schema
        .get("type")
        .and_then(|v| v.as_str())
        .is_some_and(|t| t == "object")
}

fn generate_object_example(schema: &Value, extra_required: &[String]) -> Value {
    let props = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let mut required = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    required.extend_from_slice(extra_required);
    required.sort();
    required.dedup();

    let mut out = serde_json::Map::new();
    for key in required {
        if key == "workspace" {
            out.insert(key, Value::String("<workspace>".to_string()));
            continue;
        }
        let prop_schema = props.get(&key).unwrap_or(&Value::Null);
        out.insert(
            key.clone(),
            generate_example_from_schema(prop_schema, Some(&key)),
        );
    }
    Value::Object(out)
}
