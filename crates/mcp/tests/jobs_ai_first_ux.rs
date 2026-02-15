#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::{Value, json};
use sha2::Digest as _;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn setup_plan_and_slice(server: &mut Server, workspace: &str, objective: &str) -> (String, String) {
    let plan_create = server.request(json!({
        "jsonrpc":"2.0","id":100,"method":"tools/call",
        "params":{"name":"tasks","arguments":{"workspace":workspace,"op":"call","cmd":"tasks.plan.create","args":{
            "kind":"plan",
            "title":"Jobs pipeline test plan"
        }}}
    }));
    let plan_text = extract_tool_text(&plan_create);
    assert_eq!(
        plan_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "tasks.plan.create must succeed; got: {plan_text}"
    );
    let plan_id = plan_text
        .get("result")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("plan id")
        .to_string();

    let propose = server.request(json!({
        "jsonrpc":"2.0","id":101,"method":"tools/call",
        "params":{"name":"tasks","arguments":{"workspace":workspace,"op":"call","cmd":"tasks.slices.propose_next","args":{
            "plan": plan_id.clone(),
            "objective": objective,
            "constraints": [],
            "policy":"fail_closed"
        }}}
    }));
    let propose_text = extract_tool_text(&propose);
    assert_eq!(
        propose_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "tasks.slices.propose_next must succeed; got: {propose_text}"
    );
    let slice_plan_spec = propose_text
        .get("result")
        .and_then(|v| v.get("slice_plan_spec"))
        .cloned()
        .expect("slice_plan_spec");

    let apply = server.request(json!({
        "jsonrpc":"2.0","id":102,"method":"tools/call",
        "params":{"name":"tasks","arguments":{"workspace":workspace,"op":"call","cmd":"tasks.slices.apply","args":{
            "plan": plan_id.clone(),
            "slice_plan_spec": slice_plan_spec,
            "policy":"fail_closed"
        }}}
    }));
    let apply_text = extract_tool_text(&apply);
    assert_eq!(
        apply_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "tasks.slices.apply must succeed; got: {apply_text}"
    );
    let slice_id = apply_text
        .get("result")
        .and_then(|v| v.get("slice"))
        .and_then(|v| v.get("slice_id"))
        .and_then(|v| v.as_str())
        .expect("slice_id")
        .to_string();

    (plan_id, slice_id)
}

#[test]
fn system_schema_list_compact_exposes_required_any_of_for_jobs_claim() {
    let mut server = Server::start_initialized_with_args(
        "system_schema_list_compact_exposes_required_any_of_for_jobs_claim",
        &["--workspace", "ws_jobs_schema"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.list",
                "args": { "portal": "jobs", "mode": "all", "q": "claim", "limit": 50 }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "system schema.list must succeed; got: {text}"
    );

    let schemas = text
        .get("result")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.as_array())
        .expect("result.schemas");
    let claim = schemas
        .iter()
        .find(|s| s.get("cmd").and_then(|v| v.as_str()) == Some("jobs.claim"))
        .expect("jobs.claim schema row");

    let required = claim
        .get("required")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        required.iter().any(|v| v.as_str() == Some("job")),
        "jobs.claim compact schema should expose required=[...,job,...]; got: {claim}"
    );

    let required_any_of = claim
        .get("required_any_of")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        required_any_of.iter().any(|alt| alt
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some("runner_id")))),
        "jobs.claim compact schema should expose required_any_of runner_id alternative; got: {claim}"
    );
    assert!(
        required_any_of.iter().any(|alt| alt
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some("runner")))),
        "jobs.claim compact schema should expose required_any_of runner alternative; got: {claim}"
    );
}

#[test]
fn jobs_open_unknown_id_provides_recovery_and_jobs_suggestion() {
    let mut server = Server::start_initialized_with_args(
        "jobs_open_unknown_id_provides_recovery_and_jobs_suggestion",
        &["--workspace", "ws_jobs_open_unknown"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_open_unknown",
                "op": "open",
                "args": { "job": "JOB-404" }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "jobs open with unknown id should fail; got: {text}"
    );
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ID"),
        "jobs open unknown id should return UNKNOWN_ID; got: {text}"
    );
    let recovery = text
        .get("error")
        .and_then(|v| v.get("recovery"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        recovery.contains("jobs op=radar"),
        "jobs open unknown id should include radar recovery hint; got recovery={recovery}"
    );
}

#[test]
fn jobs_create_unknown_args_fail_closed() {
    let mut server = Server::start_initialized_with_args(
        "jobs_create_unknown_args_fail_closed",
        &["--workspace", "ws_jobs_unknown_args"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_unknown_args",
                "op": "create",
                "args": {
                    "title": "Unknown args warning",
                    "prompt": "No-op",
                    "foo": "bar"
                }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "jobs create must fail on unknown args (fail-closed); got: {text}"
    );
    assert!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            == Some("INVALID_INPUT"),
        "expected INVALID_INPUT on unknown args; got: {text}"
    );
    let msg = text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        msg.contains("unknown args: foo"),
        "expected unknown args mention; got message={msg}"
    );
}

#[test]
fn jobs_radar_include_offline_false_is_explicit_and_warns_on_ignored_limit() {
    let mut server = Server::start_initialized_with_args(
        "jobs_radar_include_offline_false_is_explicit_and_warns_on_ignored_limit",
        &["--workspace", "ws_jobs_radar_offline"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_radar_offline",
                "op": "radar",
                "args": {
                    "include_offline": false,
                    "offline_limit": 5
                }
            }
        }
    }));

    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs radar should succeed; got: {text}"
    );
    let result = text.get("result").expect("result");
    assert!(
        result.get("runner_leases_offline").is_none(),
        "jobs radar include_offline=false should omit runner_leases_offline section; got: {result}"
    );
    let warnings = text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.get("code").and_then(|v| v.as_str()) == Some("ARG_IGNORED")),
        "jobs radar should warn when offline_limit is ignored; got warnings={warnings:?}"
    );
}

#[test]
fn jobs_tail_and_runner_heartbeat_op_aliases_are_dispatchable() {
    let mut server = Server::start_initialized_with_args(
        "jobs_tail_and_runner_heartbeat_op_aliases_are_dispatchable",
        &["--workspace", "ws_jobs_aliases"],
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_aliases",
                "op": "create",
                "args": {
                    "title": "Alias check",
                    "prompt": "Collect alias coverage."
                }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    assert_eq!(
        created_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs create should succeed; got: {created_text}"
    );
    let job_id = created_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string();

    let tail = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_aliases",
                "op": "tail",
                "args": { "job": job_id, "limit": 10 }
            }
        }
    }));
    let tail_text = extract_tool_text(&tail);
    assert_eq!(
        tail_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs op=tail alias should be dispatchable; got: {tail_text}"
    );

    let heartbeat = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_aliases",
                "op": "runner.heartbeat",
                "args": { "runner_id": "runner:alias-check", "status": "idle" }
            }
        }
    }));
    let heartbeat_text = extract_tool_text(&heartbeat);
    assert_eq!(
        heartbeat_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs op=runner.heartbeat alias should be dispatchable; got: {heartbeat_text}"
    );
}

#[test]
fn jobs_schema_list_includes_flagship_teamlead_commands() {
    let mut server = Server::start_initialized_with_args(
        "jobs_schema_list_includes_flagship_teamlead_commands",
        &["--workspace", "ws_jobs_schema_flagship"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.list",
                "args": { "portal": "jobs", "mode": "all", "limit": 200 }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));
    let schemas = text
        .get("result")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let cmds = schemas
        .iter()
        .filter_map(|s| s.get("cmd").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();

    for required in [
        "jobs.control.center",
        "jobs.macro.respond.inbox",
        "jobs.macro.dispatch.slice",
        "jobs.macro.dispatch.scout",
        "jobs.macro.dispatch.builder",
        "jobs.macro.dispatch.validator",
        "jobs.macro.enforce.proof",
        "jobs.macro.sync.team",
        "jobs.pipeline.ab.slice",
        "jobs.pipeline.context.review",
        "jobs.pipeline.gate",
        "jobs.pipeline.apply",
        "jobs.mesh.publish",
        "jobs.mesh.pull",
        "jobs.mesh.ack",
        "jobs.mesh.link",
    ] {
        assert!(
            cmds.contains(&required),
            "expected {required} in jobs schema list, got cmds={cmds:?}"
        );
    }
}

#[test]
fn jobs_pipeline_context_review_schema_is_exposed() {
    let mut server = Server::start_initialized_with_args(
        "jobs_pipeline_context_review_schema_is_exposed",
        &["--workspace", "ws_jobs_context_review_schema"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.get",
                "args": { "cmd": "jobs.pipeline.context.review" }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get for jobs.pipeline.context.review should succeed; got: {text}"
    );
    let required = text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let required = required
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    for field in ["task", "slice_id", "scout_pack_ref"] {
        assert!(
            required.contains(&field),
            "required field `{field}` missing in schema.get response: {text}"
        );
    }
}

#[test]
fn jobs_pipeline_ab_slice_dry_run_is_dispatchable_and_structured() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_pipeline_ab_slice_dry_run_is_dispatchable_and_structured",
        &["--agent-id", "manager"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.pipeline.ab.slice",
                "args": {
                    "task": "TASK-AB",
                    "anchor": "a:ab-ux",
                    "slice_id": "SLC-AB",
                    "objective": "Compare weak vs strong scout context for README.",
                    "dry_run": true,
                    "variant_a": { "scout_mode": "weak" },
                    "variant_b": { "scout_mode": "strong" }
                }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.pipeline.ab.slice dry_run must succeed; got: {text}"
    );

    let variants = text
        .get("result")
        .and_then(|v| v.get("variants"))
        .and_then(|v| v.as_object())
        .expect("variants object");
    let variant_a = variants
        .get("a")
        .and_then(|v| v.as_object())
        .expect("variant a");
    let variant_b = variants
        .get("b")
        .and_then(|v| v.as_object())
        .expect("variant b");
    assert_eq!(
        variant_a.get("quality_profile").and_then(|v| v.as_str()),
        Some("standard"),
        "A must stay weak/standard; got: {text}"
    );
    assert_eq!(
        variant_b.get("quality_profile").and_then(|v| v.as_str()),
        Some("flagship"),
        "B must stay strong/flagship; got: {text}"
    );
    assert_eq!(
        variant_b.get("novelty_policy").and_then(|v| v.as_str()),
        Some("strict"),
        "strong scout must use strict novelty policy; got: {text}"
    );
}

#[test]
fn jobs_macro_dispatch_scout_sets_flagship_model_and_contract() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_scout_sets_flagship_model_and_contract",
        &["--agent-id", "manager"],
    );
    let objective = "Собери точный контекст по README и constraints.";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, &workspace, objective);

    let dispatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.macro.dispatch.scout",
                "args": {
                    "task": plan_id,
                    "anchor": "a:scout-contract",
                    "slice_id": slice_id,
                    "objective": objective
                }
            }
        }
    }));
    let dispatch_text = extract_tool_text(&dispatch);
    assert_eq!(
        dispatch_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.macro.dispatch.scout must succeed; got: {dispatch_text}"
    );
    let job_id = dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string();

    let open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "open",
                "args": { "job": job_id, "include_meta": true, "include_prompt": true, "include_events": false }
            }
        }
    }));
    let open_text = extract_tool_text(&open);
    assert_eq!(
        open_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.open for scout job should succeed; got: {open_text}"
    );
    assert_eq!(
        open_text
            .get("result")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("role"))
            .and_then(|v| v.as_str()),
        Some("scout"),
        "scout dispatch should set meta.role=scout; got: {open_text}"
    );
    assert_eq!(
        open_text
            .get("result")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("executor"))
            .and_then(|v| v.as_str()),
        Some("claude_code"),
        "scout dispatch should default to executor=claude_code; got: {open_text}"
    );
    assert_eq!(
        open_text
            .get("result")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("executor_model"))
            .and_then(|v| v.as_str()),
        Some("haiku"),
        "scout dispatch should default executor_model=haiku; got: {open_text}"
    );
    assert!(
        open_text
            .get("result")
            .and_then(|v| v.get("prompt"))
            .and_then(|v| v.as_str())
            .is_some_and(|p| {
                p.contains("ROLE=SCOUT")
                    && p.contains("MUST output ONLY scout_context_pack JSON")
                    && p.contains("MUST NOT output code, patch, diff")
                    && p.contains("MUST keep context extraction bounded: max 12 repository reads")
                    && p.contains("MUST deduplicate aggressively: no repeated file+intent pairs")
                    && p.contains(
                        "Execution target: executor=claude_code model=haiku profile=deep.",
                    )
            }),
        "scout prompt should include strict ROLE=SCOUT contract; got: {open_text}"
    );
}

