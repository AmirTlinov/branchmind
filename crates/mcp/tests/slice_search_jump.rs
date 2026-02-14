#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn tasks_search_can_find_and_open_slices() {
    let mut server = Server::start_initialized_with_args(
        "tasks_search_can_find_and_open_slices",
        &["--slice-plans-v1"],
    );

    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": "ws_slice_search_jump",
            "kind": "plan",
            "title": "Plan slice search jump"
        } } }
    }));
    let plan = extract_tool_text(&plan);
    assert_eq!(plan.get("success").and_then(|v| v.as_bool()), Some(true));
    let plan_id = plan
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let propose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.propose_next", "args": {
            "workspace": "ws_slice_search_jump",
            "plan": plan_id,
            "objective": "Create a minimal slice for search/open smoke"
        } } }
    }));
    let propose = extract_tool_text(&propose);
    assert_eq!(propose.get("success").and_then(|v| v.as_bool()), Some(true));
    let plan_rev = propose
        .get("result")
        .and_then(|v| v.get("plan"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let spec = propose
        .get("result")
        .and_then(|v| v.get("slice_plan_spec"))
        .cloned()
        .expect("slice_plan_spec");

    let apply = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.apply", "args": {
            "workspace": "ws_slice_search_jump",
            "plan": propose.get("result").and_then(|v| v.get("plan")).and_then(|v| v.get("id")).and_then(|v| v.as_str()).unwrap_or(""),
            "expected_revision": plan_rev,
            "policy": "fail_closed",
            "slice_plan_spec": spec
        } } }
    }));
    let apply = extract_tool_text(&apply);
    assert_eq!(apply.get("success").and_then(|v| v.as_bool()), Some(true));
    let slice_id = apply
        .get("result")
        .and_then(|v| v.get("slice"))
        .and_then(|v| v.get("slice_id"))
        .and_then(|v| v.as_str())
        .expect("slice_id")
        .to_string();

    let search = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "workspace": "ws_slice_search_jump", "op": "search", "args": {
            "text": slice_id.clone(),
            "limit": 10
        } } }
    }));
    let search = extract_tool_text(&search);
    assert_eq!(search.get("success").and_then(|v| v.as_bool()), Some(true));
    let hits = search
        .get("result")
        .and_then(|v| v.get("hits"))
        .and_then(|v| v.as_array())
        .expect("hits");
    assert!(
        hits.iter()
            .any(|h| h.get("kind").and_then(|v| v.as_str()) == Some("slice")),
        "hits must include a slice hit: {hits:?}"
    );

    let actions = search
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("open")
                && a.get("args")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    == Some(slice_id.as_str())
                && a.get("args")
                    .and_then(|v| v.get("include_content"))
                    .and_then(|v| v.as_bool())
                    == Some(true)
        }),
        "actions must include open(id=SLC-..) for the slice: {actions:?}"
    );

    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": {
            "workspace": "ws_slice_search_jump",
            "id": slice_id.clone(),
            "include_content": true
        } }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(opened.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        opened
            .get("result")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("slice"),
        "open(id=SLC-..) should return kind=slice; got: {opened}"
    );
}

