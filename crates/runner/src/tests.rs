#![forbid(unsafe_code)]

use super::*;
use serde_json::{Value, json};

fn sorted_property_keys(schema: &Value) -> Vec<String> {
    let mut keys = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn required_keys(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn assert_required_matches_properties_recursive(schema: &Value) {
    if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
        for item in any_of {
            assert_required_matches_properties_recursive(item);
        }
        return;
    }

    if schema.get("type").and_then(|v| v.as_str()) == Some("object") {
        let keys = sorted_property_keys(schema);
        let mut req = required_keys(schema);
        req.sort();
        assert_eq!(req, keys);

        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for value in props.values() {
                assert_required_matches_properties_recursive(value);
            }
        }
        return;
    }

    if schema.get("type").and_then(|v| v.as_str()) == Some("array")
        && let Some(items) = schema.get("items")
    {
        assert_required_matches_properties_recursive(items);
    }
}

#[test]
fn codex_output_schema_required_matches_properties() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "bm_runner_schema_test_{}_{}",
        std::process::id(),
        now_ms()
    ));
    let schema_path =
        executors::output_schema::write_job_output_schema_file_for_role(&tmp_dir, None)
            .expect("write schema");
    let text = std::fs::read_to_string(&schema_path).expect("read schema");
    let schema: Value = serde_json::from_str(&text).expect("parse schema");

    assert_required_matches_properties_recursive(&schema);

    for role in [
        Some("scout"),
        Some("builder"),
        Some("validator"),
        Some("writer"),
    ] {
        let schema = executors::output_schema::job_output_schema_value_for_role(role);
        assert_required_matches_properties_recursive(&schema);
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn scout_output_schema_avoids_v2_drift_and_has_array_items() {
    let schema = executors::output_schema::job_output_schema_value_for_role(Some("scout"));
    let summary = schema
        .get("properties")
        .and_then(|v| v.get("summary"))
        .expect("summary schema");
    let required = required_keys(summary);
    assert!(
        !required.iter().any(|k| k == "format_version"),
        "scout summary schema must remain v1-compatible unless full v2 anchors are emitted"
    );
    assert!(
        required.iter().any(|k| k == "coverage_matrix"),
        "coverage_matrix must be required to force explicit change_hint↔anchor lineage in scout output"
    );

    // Ensure common array nodes declare `items` (Codex JSON schema requirement).
    let anchors = summary
        .get("properties")
        .and_then(|v| v.get("anchors"))
        .expect("anchors schema");
    assert!(
        anchors.get("items").is_some(),
        "codex JSON schema requires items for array nodes"
    );

    let coverage_matrix = summary
        .get("properties")
        .and_then(|v| v.get("coverage_matrix"))
        .expect("coverage_matrix schema");
    assert_eq!(
        coverage_matrix.get("type").and_then(|v| v.as_str()),
        Some("object"),
        "scout summary schema must expose coverage_matrix contract for change_hint↔anchor lineage"
    );
}

#[test]
fn scout_anchor_items_require_all_declared_properties_for_codex_schema() {
    let schema = executors::output_schema::job_output_schema_value_for_role(Some("scout"));
    let anchor_item = schema
        .get("properties")
        .and_then(|v| v.get("summary"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.get("anchors"))
        .and_then(|v| v.get("items"))
        .expect("scout anchors.items schema");
    let mut required = required_keys(anchor_item);
    required.sort();
    assert_eq!(
        required,
        sorted_property_keys(anchor_item),
        "scout anchors.items must keep required[] aligned with properties for Codex schema subset"
    );
}

#[test]
fn pipeline_roles_require_exec_mcp_isolation() {
    assert!(role_requires_exec_mcp_isolation(Some("scout")));
    assert!(role_requires_exec_mcp_isolation(Some("builder")));
    assert!(role_requires_exec_mcp_isolation(Some("validator")));
    assert!(role_requires_exec_mcp_isolation(Some("writer")));
    assert!(role_requires_exec_mcp_isolation(Some("SCOUT")));
    assert!(!role_requires_exec_mcp_isolation(Some("manager")));
    assert!(!role_requires_exec_mcp_isolation(None));
}

#[test]
fn builder_output_schema_enforces_execution_evidence_object_shape() {
    let schema = executors::output_schema::job_output_schema_value_for_role(Some("builder"));
    let summary = schema
        .get("properties")
        .and_then(|v| v.get("summary"))
        .expect("builder summary schema");
    let any_of = summary
        .get("anyOf")
        .and_then(|v| v.as_array())
        .expect("builder summary schema must be anyOf (context_request optional)");

    let mut saw_context_request = false;
    for variant in any_of {
        let props = variant
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("builder summary variant.properties");

        let changes = props.get("changes").expect("builder changes schema");
        assert!(
            changes.get("minItems").is_none(),
            "builder changes must allow [] for context_request path"
        );

        if let Some(context_request) = props.get("context_request") {
            saw_context_request = true;
            assert_eq!(
                context_request.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "context_request must be declared as object in builder schema variant"
            );
        }

        let evidence = props
            .get("execution_evidence")
            .expect("builder execution_evidence schema");
        let semantic_guards = evidence
            .get("properties")
            .and_then(|v| v.get("semantic_guards"))
            .expect("semantic_guards schema");
        assert_eq!(
            semantic_guards.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "semantic_guards must be object to match MCP validator contract"
        );
        let required = semantic_guards
            .get("required")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            required
                .iter()
                .any(|v| v.as_str() == Some("must_should_may_delta")),
            "semantic_guards must require must_should_may_delta"
        );
        assert!(
            required
                .iter()
                .any(|v| v.as_str() == Some("contract_term_consistency")),
            "semantic_guards must require contract_term_consistency"
        );
    }

    assert!(
        saw_context_request,
        "builder schema must include context_request in at least one anyOf variant"
    );
}

#[test]
fn proof_gate_requires_non_job_refs() {
    let job = "JOB-001";
    assert!(!has_non_job_ref(job, &[]), "empty refs must fail");
    assert!(
        !has_non_job_ref(job, &[job.to_string()]),
        "job id alone is navigation, not proof"
    );
    assert!(
        !has_non_job_ref(job, &["JOB-001@2".to_string()]),
        "job event refs are navigation, not proof"
    );
    assert!(
        !has_non_job_ref(job, &["a:core".to_string()]),
        "anchors are meaning pointers, not proof"
    );
    assert!(has_non_job_ref(job, &["CARD-1".to_string()]));
    assert!(has_non_job_ref(job, &["TASK-123".to_string()]));
    assert!(has_non_job_ref(job, &["notes@42".to_string()]));
    assert!(has_non_job_ref(job, &["LINK: ci-run-123".to_string()]));
    assert!(has_non_job_ref(job, &["CMD: cargo test".to_string()]));
}

#[test]
fn high_priority_done_requires_strict_proof_refs() {
    let job = "JOB-001";
    assert!(
        !has_done_proof_ref(job, "HIGH", &["CARD-1".to_string()]),
        "HIGH priority requires strict receipts, CARD-* alone is not enough"
    );
    assert!(has_done_proof_ref(
        job,
        "HIGH",
        &["LINK: https://example.com/ci/run/123".to_string()]
    ));
    assert!(has_done_proof_ref(
        job,
        "HIGH",
        &["CMD: cargo test".to_string()]
    ));
    assert!(has_done_proof_ref(
        job,
        "HIGH",
        &["FILE: /tmp/out.txt".to_string()]
    ));
}

#[test]
fn salvage_proof_refs_extracts_cmd_and_link_lines() {
    let text = "did stuff\ncmd: cargo test -q\nmore\nLINK: ci-run-123\n";
    let refs = salvage_proof_refs_from_text(text);
    assert!(refs.contains(&"CMD: cargo test -q".to_string()));
    assert!(refs.contains(&"LINK: ci-run-123".to_string()));
}

#[test]
fn salvage_proof_refs_extracts_embedded_card_task_notes_tokens() {
    let text = "see CARD-123 and task-456; notes@42. also: JOB-001";
    let refs = salvage_proof_refs_from_text(text);
    assert!(refs.contains(&"CARD-123".to_string()));
    assert!(refs.contains(&"TASK-456".to_string()));
    assert!(refs.contains(&"notes@42".to_string()));
    assert!(
        !refs.iter().any(|r| r == "JOB-001"),
        "job ids are not proof refs"
    );
}

#[test]
fn salvage_proof_refs_extracts_plain_urls_as_link() {
    let text = "CI: https://example.com/ci/run/123 (green)";
    let refs = salvage_proof_refs_from_text(text);
    assert!(refs.contains(&"LINK: https://example.com/ci/run/123".to_string()));
}

#[test]
fn salvage_proof_refs_extracts_markdown_bullet_commands_carefully() {
    let text = "- cargo test -q\n- Updated docs\n- $ pytest -q\n";
    let refs = salvage_proof_refs_from_text(text);
    assert!(refs.contains(&"CMD: cargo test -q".to_string()));
    assert!(refs.contains(&"CMD: pytest -q".to_string()));
    assert!(
        !refs.iter().any(|r| r.contains("Updated docs")),
        "should not treat prose bullets as commands"
    );
}

#[test]
fn normalize_skill_profile_accepts_known_profiles() {
    assert_eq!(normalize_skill_profile("daily").as_deref(), Some("daily"));
    assert_eq!(normalize_skill_profile("STRICT").as_deref(), Some("strict"));
    assert_eq!(
        normalize_skill_profile(" research ").as_deref(),
        Some("research")
    );
    assert_eq!(
        normalize_skill_profile("teamlead").as_deref(),
        Some("teamlead")
    );
    assert_eq!(normalize_skill_profile("unknown"), None);
}

#[test]
fn build_subagent_prompt_includes_skill_pack_when_present() {
    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: PathBuf::from("."),
        repo_root: PathBuf::from("."),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: "codex".to_string(),
        claude_bin: None,
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };
    let prompt = build_subagent_prompt(
        &cfg,
        "JOB-1",
        "do thing",
        "CTX",
        "skill profile=strict version=0.1.2\n[CORE LOOP]\n...",
        None,
        None,
    );
    assert!(prompt.contains("SKILL PACK (bounded):"));
    assert!(prompt.contains("skill profile=strict"));
    assert!(prompt.contains("JOB SPEC:\n"));
}