#[test]
fn jobs_macro_dispatch_scout_rejects_claude_xhigh_profile() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_scout_rejects_claude_xhigh_profile",
        &["--agent-id", "manager"],
    );
    let objective = "profile policy check";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, &workspace, objective);

    let resp = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.scout","args":{
            "task":plan_id,"anchor":"a:scout-profile","slice_id":slice_id,"objective":objective,
            "executor":"claude_code","model":"haiku","executor_profile":"xhigh","dry_run":true
        }}}
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "scout dispatch must fail on claude+xhigh; got: {text}"
    );
    let msg = text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("does not support executor_profile=xhigh"),
        "error should explain scout profile policy; got: {text}"
    );
}

#[test]
fn jobs_macro_dispatch_builder_rejects_non_codex_executor_pin() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_builder_rejects_non_codex_executor_pin",
        &["--agent-id", "manager"],
    );
    let objective = "executor pin check";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, &workspace, objective);

    let resp = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":plan_id,"slice_id":slice_id,"scout_pack_ref":"artifact://jobs/JOB-123456/scout_context_pack",
            "objective":objective,"dod":{"criteria":[],"tests":[],"security":[]},
            "executor":"auto","executor_profile":"xhigh","model":"gpt-5.3-codex","dry_run":true
        }}}
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "builder dispatch must fail on non-codex executor; got: {text}"
    );
    let msg = text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("executor must be codex"),
        "error should explain builder executor pin; got: {text}"
    );
}

#[test]
fn jobs_macro_dispatch_validator_defaults_to_claude_opus_audit() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_validator_defaults_to_claude_opus_audit",
        &["--agent-id", "manager"],
    );
    let (plan_id, slice_id) = setup_plan_and_slice(
        &mut server,
        &workspace,
        "validator dispatch defaults (dry_run)",
    );

    let resp = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.validator","args":{
            "task":plan_id.clone(),"slice_id":slice_id,
            "scout_pack_ref":"artifact://jobs/JOB-111111/scout_context_pack",
            "builder_batch_ref":"artifact://jobs/JOB-222222/builder_diff_batch",
            "plan_ref":plan_id,"dry_run":true
        }}}
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "validator dispatch dry_run should succeed; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("routing"))
            .and_then(|v| v.get("executor"))
            .and_then(|v| v.as_str()),
        Some("claude_code"),
        "validator routing should pin executor=claude_code; got: {text}"
    );
    assert_eq!(
        text.get("result")
            .and_then(|v| v.get("routing"))
            .and_then(|v| v.get("executor_profile"))
            .and_then(|v| v.as_str()),
        Some("audit"),
        "validator routing should pin executor_profile=audit; got: {text}"
    );
    assert!(
        text.get("result")
            .and_then(|v| v.get("routing"))
            .and_then(|v| v.get("executor_model"))
            .and_then(|v| v.as_str())
            .is_some_and(|m| m.to_ascii_lowercase().contains("opus") && m.contains("4.6")),
        "validator routing should pin opus-4.6 family model; got: {text}"
    );
}

#[test]
fn jobs_macro_dispatch_validator_rejects_non_claude_executor_pin() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_validator_rejects_non_claude_executor_pin",
        &["--agent-id", "manager"],
    );
    let (plan_id, slice_id) =
        setup_plan_and_slice(&mut server, &workspace, "validator executor pin check");

    let resp = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.validator","args":{
            "task":plan_id.clone(),"slice_id":slice_id,
            "scout_pack_ref":"artifact://jobs/JOB-111111/scout_context_pack",
            "builder_batch_ref":"artifact://jobs/JOB-222222/builder_diff_batch",
            "plan_ref":plan_id,
            "executor":"codex","executor_profile":"audit","model":"opus-4.6","dry_run":true
        }}}
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "validator dispatch must fail on non-claude executor; got: {text}"
    );
    let msg = text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("executor must be claude_code"),
        "error should explain validator executor pin; got: {text}"
    );
}

#[test]
fn jobs_complete_rejects_duplicate_change_hints_in_strict_scout_pack() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_complete_rejects_duplicate_change_hints_in_strict_scout_pack",
        &["--agent-id", "manager"],
    );
    let objective = "Собери контекст без дублей для builder.";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, &workspace, objective);

    let dispatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.macro.dispatch.scout",
                "args": {
                    "task": plan_id.clone(),
                    "anchor": "a:scout-dup-check",
                    "slice_id": slice_id.clone(),
                    "objective": objective
                }
            }
        }
    }));
    let dispatch_text = extract_tool_text(&dispatch);
    let scout_job = dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":root.to_string_lossy(),"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test"}}}
    }));
    let claim_text = extract_tool_text(&claim);
    let scout_claim_rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout claim revision");

    let readme = std::fs::read(root.join("README.md")).expect("read README");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L1@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L2-L2@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L3-L3@sha256:{sha}");
    let summary_text = "Контекст должен быть коротким, точным, покрывать цель и DoD, \
включать проверяемые риски и не плодить шум. Builder должен получить только нужные \
точки входа, тесты и ограничения без дубликатов, чтобы старшая модель сразу дала \
малый, безопасный и однозначный diff с прозрачным rollback и доказательствами.";
    let scout_pack = json!({
        "objective":"Контекст для дедуп-проверки",
        "scope":{"in":["README.md","docs/contracts/V1_COMMANDS.md"],"out":["crates/storage/*"]},
        "anchors":[
            {"id":"a:readme", "rationale":"Главная точка входа"},
            {"id":"a:contracts", "rationale":"Командный контракт jobs.*"},
            {"id":"a:ux", "rationale":"UX-инварианты для лид-гейта"}
        ],
        "code_refs":[code_ref_a,code_ref_b,code_ref_c],
        "change_hints":[
            {"path":"README.md","intent":"tighten wording","risk":"low"},
            {"path":"README.md","intent":"tighten wording","risk":"low"}
        ],
        "test_hints":[
            "cargo test -p bm_mcp --test jobs_ai_first_ux",
            "cargo test -p bm_runner",
            "cargo test -p bm_mcp --test jobs_ai_first_ux -- --nocapture"
        ],
        "risk_map":[
            {"risk":"docs drift","falsifier":"contract assertions stay green"},
            {"risk":"operator confusion","falsifier":"manual jobs.exec.summary review"},
            {"risk":"hidden regressions","falsifier":"targeted suite for jobs_ai_first_ux"}
        ],
        "open_questions":[],
        "summary_for_builder": summary_text,
        "coverage_matrix":{
            "objective_items":["dedupe","high-signal context"],
            "dod_items":["minimal diff","clear rollback","proof refs"],
            "tests_map":["jobs_ai_first_ux","bm_runner"],
            "risks_map":["docs drift","operator confusion","hidden regressions"],
            "unknowns":[]
        },
        "novelty_index":{
            "anchor_uniqueness":1.0,
            "ref_redundancy":0.0,
            "duplicate_groups":[]
        },
        "critic_findings":[
            {
                "issue":"duplicate path+intent can inflate context noise",
                "severity":"medium",
                "fix_hint":"enforce unique change_hints path+intent pairs",
                "falsifier":"builder dispatch rejects duplicate scout pack"
            }
        ],
        "builder_ready_checklist":{"passed":true,"missing":[]},
        "validator_ready_checklist":{"passed":true,"missing":[]}
    });
    let complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_claim_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack", scout_job),"CMD: scout done"]
        }}}
    }));
    let complete_text = extract_tool_text(&complete);
    assert_eq!(
        complete_text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "jobs.complete must fail-closed on duplicate scout_context_pack; got: {complete_text}"
    );
    let msg = complete_text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("change_hints") && msg.contains("duplicated"),
        "error should explain dedupe violation; got: {complete_text}"
    );
}

