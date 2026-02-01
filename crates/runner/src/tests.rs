#![forbid(unsafe_code)]

use super::*;
use serde_json::json;

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

#[test]
fn codex_output_schema_required_matches_properties() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "bm_runner_schema_test_{}_{}",
        std::process::id(),
        now_ms()
    ));
    let schema_path =
        executors::output_schema::write_job_output_schema_file(&tmp_dir).expect("write schema");
    let text = std::fs::read_to_string(&schema_path).expect("read schema");
    let schema: Value = serde_json::from_str(&text).expect("parse schema");

    assert_eq!(required_keys(&schema), sorted_property_keys(&schema));

    let events_item = schema
        .get("properties")
        .and_then(|v| v.get("events"))
        .and_then(|v| v.get("items"))
        .cloned()
        .expect("events.items");
    assert_eq!(
        required_keys(&events_item),
        sorted_property_keys(&events_item)
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn proof_gate_requires_non_job_refs() {
    let job = "JOB-001";
    assert!(!has_non_job_proof_ref(job, &[]), "empty refs must fail");
    assert!(
        !has_non_job_proof_ref(job, &[job.to_string()]),
        "job id alone is navigation, not proof"
    );
    assert!(
        !has_non_job_proof_ref(job, &["JOB-001@2".to_string()]),
        "job event refs are navigation, not proof"
    );
    assert!(
        !has_non_job_proof_ref(job, &["a:core".to_string()]),
        "anchors are meaning pointers, not proof"
    );
    assert!(has_non_job_proof_ref(job, &["CARD-1".to_string()]));
    assert!(has_non_job_proof_ref(job, &["TASK-123".to_string()]));
    assert!(has_non_job_proof_ref(job, &["notes@42".to_string()]));
    assert!(has_non_job_proof_ref(
        job,
        &["LINK: ci-run-123".to_string()]
    ));
    assert!(has_non_job_proof_ref(job, &["CMD: cargo test".to_string()]));
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
        "skill profile=strict version=0.1.0\n[CORE LOOP]\n...",
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