#[test]
fn skill_selection_prefers_job_meta_then_kind_then_default() {
    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: PathBuf::from("."),
        repo_root: PathBuf::from("."),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: "codex".to_string(),
        claude_bin: None,
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };

    let meta = json!({"skill_profile":"daily"});
    assert_eq!(
        select_skill_profile(Some("codex_cli"), Some(&meta), &cfg),
        "daily".to_string()
    );

    let no_meta = json!(null);
    assert_eq!(
        select_skill_profile(Some("research_probe"), Some(&no_meta), &cfg),
        "research".to_string()
    );

    assert_eq!(select_skill_profile(None, None, &cfg), "strict".to_string());
}

#[test]
fn skill_budget_can_be_overridden_or_disabled_by_job_meta() {
    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: PathBuf::from("."),
        repo_root: PathBuf::from("."),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: "codex".to_string(),
        claude_bin: None,
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };

    let meta = json!({"skill_max_chars": 500});
    assert_eq!(select_skill_max_chars(Some(&meta), &cfg), 500);

    let meta_off = json!({"skill_max_chars": 0});
    assert_eq!(select_skill_max_chars(Some(&meta_off), &cfg), 0);

    assert_eq!(select_skill_max_chars(None, &cfg), 1200);
}

fn write_stub_exe(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\necho ok\n").expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).expect("meta").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
    }
    path
}