#[test]
fn jobs_complete_rejects_unbound_change_hint_paths_in_scout_pack() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_complete_rejects_unbound_change_hint_paths_in_scout_pack",
        &["--agent-id", "manager"],
    );
    let objective = "Собери контекст с явной трассируемостью path→CODE_REF.";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, &workspace, objective);

    let dispatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.macro.dispatch.scout",
                "args": {
                    "task": plan_id.clone(),
                    "anchor": "a:scout-path-binding",
                    "slice_id": slice_id.clone(),
                    "objective": objective,
                    "quality_profile": "standard",
                    "novelty_policy": "warn"
                }
            }
        }
    }));
    let dispatch_text = extract_tool_text(&dispatch);
    let scout_job = dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":root.to_string_lossy(),"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test"}}}
    }));
    let claim_text = extract_tool_text(&claim);
    let scout_claim_rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout claim revision");

    let readme = std::fs::read(root.join("README.md")).expect("read README");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L1@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L2-L2@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L3-L3@sha256:{sha}");
    let summary_text = "Контекст должен быть кратким, трассируемым и пригодным для fail-closed builder/validator пайплайна. \
Каждый change_hint должен быть привязан к реальному CODE_REF пути без выдуманных файлов. \
Это защищает от ложных правок и снижает риск дрейфа контекста.".repeat(2);
    let scout_pack = json!({
        "objective":"Контекст для path-binding gate",
        "scope":{"in":["README.md"],"out":["crates/storage/*"]},
        "anchors":[
            {"id":"a:readme", "rationale":"Главный файл"},
            {"id":"a:intro", "rationale":"Секция быстрых команд"},
            {"id":"a:ux", "rationale":"UX-последствия"}
        ],
        "code_refs":[code_ref_a,code_ref_b,code_ref_c],
        "change_hints":[
            {"path":"docs/plans/ghost/Slice-9.md","intent":"invented scope","risk":"high"},
            {"path":"docs/plans/ghost/PLAN.md","intent":"invented root","risk":"high"}
        ],
        "test_hints":[
            "cargo test -p bm_mcp --test jobs_ai_first_ux",
            "cargo test -p bm_mcp --test jobs_ai_first_ux -- --nocapture"
        ],
        "risk_map":[
            {"risk":"fake path drift","falsifier":"reject unbound change_hints paths"},
            {"risk":"review noise","falsifier":"only CODE_REF-bound hints survive"}
        ],
        "open_questions":[],
        "summary_for_builder": summary_text
    });
    let complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_claim_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack", scout_job),"CMD: scout path binding"]
        }}}
    }));
    let complete_text = extract_tool_text(&complete);
    assert_eq!(
        complete_text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "jobs.complete must fail-closed on unbound change_hints path: {complete_text}"
    );
    let msg = complete_text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("change_hints[].path") && msg.contains("bound"),
        "error should explain CODE_REF/path binding violation; got: {complete_text}"
    );
}

#[test]
fn jobs_complete_accepts_directory_bound_change_hint_paths_in_scout_pack() {
    let root = repo_root();
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized_with_args(
        "jobs_complete_accepts_directory_bound_change_hint_paths_in_scout_pack",
        &["--agent-id", "manager"],
    );
    let objective = "Собери контекст с directory-level binding для change_hints.";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, &workspace, objective);

    let dispatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.macro.dispatch.scout",
                "args": {
                    "task": plan_id.clone(),
                    "anchor": "a:scout-path-binding-dir",
                    "slice_id": slice_id.clone(),
                    "objective": objective,
                    "quality_profile": "standard",
                    "novelty_policy": "warn"
                }
            }
        }
    }));
    let dispatch_text = extract_tool_text(&dispatch);
    let scout_job = dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":root.to_string_lossy(),"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test"}}}
    }));
    let claim_text = extract_tool_text(&claim);
    let scout_claim_rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout claim revision");

    let commands_doc =
        std::fs::read(root.join("docs/contracts/V1_COMMANDS.md")).expect("read V1_COMMANDS.md");
    let sha = sha256_hex(&commands_doc);
    let code_ref_a = format!("code:docs/contracts/V1_COMMANDS.md#L1-L1@sha256:{sha}");
    let code_ref_b = format!("code:docs/contracts/V1_COMMANDS.md#L2-L2@sha256:{sha}");
    let code_ref_c = format!("code:docs/contracts/V1_COMMANDS.md#L3-L3@sha256:{sha}");
    let summary_text = "Directory-level change hints are allowed only when they are still bound by concrete CODE_REF paths and remain within slice scope. \
This preserves fail-closed guarantees while avoiding brittle false negatives when scouts intentionally scope by folder. \
Builder still receives deterministic references and bounded context.".repeat(2);
    let scout_pack = json!({
        "objective":"Контекст для directory path-binding gate",
        "scope":{"in":["docs/contracts/V1_COMMANDS.md"],"out":["crates/storage/*"]},
        "anchors":[
            {"id":"a:commands-contract", "rationale":"Командный контракт"},
            {"id":"a:jobs-scout", "rationale":"Scout pipeline contract"},
            {"id":"a:jobs-gate", "rationale":"Gate/apply contract"}
        ],
        "code_refs":[code_ref_a,code_ref_b,code_ref_c],
        "change_hints":[
            {"path":"docs/contracts/","intent":"sync scout path-binding note","risk":"medium"},
            {"path":"docs/contracts/V1_COMMANDS.md","intent":"document exact fail-closed behavior","risk":"low"}
        ],
        "test_hints":[
            "cargo test -p bm_mcp --test jobs_ai_first_ux jobs_complete_accepts_directory_bound_change_hint_paths_in_scout_pack",
            "cargo test -p bm_mcp --test jobs_ai_first_ux jobs_complete_rejects_unbound_change_hint_paths_in_scout_pack"
        ],
        "risk_map":[
            {"risk":"directory drift","falsifier":"directory path must be covered by concrete CODE_REF descendants"},
            {"risk":"false negatives","falsifier":"accept folder-level hints only when bound"}
        ],
        "open_questions":[],
        "summary_for_builder": summary_text
    });
    let complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_claim_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack", scout_job),"CMD: scout path binding directory"]
        }}}
    }));
    let complete_text = extract_tool_text(&complete);
    assert_eq!(
        complete_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.complete must accept directory-bound change_hints path: {complete_text}"
    );
}

#[test]
fn jobs_complete_rejects_empty_checks_to_run_in_builder_diff_batch() {
    let workspace = "ws_builder_empty_checks";
    let mut server = Server::start_initialized_with_args(
        "jobs_complete_rejects_empty_checks_to_run_in_builder_diff_batch",
        &["--workspace", workspace],
    );

    let created = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"create","args":{
            "title":"builder contract check",
            "prompt":"return builder diff batch",
            "expected_artifacts":["builder_diff_batch"]
        }}}
    }));
    let created_text = extract_tool_text(&created);
    assert_eq!(
        created_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.create should succeed; got: {created_text}"
    );
    let job_id = created_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{
            "job":job_id.clone(),
            "runner_id":"runner-test",
            "lease_ttl_ms":60000
        }}}
    }));
    let claim_text = extract_tool_text(&claim);
    let claim_rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("claim revision");

    let builder_batch = json!({
        "slice_id":"SLC-EMPTY-CHECKS",
        "changes":[
            {"path":"README.md","intent":"tiny wording sync","diff_ref":"artifact://jobs/JOB-000000/diff/README","estimated_risk":"low"}
        ],
        "checks_to_run":[],
        "rollback_plan":"git checkout -- README.md",
        "proof_refs":["CMD: cargo test -q"],
        "execution_evidence":{
            "revision":claim_rev + 1,
            "diff_scope":["README.md"],
            "command_runs":[
                {
                    "cmd":"cargo test -q",
                    "exit_code":0,
                    "stdout_ref":"FILE: artifact://ci/stdout/empty-checks",
                    "stderr_ref":"FILE: artifact://ci/stderr/empty-checks"
                }
            ],
            "rollback_proof":{
                "strategy":"git_revert_single_commit",
                "target_revision":claim_rev,
                "verification_cmd_ref":"CMD: git status --porcelain"
            },
            "semantic_guards":{
                "must_should_may_delta":"none",
                "contract_term_consistency":"verified"
            }
        }
    });
    let complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":job_id.clone(),
            "runner_id":"runner-test",
            "claim_revision":claim_rev,
            "status":"DONE",
            "summary":serde_json::to_string(&builder_batch).expect("builder summary"),
            "refs":[format!("artifact://jobs/{}/builder_diff_batch",job_id),"CMD: cargo test -q"]
        }}}
    }));
    let complete_text = extract_tool_text(&complete);
    assert_eq!(
        complete_text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "jobs.complete must fail when builder_diff_batch.checks_to_run is empty; got: {complete_text}"
    );
    let msg = complete_text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("checks_to_run") && msg.contains("non-empty"),
        "error should explain empty checks_to_run contract failure; got: {complete_text}"
    );
}