#[test]
fn open_slice_compact_budget_keeps_binding_under_fat_objective() {
    let mut server = Server::start_initialized_with_args(
        "open_slice_compact_budget_keeps_binding_under_fat_objective",
        &["--slice-plans-v1"],
    );

    let ws = "ws_open_slice_budget_trim";

    let plan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.plan.create", "args": {
            "workspace": ws,
            "kind": "plan",
            "title": "Plan open(SLC) compact budget trimming"
        } } }
    }));
    let plan = extract_tool_text(&plan);
    assert_eq!(plan.get("success").and_then(|v| v.as_bool()), Some(true));
    let plan_id = plan
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let propose = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.propose_next", "args": {
            "workspace": ws,
            "plan": plan_id,
            "objective": "Base objective (will be replaced by a fat objective)"
        } } }
    }));
    let propose = extract_tool_text(&propose);
    assert_eq!(propose.get("success").and_then(|v| v.as_bool()), Some(true));
    let plan_rev = propose
        .get("result")
        .and_then(|v| v.get("plan"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let mut spec = propose
        .get("result")
        .and_then(|v| v.get("slice_plan_spec"))
        .cloned()
        .expect("slice_plan_spec");

    // Replace objective with an intentionally "fat" string to stress compact max_chars trimming.
    let fat_objective =
        "FAT OBJECTIVE: open(SLC) compact must keep binding under tight max_chars. ".repeat(200);
    spec.as_object_mut().expect("spec object").insert(
        "objective".to_string(),
        serde_json::Value::String(fat_objective),
    );
    let fat_title =
        "FAT TITLE: slice title is intentionally long and must be trimmed/dropped under budget. "
            .repeat(120);
    spec.as_object_mut()
        .expect("spec object")
        .insert("title".to_string(), serde_json::Value::String(fat_title));

    let apply = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.slices.apply", "args": {
            "workspace": ws,
            "plan": propose.get("result").and_then(|v| v.get("plan")).and_then(|v| v.get("id")).and_then(|v| v.as_str()).unwrap_or(""),
            "expected_revision": plan_rev,
            "policy": "fail_closed",
            "slice_plan_spec": spec
        } } }
    }));
    let apply = extract_tool_text(&apply);
    assert_eq!(apply.get("success").and_then(|v| v.as_bool()), Some(true));
    let slice_id = apply
        .get("result")
        .and_then(|v| v.get("slice"))
        .and_then(|v| v.get("slice_id"))
        .and_then(|v| v.as_str())
        .expect("slice_id")
        .to_string();

    // Tight budget should still preserve binding (id/kind/workspace + slice binding), not "signal=minimal".
    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "open", "arguments": {
            "workspace": ws,
            "id": slice_id.clone(),
            "include_content": true,
            "verbosity": "compact",
            "max_chars": 900
        } }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(opened.get("success").and_then(|v| v.as_bool()), Some(true));

    let result = opened.get("result").expect("open result");
    assert!(
        result.get("signal").is_none(),
        "open(compact) must not reduce to minimal signal; got: {opened}"
    );
    assert_eq!(
        result.get("id").and_then(|v| v.as_str()),
        Some(slice_id.as_str()),
        "binding id must be preserved under max_chars; got: {opened}"
    );
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("slice"),
        "binding kind must be preserved under max_chars; got: {opened}"
    );
    assert_eq!(
        result.get("workspace").and_then(|v| v.as_str()),
        Some(ws),
        "binding workspace must be preserved under max_chars; got: {opened}"
    );

    let objective = result
        .get("slice")
        .and_then(|v| v.get("objective"))
        .and_then(|v| v.as_str())
        .expect("slice.objective should survive (trimmed) in compact output");
    assert!(
        objective.len() < 500,
        "objective must be trimmed in compact budget; got len {}: {objective}",
        objective.len()
    );
    assert!(
        objective.ends_with("..."),
        "objective should show visible truncation; got: {objective}"
    );

    // Ultra-tight budgets should still preserve binding (no signal=minimal) by dropping
    // non-essential slice fields (objective/title/budgets/timestamps/etc).
    let opened = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "open", "arguments": {
            "workspace": ws,
            "id": slice_id.clone(),
            "include_content": true,
            "verbosity": "compact",
            "max_chars": 200
        } }
    }));
    let opened = extract_tool_text(&opened);
    assert_eq!(opened.get("success").and_then(|v| v.as_bool()), Some(true));

    let result = opened.get("result").expect("open result");
    assert!(
        result.get("signal").is_none(),
        "open(compact) must not reduce to minimal signal under ultra-tight budget; got: {opened}"
    );
    assert_eq!(
        result.get("id").and_then(|v| v.as_str()),
        Some(slice_id.as_str()),
        "binding id must be preserved under ultra-tight max_chars; got: {opened}"
    );
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("slice"),
        "binding kind must be preserved under ultra-tight max_chars; got: {opened}"
    );
    assert_eq!(
        result.get("workspace").and_then(|v| v.as_str()),
        Some(ws),
        "binding workspace must be preserved under ultra-tight max_chars; got: {opened}"
    );
    assert_eq!(
        result
            .get("slice")
            .and_then(|v| v.get("plan_id"))
            .and_then(|v| v.as_str()),
        Some(plan_id.as_str()),
        "slice.plan_id must survive ultra-tight budgets; got: {opened}"
    );
    assert!(
        result
            .get("slice")
            .and_then(|v| v.get("slice_task_id"))
            .and_then(|v| v.as_str())
            .is_some(),
        "slice.slice_task_id must survive ultra-tight budgets; got: {opened}"
    );
}