fn temp_dir(prefix: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("{prefix}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn test_runner_cfg_for_timing() -> RunnerConfig {
    let dir = temp_dir("bm_runner_slice_timing");
    RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: dir.clone(),
        repo_root: dir,
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 60_000,
        max_runtime_s: 7200,
        slice_s: 1800,
        slice_grace_s: 300,
        stale_after_s: 600,
        max_failures: 3,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: "codex".to_string(),
        claude_bin: None,
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    }
}

#[test]
fn auto_executor_prefers_claude_for_deep_when_available() {
    let dir = temp_dir("bm_runner_exec_select");
    let codex = write_stub_exe(&dir, "codex");
    let claude = write_stub_exe(&dir, "claude");

    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: dir.clone(),
        repo_root: dir.clone(),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: codex.to_string_lossy().to_string(),
        claude_bin: Some(claude.to_string_lossy().to_string()),
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };

    let meta = json!({"executor":"auto","executor_profile":"deep"});
    let (kind, profile, _model) = resolve_job_executor_plan(Some(&meta), &cfg).expect("plan");
    assert_eq!(profile, "deep");
    assert_eq!(kind, executors::ExecutorKind::ClaudeCode);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_executor_prefers_codex_for_fast_when_both_available() {
    let dir = temp_dir("bm_runner_exec_select2");
    let codex = write_stub_exe(&dir, "codex");
    let claude = write_stub_exe(&dir, "claude");

    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: dir.clone(),
        repo_root: dir.clone(),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: codex.to_string_lossy().to_string(),
        claude_bin: Some(claude.to_string_lossy().to_string()),
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };

    let meta = json!({"executor":"auto","executor_profile":"fast"});
    let (kind, profile, _model) = resolve_job_executor_plan(Some(&meta), &cfg).expect("plan");
    assert_eq!(profile, "fast");
    assert_eq!(kind, executors::ExecutorKind::Codex);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_executor_prefers_codex_for_xhigh_when_both_available() {
    let dir = temp_dir("bm_runner_exec_select_xhigh");
    let codex = write_stub_exe(&dir, "codex");
    let claude = write_stub_exe(&dir, "claude");

    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: dir.clone(),
        repo_root: dir.clone(),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: codex.to_string_lossy().to_string(),
        claude_bin: Some(claude.to_string_lossy().to_string()),
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };

    let meta = json!({"executor":"auto","executor_profile":"xhigh"});
    let (kind, profile, _model) = resolve_job_executor_plan(Some(&meta), &cfg).expect("plan");
    assert_eq!(profile, "xhigh");
    assert_eq!(kind, executors::ExecutorKind::Codex);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn executor_model_passthrough_is_preserved_for_codex_and_claude() {
    let dir = temp_dir("bm_runner_exec_model_passthrough");
    let codex = write_stub_exe(&dir, "codex");
    let claude = write_stub_exe(&dir, "claude");

    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: dir.clone(),
        repo_root: dir.clone(),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: codex.to_string_lossy().to_string(),
        claude_bin: Some(claude.to_string_lossy().to_string()),
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };

    let codex_meta = json!({
        "executor": "codex",
        "executor_profile": "xhigh",
        "executor_model": "gpt-5.3-codex"
    });
    let (codex_kind, _, codex_model) =
        resolve_job_executor_plan(Some(&codex_meta), &cfg).expect("codex plan");
    assert_eq!(codex_kind, executors::ExecutorKind::Codex);
    assert_eq!(codex_model.as_deref(), Some("gpt-5.3-codex"));

    let claude_meta = json!({
        "executor": "claude_code",
        "executor_profile": "deep",
        "executor_model": "claude-3-7-sonnet"
    });
    let (claude_kind, _, claude_model) =
        resolve_job_executor_plan(Some(&claude_meta), &cfg).expect("claude plan");
    assert_eq!(claude_kind, executors::ExecutorKind::ClaudeCode);
    assert_eq!(claude_model.as_deref(), Some("claude-3-7-sonnet"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn slice_timing_caps_scout_role_to_sla_by_default() {
    let cfg = test_runner_cfg_for_timing();
    let timing = resolve_slice_timing(&cfg, Some(&json!({})), Some("scout"));
    assert_eq!(timing.slice_s, SCOUT_SLICE_SLA_S);
    assert_eq!(
        timing.slice_grace_s,
        cfg.slice_grace_s.min(SCOUT_SLICE_SLA_S)
    );
    assert_eq!(timing.heartbeat_ms, SCOUT_HEARTBEAT_SLA_MS);
    let _ = std::fs::remove_dir_all(&cfg.storage_dir);
}

#[test]
fn slice_timing_respects_job_meta_override_with_safety_clamps() {
    let cfg = test_runner_cfg_for_timing();
    let meta = json!({
        "slice_s": 45,
        "slice_grace_s": 90,
        "heartbeat_ms": 5000
    });
    let timing = resolve_slice_timing(&cfg, Some(&meta), Some("scout"));
    assert_eq!(timing.slice_s, 45);
    assert_eq!(timing.slice_grace_s, 45, "grace cannot exceed slice_s");
    assert_eq!(
        timing.heartbeat_ms, MIN_HEARTBEAT_MS,
        "heartbeat is clamped to avoid runner spam / jitter"
    );
    let _ = std::fs::remove_dir_all(&cfg.storage_dir);
}

#[test]
fn claude_code_output_unwraps_structured_output() {
    let dir = temp_dir("bm_runner_claude_structured_output");
    let out_path = dir.join("out.json");

    // This matches the shape emitted by Claude Code CLI when run with:
    // - `--output-format json`
    // - `--json-schema <schema>`
    //
    // The runner expects the schema-validated payload directly, so we must unwrap it.
    let wrapper = json!({
        "type": "result",
        "subtype": "success",
        "structured_output": {
            "status": "DONE",
            "summary": "ok",
            "refs": ["CMD: echo ok"],
            "events": []
        }
    });
    std::fs::write(&out_path, serde_json::to_string(&wrapper).expect("json"))
        .expect("write stub output");

    let parsed = executors::claude_code::read_output(&out_path).expect("read_output");
    assert_eq!(
        parsed.get("status").and_then(|v| v.as_str()),
        Some("DONE"),
        "expected unwrapped structured_output"
    );
    assert!(
        parsed.get("structured_output").is_none(),
        "must return the structured payload, not the wrapper"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_subagent_prompt_includes_scout_contract_when_role_is_scout() {
    let cfg = RunnerConfig {
        workspace: "ws".to_string(),
        storage_dir: PathBuf::from("."),
        repo_root: PathBuf::from("."),
        runner_id: "r".to_string(),
        poll_ms: 1000,
        heartbeat_ms: 1000,
        max_runtime_s: 10,
        slice_s: 1,
        slice_grace_s: 0,
        stale_after_s: 1,
        max_failures: 1,
        once: true,
        dry_run: true,
        mcp_bin: "bm_mcp".to_string(),
        codex_bin: "codex".to_string(),
        claude_bin: None,
        skill_profile: "strict".to_string(),
        skill_max_chars: 1200,
    };
    let prompt = build_subagent_prompt(&cfg, "JOB-1", "spec", "ctx", "", Some("scout"), None);
    assert!(prompt.contains("PIPELINE ROLE: SCOUT"));
    assert!(prompt.contains("MUST NOT include code/patch/diff/apply"));
    assert!(prompt.contains("Every `change_hints[].path` MUST be covered"));
    assert!(prompt.contains("coverage_matrix.change_hint_coverage[]"));
}

#[test]
fn validate_pipeline_summary_contract_rejects_invalid_scout_summary() {
    let bad = "{\"objective\":\"x\",\"code_refs\":[\"code:a#L1-L1@sha256:bad\"],\"summary_for_builder\":\"ok\"}";
    let err = validate_pipeline_summary_contract("scout", bad).expect_err("must reject short refs");
    assert!(
        err.contains("code_refs"),
        "expected code_refs validation error, got: {err}"
    );
}

#[test]
fn validate_pipeline_summary_contract_rejects_weak_scout_context() {
    let weak = r#"{
        "objective":"x",
        "scope":{"in":["README.md"],"out":["crates/*"]},
        "anchors":[
            {"id":"a1","rationale":"r1"},
            {"id":"a2","rationale":"r2"},
            {"id":"a3","rationale":"r3"}
        ],
        "code_refs":[
            "code:README.md#L1-L1@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "code:README.md#L2-L2@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "code:README.md#L3-L3@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ],
        "change_hints":[
            {"path":"README.md","intent":"x","risk":"low"},
            {"path":"README.md","intent":"y","risk":"low"}
        ],
        "test_hints":["cargo test -q","cargo clippy -q","cargo test -p bm_runner"],
        "risk_map":[
            {"risk":"docs drift","falsifier":"contract lint"},
            {"risk":"regression","falsifier":"smoke"},
            {"risk":"coverage holes","falsifier":"targeted audit"}
        ],
        "open_questions":[],
        "summary_for_builder":"too short"
    }"#;
    let err =
        validate_pipeline_summary_contract("scout", weak).expect_err("must reject weak summary");
    assert!(
        err.contains("summary_for_builder"),
        "expected summary quality validation error, got: {err}"
    );
}

#[test]
fn validate_pipeline_summary_contract_rejects_non_strict_code_ref_shape() {
    let bad = r#"{
        "objective":"context",
        "scope":{"in":["README.md"],"out":["tests"]},
        "anchors":[
            {"id":"a:1","rationale":"anchor one rationale"},
            {"id":"a:2","rationale":"anchor two rationale"},
            {"id":"a:3","rationale":"anchor three rationale"}
        ],
        "code_refs":[
            "README.md#L1-L3",
            "code:README.md#L4-L6",
            "code:README.md#L7-L9"
        ],
        "change_hints":[
            {"path":"README.md","intent":"x","risk":"low"},
            {"path":"README.md","intent":"y","risk":"low"}
        ],
        "test_hints":["cargo test","cargo test -p bm_runner","cargo test -p bm_mcp"],
        "risk_map":[
            {"risk":"docs drift","falsifier":"lint"},
            {"risk":"regression","falsifier":"tests"},
            {"risk":"coverage","falsifier":"audit"}
        ],
        "open_questions":[],
        "summary_for_builder":"This summary is deliberately long enough to pass the minimum builder handoff threshold (>=320 chars). It exists to ensure the runner-side contract rejects malformed CODE_REF tokens before the pipeline proceeds. We include enough context and wording so the length check does not mask the intended validation error. The content is irrelevant; the shape is the point."
    }"#;
    let err = validate_pipeline_summary_contract("scout", bad).expect_err("must reject bad ref");
    assert!(
        err.contains("CODE_REF token"),
        "expected CODE_REF token error, got: {err}"
    );
}

#[test]
fn validate_pipeline_summary_contract_rejects_uncovered_change_hints_in_scout_v2() {
    let scout_v2 = json!({
        "format_version": 2,
        "objective": "verify pipeline context",
        "anchors": [
            {
                "id": "a:primary",
                "anchor_type": "primary",
                "rationale": "main pipeline entry",
                "code_ref": "code:crates/mcp/src/handlers/tasks/jobs/pipeline.rs#L1-L30",
                "content": "pipeline entry",
                "line_count": 30
            },
            {
                "id": "a:dep",
                "anchor_type": "dependency",
                "rationale": "artifacts layer",
                "code_ref": "code:crates/storage/src/store/jobs/artifacts.rs#L1-L50",
                "content": "artifact persistence",
                "line_count": 50
            },
            {
                "id": "a:ref",
                "anchor_type": "reference",
                "rationale": "contract rules",
                "code_ref": "code:crates/mcp/src/support/artifact_contracts/mod.rs#L1-L40",
                "content": "contract",
                "line_count": 40
            }
        ],
        "change_hints": [
            { "path": "crates/mcp/src/handlers/tasks/jobs/pipeline.rs", "intent": "scope", "risk": "low" },
            { "path": "crates/storage/src/store/jobs/artifacts.rs", "intent": "scope", "risk": "medium" }
        ],
        "summary_for_builder": "long enough summary for v2 scout contract"
    })
    .to_string();

    let err = validate_pipeline_summary_contract("scout", &scout_v2)
        .expect_err("must reject uncovered change_hints in v2 scout pack");
    assert!(
        err.contains("missing primary/structural anchor coverage"),
        "expected coverage error, got: {err}"
    );
}

#[test]
fn clamp_scout_summary_code_refs_promotes_anchor_coverage_for_change_hints() {
    let raw = json!({
        "format_version": 2,
        "objective": "verify pipeline context",
        "anchors": [
            {
                "id": "a:primary",
                "anchor_type": "primary",
                "rationale": "pipeline entry",
                "code_ref": "code:crates/mcp/src/handlers/tasks/jobs/pipeline.rs#L1-L30",
                "content": "pipeline entry",
                "line_count": 30
            },
            {
                "id": "a:dep",
                "anchor_type": "dependency",
                "rationale": "artifacts layer",
                "code_ref": "code:crates/storage/src/store/jobs/artifacts.rs#L1-L50",
                "content": "artifact persistence",
                "line_count": 50
            },
            {
                "id": "a:ref",
                "anchor_type": "reference",
                "rationale": "contract rules",
                "code_ref": "code:crates/mcp/src/support/artifact_contracts/mod.rs#L1-L40",
                "content": "contract",
                "line_count": 40
            }
        ],
        "code_refs": [
            "code:crates/mcp/src/handlers/tasks/jobs/pipeline.rs#L1-L30",
            "code:crates/storage/src/store/jobs/artifacts.rs#L1-L50",
            "code:crates/mcp/src/support/artifact_contracts/mod.rs#L1-L40"
        ],
        "change_hints": [
            { "path": "crates/mcp/src/handlers/tasks/jobs/pipeline.rs", "intent": "scope", "risk": "low" },
            { "path": "crates/storage/src/store/jobs/artifacts.rs", "intent": "scope", "risk": "medium" }
        ],
        "summary_for_builder": "long enough summary for v2 scout contract"
    })
    .to_string();

    let normalized = clamp_scout_summary_code_refs(&raw, 24);
    validate_pipeline_summary_contract("scout", &normalized)
        .expect("normalized scout summary must satisfy v2 coverage checks");

    let parsed: Value = serde_json::from_str(&normalized).expect("normalized json");
    let anchors = parsed
        .get("anchors")
        .and_then(|v| v.as_array())
        .expect("anchors array");
    let artifacts_anchor_type = anchors
        .iter()
        .filter_map(|a| a.as_object())
        .find(|obj| {
            obj.get("code_ref")
                .and_then(|v| v.as_str())
                .is_some_and(|raw| raw.contains("crates/storage/src/store/jobs/artifacts.rs"))
        })
        .and_then(|obj| obj.get("anchor_type"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert_eq!(artifacts_anchor_type, "structural");
}

fn valid_builder_batch() -> String {
    serde_json::json!({
        "slice_id": "SLC-001",
        "changes": [{
            "path": "crates/mcp/src/handlers/tasks/jobs/pipeline.rs",
            "intent": "tighten strict scout gate",
            "diff_ref": "FILE: artifact://jobs/JOB-100/diff.patch",
            "estimated_risk": "medium"
        }],
        "checks_to_run": ["cargo test -p bm_mcp --test jobs_ai_first_ux"],
        "rollback_plan": "revert builder gate commit and rerun targeted suite",
        "proof_refs": ["CMD: cargo test -p bm_mcp --test jobs_ai_first_ux"],
        "execution_evidence": {
            "revision": 2,
            "diff_scope": ["crates/mcp/src/handlers/tasks/jobs/pipeline.rs"],
            "command_runs": [{
                "cmd": "cargo test -p bm_mcp --test jobs_ai_first_ux",
                "exit_code": 0,
                "stdout_ref": "FILE: artifact://ci/stdout/jobs_ai_first_ux",
                "stderr_ref": "FILE: artifact://ci/stderr/jobs_ai_first_ux"
            }],
            "rollback_proof": {
                "strategy": "git revert",
                "target_revision": 1,
                "verification_cmd_ref": "CMD: git status --porcelain"
            },
            "semantic_guards": {
                "must_should_may_delta": "must preserved",
                "contract_term_consistency": "terms unchanged"
            }
        }
    })
    .to_string()
}

#[test]
fn builder_contract_accepts_context_request_with_empty_changes() {
    let mut batch: Value =
        serde_json::from_str(&valid_builder_batch()).expect("valid builder batch");
    batch["changes"] = json!([]);
    batch["checks_to_run"] = json!([]);
    batch["execution_evidence"]["diff_scope"] = json!([]);
    batch["context_request"] = json!({
        "reason": "Need stronger dependency context before code changes",
        "missing_context": ["dependency anchor for runner proof gate path"],
        "suggested_scout_focus": ["crates/runner/src/pipeline_contract.rs"],
        "suggested_tests": ["cargo test -p bm_runner -q"]
    });
    validate_pipeline_summary_contract("builder", &batch.to_string())
        .expect("builder context_request summary must pass");
}

#[test]
fn builder_contract_rejects_context_request_when_changes_present() {
    let mut batch: Value =
        serde_json::from_str(&valid_builder_batch()).expect("valid builder batch");
    batch["context_request"] = json!({
        "reason": "Need stronger dependency context before code changes",
        "missing_context": ["dependency anchor for runner proof gate path"],
        "suggested_scout_focus": ["crates/runner/src/pipeline_contract.rs"],
        "suggested_tests": ["cargo test -p bm_runner -q"]
    });
    let err = validate_pipeline_summary_contract("builder", &batch.to_string())
        .expect_err("must reject mixed changes + context_request");
    assert!(
        err.contains("context_request requires changes=[]"),
        "expected context_request exclusivity error, got: {err}"
    );
}

// ── Writer pipeline contract tests ──

fn valid_writer_pack() -> String {
    serde_json::json!({
        "slice_id": "SLICE-001",
        "patches": [{
            "path": "src/main.rs",
            "ops": [{
                "kind": "replace",
                "old_lines": ["fn old() {}"],
                "new_lines": ["fn new() {}"]
            }]
        }],
        "summary": "Replace old with new",
        "affected_files": ["src/main.rs"],
        "checks_to_run": ["cargo test"]
    })
    .to_string()
}

#[test]
fn writer_contract_accepts_valid_pack() {
    validate_pipeline_summary_contract("writer", &valid_writer_pack()).expect("must accept");
}

#[test]
fn writer_contract_rejects_missing_slice_id() {
    let pack = serde_json::json!({
        "patches": [],
        "summary": "x",
        "affected_files": [],
        "insufficient_context": "not enough info"
    })
    .to_string();
    let err = validate_pipeline_summary_contract("writer", &pack).expect_err("must reject");
    assert!(err.contains("slice_id"), "got: {err}");
}

#[test]
fn writer_contract_rejects_empty_patches_without_escape() {
    let pack = serde_json::json!({
        "slice_id": "S1",
        "patches": [],
        "summary": "x",
        "affected_files": []
    })
    .to_string();
    let err = validate_pipeline_summary_contract("writer", &pack).expect_err("must reject");
    assert!(err.contains("patches"), "got: {err}");
}

#[test]
fn writer_contract_accepts_empty_patches_with_escape() {
    let pack = serde_json::json!({
        "slice_id": "S1",
        "patches": [],
        "summary": "Insufficient context",
        "affected_files": [],
        "insufficient_context": "Scout did not provide enough detail"
    })
    .to_string();
    validate_pipeline_summary_contract("writer", &pack).expect("must accept with escape hatch");
}

#[test]
fn writer_contract_rejects_path_traversal() {
    let pack = serde_json::json!({
        "slice_id": "S1",
        "patches": [{
            "path": "../etc/passwd",
            "ops": [{"kind": "delete_file"}]
        }],
        "summary": "evil",
        "affected_files": []
    })
    .to_string();
    let err = validate_pipeline_summary_contract("writer", &pack).expect_err("must reject");
    assert!(err.contains("path traversal"), "got: {err}");
}

#[test]
fn writer_contract_rejects_invalid_op_kind() {
    let pack = serde_json::json!({
        "slice_id": "S1",
        "patches": [{
            "path": "src/lib.rs",
            "ops": [{"kind": "execute_shell", "cmd": "rm -rf /"}]
        }],
        "summary": "nope",
        "affected_files": []
    })
    .to_string();
    let err = validate_pipeline_summary_contract("writer", &pack).expect_err("must reject");
    assert!(err.contains("kind"), "got: {err}");
}

#[test]
fn writer_contract_rejects_replace_without_old_lines() {
    let pack = serde_json::json!({
        "slice_id": "S1",
        "patches": [{
            "path": "src/lib.rs",
            "ops": [{
                "kind": "replace",
                "old_lines": [],
                "new_lines": ["x"]
            }]
        }],
        "summary": "x",
        "affected_files": []
    })
    .to_string();
    let err = validate_pipeline_summary_contract("writer", &pack).expect_err("must reject");
    assert!(err.contains("old_lines"), "got: {err}");
}

#[test]
fn writer_contract_validates_all_op_kinds() {
    let pack = serde_json::json!({
        "slice_id": "S1",
        "patches": [
            {
                "path": "src/a.rs",
                "ops": [{
                    "kind": "replace",
                    "old_lines": ["old"],
                    "new_lines": ["new"],
                    "anchor_ref": "a:test"
                }]
            },
            {
                "path": "src/b.rs",
                "ops": [{
                    "kind": "insert_after",
                    "after": ["// marker"],
                    "content": ["// inserted"]
                }]
            },
            {
                "path": "src/c.rs",
                "ops": [{
                    "kind": "insert_before",
                    "before": ["fn main()"],
                    "content": ["// header"]
                }]
            },
            {
                "path": "src/new.rs",
                "ops": [{"kind": "create_file", "content": ["fn main() {}"]}]
            },
            {
                "path": "src/old.rs",
                "ops": [{"kind": "delete_file"}]
            }
        ],
        "summary": "Multi-op test",
        "affected_files": ["src/a.rs", "src/b.rs", "src/c.rs", "src/new.rs", "src/old.rs"],
        "checks_to_run": ["cargo test"]
    })
    .to_string();
    validate_pipeline_summary_contract("writer", &pack).expect("must accept all op kinds");
}

// ── Cascade retry context injection tests ──

#[test]
fn cascade_retry_hints_injected_into_prompt() {
    let cfg = RunnerConfig {
        workspace: "ws".into(),
        storage_dir: "/tmp".into(),
        repo_root: "/tmp".into(),
        runner_id: "r-test".into(),
        poll_ms: 1000,
        heartbeat_ms: 30000,
        max_runtime_s: 600,
        slice_s: 300,
        slice_grace_s: 30,
        stale_after_s: 180,
        max_failures: 3,
        once: false,
        dry_run: false,
        mcp_bin: "bm_mcp".into(),
        codex_bin: "codex".into(),
        claude_bin: None,
        skill_profile: "strict".into(),
        skill_max_chars: 1200,
    };
    let meta = serde_json::json!({
        "pipeline_role": "scout",
        "cascade_retry_hints": [
            "Missing coverage for error handling in AuthService",
            "No dependency anchor for src/types.rs"
        ],
        "cascade_previous_ref": "artifact://jobs/JOB-1/scout_context_pack"
    });
    let prompt = build_subagent_prompt(
        &cfg,
        "JOB-2",
        "Improve scout context",
        "CTX",
        "",
        Some("scout"),
        Some(&meta),
    );
    assert!(
        prompt.contains("RETRY CONTEXT"),
        "must include retry context block"
    );
    assert!(prompt.contains("AuthService"), "must include hint content");
    assert!(
        prompt.contains("artifact://jobs/JOB-1"),
        "must include previous ref"
    );
}

#[test]
fn cascade_no_retry_context_when_absent() {
    let cfg = RunnerConfig {
        workspace: "ws".into(),
        storage_dir: "/tmp".into(),
        repo_root: "/tmp".into(),
        runner_id: "r-test".into(),
        poll_ms: 1000,
        heartbeat_ms: 30000,
        max_runtime_s: 600,
        slice_s: 300,
        slice_grace_s: 30,
        stale_after_s: 180,
        max_failures: 3,
        once: false,
        dry_run: false,
        mcp_bin: "bm_mcp".into(),
        codex_bin: "codex".into(),
        claude_bin: None,
        skill_profile: "strict".into(),
        skill_max_chars: 1200,
    };
    let prompt = build_subagent_prompt(&cfg, "JOB-1", "spec", "ctx", "", Some("scout"), None);
    assert!(
        !prompt.contains("RETRY CONTEXT"),
        "no retry context without meta"
    );
}