#[test]
fn jobs_pipeline_gate_and_apply_validate_contracts_and_gate() {
    let workspace = "ws_pipeline_gate_apply";
    let mut server = Server::start_initialized_with_args(
        "jobs_pipeline_gate_and_apply_validate_contracts_and_gate",
        &["--agent-id", "manager"],
    );
    let objective = "Scout context for README slice.";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, workspace, objective);

    let root = repo_root();
    let readme = std::fs::read(root.join("README.md")).expect("read README.md");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L3@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L4-L8@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L9-L14@sha256:{sha}");

    let scout_dispatch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.macro.dispatch.scout",
                "args": {
                    "task": plan_id.clone(),
                    "anchor": "a:pipeline-v3",
                    "slice_id": slice_id.clone(),
                    "objective": objective
                }
            }
        }
    }));
    let scout_dispatch_text = extract_tool_text(&scout_dispatch);
    let scout_job = scout_dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job id")
        .to_string();

    let scout_claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let scout_claim_rev = extract_tool_text(&scout_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout claim revision");
    let scout_pack = json!({
        "objective": "Scout context for README slice.",
        "scope": { "in": ["README.md"], "out": ["crates/*"] },
        "anchors": [
            { "id": "a-readme-entry", "rationale": "README entrypoint shapes first-run DX and expectations." },
            { "id": "a-readme-setup", "rationale": "Setup section drives command ergonomics and onboarding speed." },
            { "id": "a-readme-contract", "rationale": "Contract wording must stay aligned with v3 jobs pipeline semantics." }
        ],
        "code_refs": [code_ref_a.clone(), code_ref_b.clone(), code_ref_c.clone()],
        "change_hints": [
            { "path": "README.md", "intent": "tighten pipeline role wording", "risk": "low" },
            { "path": "README.md", "intent": "clarify gate/apply fail-closed semantics", "risk": "low" }
        ],
        "test_hints": [
            "cargo test -q",
            "cargo test -p bm_mcp --test jobs_ai_first_ux",
            "rg -n \"jobs\\.pipeline\" docs/contracts/V1_COMMANDS.md docs/contracts/DELEGATION.md"
        ],
        "risk_map": [
            { "risk": "docs drift", "falsifier": "contract lint + docs diff review" },
            { "risk": "semantic mismatch with control.center actions", "falsifier": "manual mcp smoke for gate/apply flow" },
            { "risk": "duplicate anchor context", "falsifier": "novelty index duplicate_groups remains empty" }
        ],
        "open_questions": ["Need final wording approval?"],
        "summary_for_builder": "Focus the diff on two precise README edits: first, align role flow language with scout->builder->validator->gate/apply lifecycle; second, spell out fail-closed checks so operators do not attempt apply on non-approved decisions. Keep edits minimal, reproducible, and covered by docs/tests smoke references. Preserve command naming parity with V1 contracts and avoid introducing normative policy wording drift.",
        "coverage_matrix": {
            "objective_items": ["role lifecycle wording", "fail-closed clarity"],
            "dod_items": ["single tiny docs change", "tests listed in DoD kept"],
            "tests_map": ["cargo test -q", "cargo test -p bm_mcp --test jobs_ai_first_ux"],
            "risks_map": ["docs drift", "semantic mismatch", "duplicate context"],
            "unknowns": ["final wording approval"]
        },
        "novelty_index": {
            "anchor_uniqueness": 1.0,
            "ref_redundancy": 0.0,
            "duplicate_groups": []
        },
        "critic_findings": [
            {
                "issue": "Potential wording drift toward policy language",
                "severity": "medium",
                "fix_hint": "Keep edits descriptive and contract-aligned",
                "falsifier": "MUST/SHOULD delta remains unchanged"
            }
        ],
        "builder_ready_checklist": { "passed": true, "missing": [] },
        "validator_ready_checklist": { "passed": true, "missing": [] }
    });
    let scout_complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_claim_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack", scout_job),"CMD: scout done"]
        }}}
    }));
    assert_eq!(
        extract_tool_text(&scout_complete)
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "scout complete failed: {}",
        extract_tool_text(&scout_complete)
    );
    let scout_open_after_complete = server.request(json!({
        "jsonrpc":"2.0","id":31,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"open","args":{"job":scout_job,"include_meta":true,"include_events":false}}}
    }));
    let scout_open_text = extract_tool_text(&scout_open_after_complete);
    let scout_summary_text = scout_open_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("summary"))
        .and_then(|v| v.as_str())
        .expect("scout summary after complete");
    let scout_summary_json: Value =
        serde_json::from_str(scout_summary_text).expect("scout summary must remain JSON object");
    assert!(
        scout_summary_json.is_object(),
        "scout summary after complete must be object text; got: {}",
        scout_summary_text
    );

    let builder_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),"scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack", scout_job),
            "objective":"Produce one safe README diff batch.",
            "dod":{"criteria":["single tiny change"],"tests":["cargo test -q"],"security":["no secret leaks"]},
            "executor":"codex","executor_profile":"xhigh"
        }}}
    }));
    let builder_dispatch_text = extract_tool_text(&builder_dispatch);
    assert_eq!(
        builder_dispatch_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "builder dispatch failed: {}",
        builder_dispatch_text
    );
    let builder_job = builder_dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("builder job id")
        .to_string();
    let builder_claim = server.request(json!({
        "jsonrpc":"2.0","id":5,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":builder_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let builder_claim_rev = extract_tool_text(&builder_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("builder claim revision");

    let diff_text = "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,1 +1,1 @@\n-Old\n+New\n";
    let diff_put = server.request(json!({
        "jsonrpc":"2.0","id":55,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.artifact.put","args":{
            "job": builder_job.clone(),
            "artifact_key": "diff/README",
            "content_text": diff_text
        }}}
    }));
    assert_eq!(
        extract_tool_text(&diff_put)
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "jobs.artifact.put must succeed; got: {}",
        extract_tool_text(&diff_put)
    );
    let builder_batch = json!({
        "slice_id":slice_id.clone(),
        "changes":[{"path":"README.md","intent":"copy tweak","diff_ref":format!("artifact://jobs/{}/diff/README", builder_job),"estimated_risk":"low"}],
        "checks_to_run":["cargo test -q"],
        "rollback_plan":"git checkout -- README.md",
        "proof_refs":["CMD: cargo test -q"],
        "execution_evidence": {
            "revision": builder_claim_rev + 1,
            "diff_scope": ["README.md"],
            "command_runs": [
                {
                    "cmd": "cargo test -q",
                    "exit_code": 0,
                    "stdout_ref": "FILE: artifact://ci/stdout/builder-1",
                    "stderr_ref": "FILE: artifact://ci/stderr/builder-1"
                }
            ],
            "rollback_proof": {
                "strategy": "git_revert_single_commit",
                "target_revision": builder_claim_rev,
                "verification_cmd_ref": "CMD: git status --porcelain"
            },
            "semantic_guards": {
                "must_should_may_delta": "none",
                "contract_term_consistency": "verified"
            }
        }
    });
    let builder_complete = server.request(json!({
        "jsonrpc":"2.0","id":6,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":builder_job,"runner_id":"runner-test","claim_revision":builder_claim_rev,"status":"DONE",
            "summary":serde_json::to_string(&builder_batch).expect("builder summary"),
            "refs":[format!("artifact://jobs/{}/builder_diff_batch", builder_job),"CMD: cargo test -q"]
        }}}
    }));
    let builder_complete_text = extract_tool_text(&builder_complete);
    assert_eq!(
        builder_complete_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "builder complete failed: {builder_complete_text}"
    );
    let builder_revision = builder_complete_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("builder revision");

    let validator_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.validator","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack", scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch", builder_job),
            "plan_ref":plan_id.clone(),"executor":"claude_code","executor_profile":"audit","model":"opus-4.6"
        }}}
    }));
    let validator_dispatch_text = extract_tool_text(&validator_dispatch);
    let validator_job = validator_dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("validator job id")
        .to_string();
    let validator_claim = server.request(json!({
        "jsonrpc":"2.0","id":8,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":validator_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let validator_claim_rev = extract_tool_text(&validator_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("validator claim revision");
    let validator_report = json!({
        "slice_id":slice_id.clone(),
        "plan_fit_score":96,
        "policy_checks":[{"name":"contracts","pass":true,"reason":"ok"}],
        "tests":[{"name":"cargo test -q","pass":true,"evidence_ref":"CMD: cargo test -q"}],
        "security_findings":[],
        "regression_risk":"low",
        "recommendation":"approve",
        "rework_actions":[]
    });
    let validator_complete = server.request(json!({
        "jsonrpc":"2.0","id":9,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":validator_job,"runner_id":"runner-test","claim_revision":validator_claim_rev,"status":"DONE",
            "summary":serde_json::to_string(&validator_report).expect("validator summary"),
            "refs":[format!("artifact://jobs/{}/validator_report", validator_job),"LINK: https://ci.local/run/1"]
        }}}
    }));
    assert_eq!(
        extract_tool_text(&validator_complete)
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "validator complete failed: {}",
        extract_tool_text(&validator_complete)
    );

    let gate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.pipeline.gate",
                "args": {
                    "task": plan_id.clone(),
                    "slice_id": slice_id.clone(),
                    "scout_pack_ref": format!("artifact://jobs/{}/scout_context_pack", scout_job),
                    "builder_batch_ref": format!("artifact://jobs/{}/builder_diff_batch", builder_job),
                    "validator_report_ref": format!("artifact://jobs/{}/validator_report", validator_job),
                    "policy": "fail_closed"
                }
            }
        }
    }));
    let gate_text = extract_tool_text(&gate);
    assert_eq!(
        gate_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.pipeline.gate should succeed; got: {gate_text}"
    );
    assert_eq!(
        gate_text
            .get("result")
            .and_then(|v| v.get("decision"))
            .and_then(|v| v.as_str()),
        Some("approve"),
        "pipeline.gate should produce approve; got: {gate_text}"
    );
    let decision_ref = gate_text
        .get("result")
        .and_then(|v| v.get("decision_ref"))
        .and_then(|v| v.as_str())
        .expect("decision_ref")
        .to_string();

    let apply = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.pipeline.apply",
                "args": {
                    "task": plan_id,
                    "slice_id": slice_id,
                    "decision_ref": decision_ref,
                    "builder_batch_ref": format!("artifact://jobs/{}/builder_diff_batch", builder_job),
                    "expected_revision": builder_revision
                }
            }
        }
    }));
    let apply_text = extract_tool_text(&apply);
    assert_eq!(
        apply_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "jobs.pipeline.apply should succeed on pass verdict; got: {apply_text}"
    );
    assert_eq!(
        apply_text
            .get("result")
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str()),
        Some("applied"),
        "pipeline.apply should return status=applied; got: {apply_text}"
    );
}

#[test]
fn jobs_pipeline_apply_blocks_on_failed_validator_report() {
    let workspace = "ws_pipeline_apply_reject";
    let mut server = Server::start_initialized_with_args(
        "jobs_pipeline_apply_blocks_on_failed_validator_report",
        &["--agent-id", "manager"],
    );
    let objective = "reject path";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, workspace, objective);
    let root = repo_root();
    let readme = std::fs::read(root.join("README.md")).expect("read README.md");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L3@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L4-L8@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L9-L14@sha256:{sha}");

    let scout_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.scout","args":{
            "task":plan_id.clone(),"anchor":"a:pipeline-v3","slice_id":slice_id.clone(),"objective":objective
        }}}
    }));
    let scout_job = extract_tool_text(&scout_dispatch)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job")
        .to_string();
    let scout_claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let scout_rev = extract_tool_text(&scout_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout rev");
    let scout_pack = json!({
        "objective":"reject path",
        "scope":{"in":["README.md"],"out":[]},
        "anchors":[
            {"id":"a1","rationale":"README entrypoint contains user-facing pipeline framing."},
            {"id":"a2","rationale":"Reject-path wording should still preserve operator clarity."},
            {"id":"a3","rationale":"Artifact contracts in docs must match runtime fail-closed behavior."}
        ],
        "code_refs":[code_ref_a.clone(),code_ref_b.clone(),code_ref_c.clone()],
        "change_hints":[
            {"path":"README.md","intent":"adjust reject-path wording","risk":"low"},
            {"path":"README.md","intent":"preserve gate/apply constraints","risk":"low"}
        ],
        "test_hints":[
            "cargo test -q",
            "cargo test -p bm_mcp --test jobs_ai_first_ux",
            "rg -n \"jobs\\.pipeline\" docs/contracts/V1_COMMANDS.md docs/contracts/DELEGATION.md"
        ],
        "risk_map":[
            {"risk":"reject flow wording drift","falsifier":"contract assertion in gate tests"},
            {"risk":"operator confusion","falsifier":"manual control.center action review"},
            {"risk":"duplicate context","falsifier":"novelty duplicate_groups empty"}
        ],
        "open_questions":[],
        "summary_for_builder":"Builder should produce a reject-path-safe diff batch that keeps pipeline behavior deterministic: preserve validator recommendation precedence, keep gate reasons explicit, and avoid broad refactors. The change should be tiny, auditable, and include clear rollback notes so lead gate can quickly rework or reject without noisy churn. Maintain contract naming parity and evidence-first language.",
        "coverage_matrix": {
            "objective_items": ["reject-path stability", "docs-only wording"],
            "dod_items": ["tiny diff", "tests evidence retained"],
            "tests_map": ["cargo test -q", "cargo test -p bm_mcp --test jobs_ai_first_ux"],
            "risks_map": ["wording drift", "operator confusion", "duplicate context"],
            "unknowns": []
        },
        "novelty_index": {
            "anchor_uniqueness": 1.0,
            "ref_redundancy": 0.0,
            "duplicate_groups": []
        },
        "critic_findings": [
            {
                "issue": "Rollback evidence can be under-specified",
                "severity": "medium",
                "fix_hint": "Require explicit execution_evidence rollback proof in builder output",
                "falsifier": "validator report includes rollback-evidence check"
            }
        ],
        "builder_ready_checklist": { "passed": true, "missing": [] },
        "validator_ready_checklist": { "passed": true, "missing": [] }
    });
    let scout_complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack",scout_job),"CMD: scout"]
        }}}
    }));
    assert_eq!(
        extract_tool_text(&scout_complete)
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "scout complete failed: {}",
        extract_tool_text(&scout_complete)
    );
    let scout_open_after_complete = server.request(json!({
        "jsonrpc":"2.0","id":32,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"open","args":{"job":scout_job,"include_meta":true,"include_events":false}}}
    }));
    let scout_open_text = extract_tool_text(&scout_open_after_complete);
    let scout_summary_text = scout_open_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("summary"))
        .and_then(|v| v.as_str())
        .expect("scout summary after complete");
    let scout_summary_json: Value =
        serde_json::from_str(scout_summary_text).expect("scout summary must remain JSON object");
    assert!(
        scout_summary_json.is_object(),
        "scout summary after complete must be object text; got: {}",
        scout_summary_text
    );
    let builder_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "objective":"build reject path","dod":{"criteria":["x"],"tests":["cargo test -q"],"security":["none"]}
        }}}
    }));
    let builder_dispatch_text = extract_tool_text(&builder_dispatch);
    assert_eq!(
        builder_dispatch_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "builder dispatch failed: {}",
        builder_dispatch_text
    );
    let builder_job = builder_dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("builder job")
        .to_string();
    let builder_claim = server.request(json!({
        "jsonrpc":"2.0","id":5,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":builder_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let builder_rev = extract_tool_text(&builder_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("builder rev");
    let diff_text = "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,1 +1,1 @@\n-Old\n+New\n";
    let diff_put = server.request(json!({
        "jsonrpc":"2.0","id":56,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.artifact.put","args":{
            "job": builder_job.clone(),
            "artifact_key": "diff/README",
            "content_text": diff_text
        }}}
    }));
    assert_eq!(
        extract_tool_text(&diff_put)
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "jobs.artifact.put must succeed; got: {}",
        extract_tool_text(&diff_put)
    );
    let builder_batch = json!({
        "slice_id":slice_id.clone(),
        "changes":[{"path":"README.md","intent":"tiny","diff_ref":format!("artifact://jobs/{}/diff/README",builder_job),"estimated_risk":"low"}],
        "checks_to_run":["cargo test -q"],
        "rollback_plan":"git checkout -- README.md",
        "proof_refs":["CMD: cargo test -q"],
        "execution_evidence": {
            "revision": builder_rev + 1,
            "diff_scope": ["README.md"],
            "command_runs": [
                {
                    "cmd": "cargo test -q",
                    "exit_code": 0,
                    "stdout_ref": "FILE: artifact://ci/stdout/builder-2",
                    "stderr_ref": "FILE: artifact://ci/stderr/builder-2"
                }
            ],
            "rollback_proof": {
                "strategy": "git_revert_single_commit",
                "target_revision": builder_rev,
                "verification_cmd_ref": "CMD: git status --porcelain"
            },
            "semantic_guards": {
                "must_should_may_delta": "none",
                "contract_term_consistency": "verified"
            }
        }
    });
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":6,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":builder_job,"runner_id":"runner-test","claim_revision":builder_rev,"status":"DONE",
            "summary":serde_json::to_string(&builder_batch).expect("builder summary"),
            "refs":[format!("artifact://jobs/{}/builder_diff_batch",builder_job),"CMD: builder"]
        }}}
    }));
    let validator_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.validator","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch",builder_job),
            "plan_ref":plan_id.clone()
        }}}
    }));
    let validator_job = extract_tool_text(&validator_dispatch)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("validator job")
        .to_string();
    let validator_claim = server.request(json!({
        "jsonrpc":"2.0","id":8,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":validator_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let validator_rev = extract_tool_text(&validator_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("validator rev");
    let reject_report = json!({
        "slice_id":slice_id.clone(),
        "plan_fit_score":41,
        "policy_checks":[{"name":"contracts","pass":false,"reason":"missing proof"}],
        "tests":[{"name":"cargo test -q","pass":false,"evidence_ref":"CMD: cargo test -q"}],
        "security_findings":[{"severity":"high","issue":"policy breach"}],
        "regression_risk":"high",
        "recommendation":"reject",
        "rework_actions":["fix proof chain"]
    });
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":9,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":validator_job,"runner_id":"runner-test","claim_revision":validator_rev,"status":"DONE",
            "summary":serde_json::to_string(&reject_report).expect("validator summary"),
            "refs":[format!("artifact://jobs/{}/validator_report",validator_job),"LINK: https://ci.local/reject"]
        }}}
    }));
    let gate = server.request(json!({
        "jsonrpc":"2.0","id":10,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.pipeline.gate","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch",builder_job),
            "validator_report_ref":format!("artifact://jobs/{}/validator_report",validator_job),
            "policy":"fail_closed"
        }}}
    }));
    let gate_text = extract_tool_text(&gate);
    assert_eq!(
        gate_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "pipeline.gate reject path should still succeed; got: {gate_text}"
    );
    let decision_ref = gate_text
        .get("result")
        .and_then(|v| v.get("decision_ref"))
        .and_then(|v| v.as_str())
        .expect("decision_ref")
        .to_string();

    let apply = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "call",
                "cmd": "jobs.pipeline.apply",
                "args": {
                    "task": plan_id,
                    "slice_id": slice_id,
                    "decision_ref": decision_ref,
                    "builder_batch_ref": format!("artifact://jobs/{}/builder_diff_batch",builder_job),
                    "expected_revision": builder_rev + 1
                }
            }
        }
    }));
    let apply_text = extract_tool_text(&apply);
    assert_eq!(
        apply_text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "pipeline.apply must fail when decision is reject; got: {apply_text}"
    );
    assert_eq!(
        apply_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("PRECONDITION_FAILED"),
        "pipeline.apply fail verdict should return PRECONDITION_FAILED; got: {apply_text}"
    );
}

#[test]
fn jobs_macro_dispatch_builder_unknown_args_fail_closed() {
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_builder_unknown_args_fail_closed",
        &["--workspace", "ws_builder_unknown_args"],
    );

    let resp = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":"ws_builder_unknown_args","op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":"TASK-UA","slice_id":"SLC-UA","scout_pack_ref":"artifact://jobs/JOB-999999/scout_context_pack",
            "objective":"noop","dod":{"criteria":["c"],"tests":["t"],"security":["s"]},
            "foo":"bar"
        }}}
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "unknown args must fail-closed; got: {text}"
    );
    assert_eq!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT"),
        "unknown args must return INVALID_INPUT; got: {text}"
    );
    let msg = text
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        msg.contains("unknown args: foo"),
        "expected unknown args mention; got message={msg}"
    );
}

#[test]
fn jobs_macro_dispatch_builder_strict_mode_rejects_standard_scout_quality() {
    let workspace = "ws_builder_strict_quality";
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_builder_strict_mode_rejects_standard_scout_quality",
        &["--agent-id", "manager"],
    );
    let objective = "Prepare standard scout pack";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, workspace, objective);
    let root = repo_root();
    let readme = std::fs::read(root.join("README.md")).expect("read README.md");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L3@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L4-L8@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L9-L14@sha256:{sha}");
    let long_summary = "Standard scout summary keeps only baseline details for quick iteration and intentionally skips flagship-only matrices while still preserving deterministic references and bounded context for builders.".repeat(3);

    let scout_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.scout","args":{
            "task":plan_id.clone(),"anchor":"a:strict-quality","slice_id":slice_id.clone(),
            "objective":objective,
            "quality_profile":"standard"
        }}}
    }));
    let scout_job = extract_tool_text(&scout_dispatch)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job")
        .to_string();
    let scout_claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let scout_rev = extract_tool_text(&scout_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout revision");
    let scout_pack = json!({
        "objective":"standard scout",
        "scope":{"in":["README.md"],"out":[]},
        "anchors":[
            {"id":"a:std-1","rationale":"README intro context."},
            {"id":"a:std-2","rationale":"README setup context."},
            {"id":"a:std-3","rationale":"README contracts context."}
        ],
        "code_refs":[code_ref_a.clone(),code_ref_b.clone(),code_ref_c.clone()],
        "change_hints":[
            {"path":"README.md","intent":"tiny edit","risk":"low"},
            {"path":"README.md","intent":"wording sync","risk":"low"}
        ],
        "test_hints":[
            "cargo test -q",
            "cargo test -p bm_mcp --test jobs_ai_first_ux",
            "rg -n \"jobs\\.pipeline\" docs/contracts/V1_COMMANDS.md"
        ],
        "risk_map":[
            {"risk":"wording drift","falsifier":"contracts diff"},
            {"risk":"context loss","falsifier":"open scout summary"},
            {"risk":"retry confusion","falsifier":"gate action review"}
        ],
        "open_questions":[],
        "summary_for_builder": long_summary
    });
    let scout_complete = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack",scout_job),"CMD: scout standard"]
        }}}
    }));
    assert_eq!(
        extract_tool_text(&scout_complete)
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "scout complete failed: {}",
        extract_tool_text(&scout_complete)
    );

    let builder_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":plan_id,"slice_id":slice_id,
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "objective":"strict builder must reject standard scout quality",
            "dod":{"criteria":["c"],"tests":["t"],"security":["s"]},
            "strict_scout_mode":true
        }}}
    }));
    let builder_text = extract_tool_text(&builder_dispatch);
    assert_eq!(
        builder_text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "builder dispatch must fail under strict mode + standard quality; got: {builder_text}"
    );
    assert_eq!(
        builder_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("PRECONDITION_FAILED"),
        "expected PRECONDITION_FAILED; got: {builder_text}"
    );
}

#[test]
fn jobs_pipeline_gate_context_request_loop_is_bounded() {
    let workspace = "ws_pipeline_context_loop";
    let mut server = Server::start_initialized_with_args(
        "jobs_pipeline_gate_context_request_loop_is_bounded",
        &["--agent-id", "manager"],
    );
    let objective = "Context loop test for builder request";
    let (plan_id, slice_id) = setup_plan_and_slice(&mut server, workspace, objective);
    let root = repo_root();
    let readme = std::fs::read(root.join("README.md")).expect("read README.md");
    let sha = sha256_hex(&readme);
    let code_ref_a = format!("code:README.md#L1-L3@sha256:{sha}");
    let code_ref_b = format!("code:README.md#L4-L8@sha256:{sha}");
    let code_ref_c = format!("code:README.md#L9-L14@sha256:{sha}");

    let scout_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.scout","args":{
            "task":plan_id.clone(),"anchor":"a:ctx-loop","slice_id":slice_id.clone(),
            "objective":objective
        }}}
    }));
    let scout_job = extract_tool_text(&scout_dispatch)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("scout job")
        .to_string();
    let scout_claim = server.request(json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":scout_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let scout_rev = extract_tool_text(&scout_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("scout rev");
    let scout_pack = json!({
        "objective":"context loop scout",
        "scope":{"in":["README.md"],"out":[]},
        "anchors":[
            {"id":"a:ctx-1","rationale":"primary context"},
            {"id":"a:ctx-2","rationale":"dependency context"},
            {"id":"a:ctx-3","rationale":"reference context"}
        ],
        "code_refs":[code_ref_a.clone(),code_ref_b.clone(),code_ref_c.clone()],
        "change_hints":[
            {"path":"README.md","intent":"prepare context loop wording","risk":"low"},
            {"path":"README.md","intent":"keep gate behavior deterministic","risk":"low"}
        ],
        "test_hints":[
            "cargo test -q",
            "cargo test -p bm_mcp --test jobs_ai_first_ux",
            "rg -n \"context_request\" crates/mcp/src/handlers/tasks/jobs/pipeline.rs"
        ],
        "risk_map":[
            {"risk":"loop never stops","falsifier":"retry limit enforced at gate"},
            {"risk":"lineage drift","falsifier":"gate lineage checks"},
            {"risk":"action ambiguity","falsifier":"first gate action is dispatch.scout"}
        ],
        "open_questions":[],
        "summary_for_builder":"Builder should request additional context when critical inputs are missing, and gate must bound retries to avoid infinite loops while preserving fail-closed lineage constraints. Keep output deterministic and proof-carrying. Validate that context requests never bypass lineage checks and that retry budgets are explicit in metadata. Ensure rework actions remain low-noise and reproducible so operators can continue the pipeline without ambiguity.",
        "coverage_matrix": {
            "objective_items": ["request context", "bounded retries"],
            "dod_items": ["gate emits scout action", "retry cap enforced"],
            "tests_map": ["cargo test -q"],
            "risks_map": ["loop never stops", "lineage drift"],
            "unknowns": []
        },
        "novelty_index": {"anchor_uniqueness": 1.0, "ref_redundancy": 0.0, "duplicate_groups": []},
        "critic_findings": [{
            "issue": "Potential unbounded retry loop",
            "severity": "high",
            "fix_hint": "Enforce hard retry cap at gate",
            "falsifier": "decision becomes reject after cap"
        }],
        "builder_ready_checklist": {"passed": true, "missing": []},
        "validator_ready_checklist": {"passed": true, "missing": []}
    });
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":scout_job,"runner_id":"runner-test","claim_revision":scout_rev,"status":"DONE",
            "summary":serde_json::to_string(&scout_pack).expect("scout summary"),
            "refs":[format!("artifact://jobs/{}/scout_context_pack",scout_job),"CMD: scout context loop"]
        }}}
    }));

    let builder_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "objective":"context request pass",
            "dod":{"criteria":["ctx"],"tests":["cargo test -q"],"security":["lineage"]},
            "strict_scout_mode":false,
            "allow_prevalidate_non_pass":true,
            "context_retry_count":0,
            "context_retry_limit":2
        }}}
    }));
    let builder_dispatch_text = extract_tool_text(&builder_dispatch);
    assert_eq!(
        builder_dispatch_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "builder dispatch (retry 0/2) failed: {builder_dispatch_text}"
    );
    let builder_job = builder_dispatch_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("builder job")
        .to_string();
    let builder_claim = server.request(json!({
        "jsonrpc":"2.0","id":5,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":builder_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let builder_rev = extract_tool_text(&builder_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("builder revision");
    let builder_batch_need_more = json!({
        "slice_id":slice_id.clone(),
        "changes":[],
        "context_request":{
            "reason":"Need dependency anchor for rollout checklist",
            "missing_context":["dependency anchor for README rollout checklist"],
            "suggested_scout_focus":["README.md rollout section"],
            "suggested_tests":["cargo test -q"]
        },
        "checks_to_run":["cargo test -q -p bm_mcp --test jobs_ai_first_ux jobs_pipeline_gate_context_request_loop_is_bounded"],
        "rollback_plan":"no-op: context request only",
        "proof_refs":["CMD: echo context-request"],
        "execution_evidence": {
            "revision": builder_rev + 1,
            "diff_scope": ["context_request"],
            "command_runs": [{
                "cmd": "echo context-request",
                "exit_code": 0,
                "stdout_ref": "FILE: artifact://ci/stdout/context-request",
                "stderr_ref": "FILE: artifact://ci/stderr/context-request"
            }],
            "rollback_proof": {
                "strategy": "noop",
                "target_revision": builder_rev,
                "verification_cmd_ref": "CMD: git status --porcelain"
            },
            "semantic_guards": {
                "must_should_may_delta": "none",
                "contract_term_consistency": "verified"
            }
        }
    });
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":6,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":builder_job,"runner_id":"runner-test","claim_revision":builder_rev,"status":"DONE",
            "summary":serde_json::to_string(&builder_batch_need_more).expect("builder summary"),
            "refs":[format!("artifact://jobs/{}/builder_diff_batch",builder_job),"CMD: context-request"]
        }}}
    }));

    let validator_dispatch = server.request(json!({
        "jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.validator","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch",builder_job),
            "plan_ref":plan_id.clone()
        }}}
    }));
    let validator_job = extract_tool_text(&validator_dispatch)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("validator job")
        .to_string();
    let validator_claim = server.request(json!({
        "jsonrpc":"2.0","id":8,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":validator_job,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let validator_rev = extract_tool_text(&validator_claim)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("validator rev");
    let validator_report = json!({
        "slice_id":slice_id.clone(),
        "plan_fit_score":72,
        "policy_checks":[{"name":"contracts","pass":true,"reason":"context request allowed"}],
        "tests":[{"name":"cargo test -q","pass":true,"evidence_ref":"CMD: cargo test -q"}],
        "security_findings":[],
        "regression_risk":"medium",
        "recommendation":"rework",
        "rework_actions":["refresh scout context"]
    });
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":9,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":validator_job,"runner_id":"runner-test","claim_revision":validator_rev,"status":"DONE",
            "summary":serde_json::to_string(&validator_report).expect("validator summary"),
            "refs":[format!("artifact://jobs/{}/validator_report",validator_job),"LINK: https://ci.local/context-loop"]
        }}}
    }));
    let gate = server.request(json!({
        "jsonrpc":"2.0","id":10,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.pipeline.gate","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch",builder_job),
            "validator_report_ref":format!("artifact://jobs/{}/validator_report",validator_job),
            "policy":"fail_closed"
        }}}
    }));
    let gate_text = extract_tool_text(&gate);
    assert_eq!(
        gate_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "pipeline.gate context-request path should succeed; got: {gate_text}"
    );
    assert_eq!(
        gate_text
            .get("result")
            .and_then(|v| v.get("decision"))
            .and_then(|v| v.as_str()),
        Some("rework"),
        "gate should force rework on context request within retry budget; got: {gate_text}"
    );
    assert_eq!(
        gate_text
            .get("result")
            .and_then(|v| v.get("context_loop"))
            .and_then(|v| v.get("builder_requested_context"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "context-loop metadata should mark builder context request; got: {gate_text}"
    );
    assert_eq!(
        gate_text
            .get("result")
            .and_then(|v| v.get("context_loop"))
            .and_then(|v| v.get("context_retry_count"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "gate should preserve initial retry count before replay action; got: {gate_text}"
    );
    assert_eq!(
        gate_text
            .get("result")
            .and_then(|v| v.get("context_loop"))
            .and_then(|v| v.get("context_retry_limit"))
            .and_then(|v| v.as_u64()),
        Some(2),
        "gate should expose retry limit in context_loop metadata; got: {gate_text}"
    );
    assert_eq!(
        gate_text
            .get("result")
            .and_then(|v| v.get("actions"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str()),
        Some("jobs.macro.dispatch.scout"),
        "first action should refresh scout context; got: {gate_text}"
    );

    // Exhausted retry budget: context_request must fail closed to reject.
    let builder_dispatch_2 = server.request(json!({
        "jsonrpc":"2.0","id":11,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.builder","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "objective":"context request exhausted",
            "dod":{"criteria":["ctx"],"tests":["cargo test -q"],"security":["lineage"]},
            "strict_scout_mode":false,
            "allow_prevalidate_non_pass":true,
            "context_retry_count":2,
            "context_retry_limit":2
        }}}
    }));
    let builder_dispatch_2_text = extract_tool_text(&builder_dispatch_2);
    assert_eq!(
        builder_dispatch_2_text
            .get("success")
            .and_then(|v| v.as_bool()),
        Some(true),
        "builder dispatch (retry 2/2) failed: {builder_dispatch_2_text}"
    );
    let builder_job_2 = builder_dispatch_2_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("builder job 2")
        .to_string();
    let builder_claim_2 = server.request(json!({
        "jsonrpc":"2.0","id":12,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":builder_job_2,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let builder_rev_2 = extract_tool_text(&builder_claim_2)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("builder revision 2");
    let mut builder_batch_exhausted = builder_batch_need_more.clone();
    builder_batch_exhausted["execution_evidence"]["revision"] = json!(builder_rev_2 + 1);
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":13,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":builder_job_2,"runner_id":"runner-test","claim_revision":builder_rev_2,"status":"DONE",
            "summary":serde_json::to_string(&builder_batch_exhausted).expect("builder summary exhausted"),
            "refs":[format!("artifact://jobs/{}/builder_diff_batch",builder_job_2),"CMD: context-request-exhausted"]
        }}}
    }));
    let validator_dispatch_2 = server.request(json!({
        "jsonrpc":"2.0","id":14,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.macro.dispatch.validator","args":{
            "task":plan_id.clone(),"slice_id":slice_id.clone(),
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch",builder_job_2),
            "plan_ref":plan_id.clone()
        }}}
    }));
    let validator_job_2 = extract_tool_text(&validator_dispatch_2)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("validator job 2")
        .to_string();
    let validator_claim_2 = server.request(json!({
        "jsonrpc":"2.0","id":15,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.claim","args":{"job":validator_job_2,"runner_id":"runner-test","lease_ttl_ms":60000}}}
    }));
    let validator_rev_2 = extract_tool_text(&validator_claim_2)
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("validator rev 2");
    let _ = server.request(json!({
        "jsonrpc":"2.0","id":16,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.complete","args":{
            "job":validator_job_2,"runner_id":"runner-test","claim_revision":validator_rev_2,"status":"DONE",
            "summary":serde_json::to_string(&validator_report).expect("validator summary 2"),
            "refs":[format!("artifact://jobs/{}/validator_report",validator_job_2),"LINK: https://ci.local/context-loop-2"]
        }}}
    }));
    let gate_exhausted = server.request(json!({
        "jsonrpc":"2.0","id":17,"method":"tools/call",
        "params":{"name":"jobs","arguments":{"workspace":workspace,"op":"call","cmd":"jobs.pipeline.gate","args":{
            "task":plan_id,"slice_id":slice_id,
            "scout_pack_ref":format!("artifact://jobs/{}/scout_context_pack",scout_job),
            "builder_batch_ref":format!("artifact://jobs/{}/builder_diff_batch",builder_job_2),
            "validator_report_ref":format!("artifact://jobs/{}/validator_report",validator_job_2),
            "policy":"fail_closed"
        }}}
    }));
    let gate_exhausted_text = extract_tool_text(&gate_exhausted);
    assert_eq!(
        gate_exhausted_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "pipeline.gate exhausted path should still return success with reject decision; got: {gate_exhausted_text}"
    );
    assert_eq!(
        gate_exhausted_text
            .get("result")
            .and_then(|v| v.get("decision"))
            .and_then(|v| v.as_str()),
        Some("reject"),
        "gate must reject when context retry budget exhausted; got: {gate_exhausted_text}"
    );
    assert_eq!(
        gate_exhausted_text
            .get("result")
            .and_then(|v| v.get("context_loop"))
            .and_then(|v| v.get("builder_requested_context"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "context-loop metadata should persist builder request on exhausted path; got: {gate_exhausted_text}"
    );
    assert_eq!(
        gate_exhausted_text
            .get("result")
            .and_then(|v| v.get("actions"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("cmd"))
            .and_then(|v| v.as_str()),
        Some("jobs.macro.dispatch.builder"),
        "exhausted retry path should avoid auto-scout and route to builder rework; got: {gate_exhausted_text}"
    );
    let exhausted_reason = gate_exhausted_text
        .get("result")
        .and_then(|v| v.get("reasons"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find_map(|entry| entry.as_str()))
        .unwrap_or_default();
    assert!(
        exhausted_reason.contains("validator recommendation="),
        "gate should always include validator recommendation in reasons; got: {gate_exhausted_text}"
    );
    let has_retry_exhausted_reason = gate_exhausted_text
        .get("result")
        .and_then(|v| v.get("reasons"))
        .and_then(|v| v.as_array())
        .is_some_and(|arr| {
            arr.iter().any(|entry| {
                entry
                    .as_str()
                    .is_some_and(|reason| reason.contains("retry budget exhausted"))
            })
        });
    assert!(
        has_retry_exhausted_reason,
        "gate reasons should explicitly mention retry budget exhaustion; got: {gate_exhausted_text}"
    );
}

#[test]
fn jobs_cancel_schema_exposes_force_running_and_expected_revision() {
    let mut server = Server::start_initialized_with_args(
        "jobs_cancel_schema_exposes_force_running_and_expected_revision",
        &["--workspace", "ws_jobs_cancel_schema"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "system",
            "arguments": {
                "op": "schema.get",
                "args": { "cmd": "jobs.cancel" }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(text.get("success").and_then(|v| v.as_bool()), Some(true));
    let props = text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_object())
        .expect("jobs.cancel args_schema.properties");
    assert!(
        props.contains_key("force_running"),
        "missing force_running: {props:?}"
    );
    assert!(
        props.contains_key("expected_revision"),
        "missing expected_revision: {props:?}"
    );
    assert!(
        !props.contains_key("mode"),
        "jobs.cancel schema must not leak jobs.wait-only mode field: {props:?}"
    );
}

#[test]
fn jobs_macro_dispatch_builder_schema_exposes_strict_context_retry_args() {
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_dispatch_builder_schema_exposes_strict_context_retry_args",
        &["--workspace", "ws_jobs_builder_schema_strict"],
    );

    let resp = server.request(json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"system","arguments":{
            "op":"schema.get",
            "args":{"cmd":"jobs.macro.dispatch.builder"}
        }}
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "schema.get must succeed; got: {text}"
    );
    let props = text
        .get("result")
        .and_then(|v| v.get("args_schema"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_object())
        .expect("jobs.macro.dispatch.builder args_schema.properties");
    for key in [
        "strict_scout_mode",
        "scout_stale_after_s",
        "context_retry_count",
        "context_retry_limit",
    ] {
        assert!(
            props.contains_key(key),
            "builder schema must expose {key}; got keys={:?}",
            props.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn jobs_pipeline_pre_validate_accepts_task_and_slice_hints_without_unknown_arg_error() {
    let mut server = Server::start_initialized_with_args(
        "jobs_pipeline_pre_validate_accepts_task_and_slice_hints_without_unknown_arg_error",
        &["--workspace", "ws_jobs_prevalidate_hints"],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_prevalidate_hints",
                "op": "call",
                "cmd": "jobs.pipeline.pre.validate",
                "args": {
                    "task": "TASK-HINT",
                    "slice_id": "SLC-HINT",
                    "scout_pack_ref": "artifact://jobs/JOB-999999/scout_context_pack"
                }
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(false),
        "pre_validate with unknown scout ref should fail cleanly; got: {text}"
    );
    assert_ne!(
        text.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT"),
        "task/slice hints must not trigger unknown-args INVALID_INPUT; got: {text}"
    );
}

#[test]
fn jobs_complete_proof_gate_recovery_actions_stay_in_jobs_portal() {
    let mut server = Server::start_initialized_with_args(
        "jobs_complete_proof_gate_recovery_actions_stay_in_jobs_portal",
        &["--workspace", "ws_jobs_proof_recovery"],
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_proof_recovery",
                "op": "create",
                "args": {
                    "title": "proof gate recovery",
                    "prompt": "noop",
                    "priority": "HIGH"
                }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    let job_id = created_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_proof_recovery",
                "op": "call",
                "cmd": "jobs.claim",
                "args": { "job": job_id, "runner_id": "runner:proof" }
            }
        }
    }));
    let claim_text = extract_tool_text(&claim);
    let rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("result.job.revision");

    let complete = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_proof_recovery",
                "op": "call",
                "cmd": "jobs.complete",
                "args": {
                    "job": created_text.get("result").and_then(|v| v.get("job")).and_then(|v| v.get("job_id")).and_then(|v| v.as_str()).expect("job_id"),
                    "runner_id": "runner:proof",
                    "claim_revision": rev,
                    "status": "DONE",
                    "summary": "without proof"
                }
            }
        }
    }));
    let complete_text = extract_tool_text(&complete);
    assert_eq!(
        complete_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("PRECONDITION_FAILED"),
        "expected HIGH DONE proof gate; got: {complete_text}"
    );
    let actions = complete_text
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        actions.iter().any(|a| {
            a.get("tool").and_then(|v| v.as_str()) == Some("jobs")
                && a.get("args")
                    .and_then(|v| v.get("cmd"))
                    .and_then(|v| v.as_str())
                    == Some("jobs.report")
        }),
        "proof gate recovery actions should stay in jobs portal; got actions={actions:?}"
    );
    assert!(
        actions.iter().all(|a| {
            !a.get("action_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .contains("tasks_jobs_")
        }),
        "proof gate recovery action_id must not leak legacy tasks_jobs_* names: {actions:?}"
    );
}

#[test]
fn tasks_jobs_call_accepts_injected_agent_id_without_unknown_arg_failure() {
    let mut server = Server::start_initialized_with_args(
        "tasks_jobs_call_accepts_injected_agent_id_without_unknown_arg_failure",
        &[
            "--workspace",
            "ws_jobs_tasks_bridge",
            "--agent-id",
            "agent-default",
        ],
    );

    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "tasks",
            "arguments": {
                "workspace": "ws_jobs_tasks_bridge",
                "op": "call",
                "cmd": "tasks.jobs.radar",
                "args": {}
            }
        }
    }));
    let text = extract_tool_text(&resp);
    assert_eq!(
        text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "tasks.jobs.radar should tolerate injected agent_id envelope key; got: {text}"
    );
}

#[test]
fn jobs_message_validates_code_ref_contract() {
    let root = std::env::current_dir().expect("cwd");
    let workspace = root.to_string_lossy().to_string();
    let mut server = Server::start_initialized("jobs_message_validates_code_ref_contract");

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": workspace,
                "op": "create",
                "args": { "title": "code ref", "prompt": "noop" }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    let job_id = created_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string();

    let invalid_ref = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": root.to_string_lossy(),
                "op": "message",
                "args": {
                    "job": job_id,
                    "message": "invalid",
                    "refs": ["code:broken"]
                }
            }
        }
    }));
    let invalid_text = extract_tool_text(&invalid_ref);
    assert_eq!(
        invalid_text
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT"),
        "invalid CODE_REF should be rejected; got: {invalid_text}"
    );
}

#[test]
fn jobs_macro_respond_inbox_auto_targets_needs_manager_without_job_args() {
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_respond_inbox_auto_targets_needs_manager_without_job_args",
        &["--workspace", "ws_jobs_macro_auto_manager"],
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_manager",
                "op": "create",
                "args": { "title": "auto inbox", "prompt": "noop" }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    let job_id = created_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_manager",
                "op": "claim",
                "args": { "job": job_id, "runner_id": "runner:auto-manager" }
            }
        }
    }));
    let claim_text = extract_tool_text(&claim);
    let rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("result.job.revision");

    let _question = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_manager",
                "op": "report",
                "args": {
                    "job": created_text.get("result").and_then(|v| v.get("job")).and_then(|v| v.get("job_id")).and_then(|v| v.as_str()).expect("job_id"),
                    "runner_id": "runner:auto-manager",
                    "claim_revision": rev,
                    "kind": "question",
                    "message": "need manager"
                }
            }
        }
    }));

    let auto = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_manager",
                "op": "macro.respond.inbox",
                "args": { "message": "ack from manager" }
            }
        }
    }));
    let auto_text = extract_tool_text(&auto);
    assert_eq!(
        auto_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "macro.respond.inbox without job/jobs should auto-target needs_manager jobs; got: {auto_text}"
    );
    assert_eq!(
        auto_text
            .get("result")
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_i64()),
        Some(1),
        "macro.respond.inbox should post exactly one response; got: {auto_text}"
    );
}

#[test]
fn jobs_macro_enforce_proof_auto_targets_needs_proof_without_job_args() {
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_enforce_proof_auto_targets_needs_proof_without_job_args",
        &["--workspace", "ws_jobs_macro_auto_proof"],
    );

    let created = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_proof",
                "op": "create",
                "args": { "title": "auto proof", "prompt": "noop", "priority": "HIGH" }
            }
        }
    }));
    let created_text = extract_tool_text(&created);
    let job_id = created_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("result.job.job_id")
        .to_string();

    let claim = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_proof",
                "op": "claim",
                "args": { "job": job_id, "runner_id": "runner:auto-proof" }
            }
        }
    }));
    let claim_text = extract_tool_text(&claim);
    let rev = claim_text
        .get("result")
        .and_then(|v| v.get("job"))
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_i64())
        .expect("result.job.revision");

    let _proof_gate = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_proof",
                "op": "report",
                "args": {
                    "job": created_text.get("result").and_then(|v| v.get("job")).and_then(|v| v.get("job_id")).and_then(|v| v.as_str()).expect("job_id"),
                    "runner_id": "runner:auto-proof",
                    "claim_revision": rev,
                    "kind": "proof_gate",
                    "message": "attach proof"
                }
            }
        }
    }));

    let auto = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_auto_proof",
                "op": "macro.enforce.proof",
                "args": { "refs": ["CMD:echo proof"] }
            }
        }
    }));
    let auto_text = extract_tool_text(&auto);
    assert_eq!(
        auto_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "macro.enforce.proof without job/jobs should auto-target needs_proof jobs; got: {auto_text}"
    );
    assert_eq!(
        auto_text
            .get("result")
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_i64()),
        Some(1),
        "macro.enforce.proof should post exactly one response; got: {auto_text}"
    );
}

#[test]
fn jobs_macro_call_path_keeps_command_default_limit_for_auto_selection() {
    let mut server = Server::start_initialized_with_args(
        "jobs_macro_call_path_keeps_command_default_limit_for_auto_selection",
        &["--workspace", "ws_jobs_macro_call_limit_defaults"],
    );

    let respond = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_call_limit_defaults",
                "op": "call",
                "cmd": "jobs.macro.respond.inbox",
                "args": { "message": "ack", "dry_run": true }
            }
        }
    }));
    let respond_text = extract_tool_text(&respond);
    assert_eq!(
        respond_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "call-path respond.inbox should succeed; got: {respond_text}"
    );
    assert_eq!(
        respond_text
            .get("result")
            .and_then(|v| v.get("selection"))
            .and_then(|v| v.get("limit"))
            .and_then(|v| v.as_i64()),
        Some(25),
        "call-path respond.inbox should keep command default limit=25 when caller omits limit; got: {respond_text}"
    );

    let enforce = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "jobs",
            "arguments": {
                "workspace": "ws_jobs_macro_call_limit_defaults",
                "op": "call",
                "cmd": "jobs.macro.enforce.proof",
                "args": { "refs": ["CMD:echo proof"], "dry_run": true }
            }
        }
    }));
    let enforce_text = extract_tool_text(&enforce);
    assert_eq!(
        enforce_text.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "call-path enforce.proof should succeed; got: {enforce_text}"
    );
    assert_eq!(
        enforce_text
            .get("result")
            .and_then(|v| v.get("selection"))
            .and_then(|v| v.get("limit"))
            .and_then(|v| v.as_i64()),
        Some(25),
        "call-path enforce.proof should keep command default limit=25 when caller omits limit; got: {enforce_text}"
    );
}

#[test]
fn jobs_budget_block_is_consistent_when_payload_exceeds_limit() {
    let mut server = Server::start_initialized_with_args(
        "jobs_budget_block_is_consistent_when_payload_exceeds_limit",
        &["--workspace", "ws_jobs_budget_consistency"],
    );

    let cases = vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "jobs",
                "arguments": {
                    "workspace": "ws_jobs_budget_consistency",
                    "op": "radar",
                    "args": { "max_chars": 10 }
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "jobs",
                "arguments": {
                    "workspace": "ws_jobs_budget_consistency",
                    "op": "call",
                    "cmd": "jobs.control.center",
                    "args": { "max_chars": 10 }
                }
            }
        }),
    ];

    for req in cases {
        let resp = server.request(req);
        let text = extract_tool_text(&resp);
        assert_eq!(
            text.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "jobs request should succeed under tiny budget; got: {text}"
        );
        let budget = text
            .get("result")
            .and_then(|v| v.get("budget"))
            .expect("result.budget");
        let max_chars = budget
            .get("max_chars")
            .and_then(|v| v.as_i64())
            .expect("budget.max_chars");
        let used_chars = budget
            .get("used_chars")
            .and_then(|v| v.as_i64())
            .expect("budget.used_chars");
        let truncated = budget
            .get("truncated")
            .and_then(|v| v.as_bool())
            .expect("budget.truncated");
        assert!(
            used_chars <= max_chars || truncated,
            "budget must never report used_chars > max_chars with truncated=false; got budget={budget:?}"
        );
    }
}
