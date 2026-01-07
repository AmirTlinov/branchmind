#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_transcripts_{prefix}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn transcripts_search_finds_hits_and_open_reads_window() {
    let mut server = Server::start_initialized("transcripts_search_finds_hits");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts" } }
    }));

    let root_dir = make_temp_dir("root");
    let file_path = root_dir.join("rollout-test.jsonl");
    let content = r#"
{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-1","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}
{"timestamp":"2026-01-06T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello branchmind transcripts"}]}}
{"timestamp":"2026-01-06T00:00:02.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}}
"#;
    std::fs::write(&file_path, content.trim_start()).expect("write transcript file");

    let search = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_search", "arguments": {
            "workspace": "ws_transcripts",
            "root_dir": root_dir.to_string_lossy(),
            "query": "branchmind",
            "cwd_prefix": "",
            "hits_limit": 5,
            "max_chars": 8000
        } }
    }));
    let search_text = extract_tool_text(&search);
    assert!(
        search_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "search should succeed"
    );
    let hits = search_text
        .get("result")
        .and_then(|v| v.get("hits"))
        .and_then(|v| v.as_array())
        .expect("result.hits");
    assert_eq!(hits.len(), 1, "expected exactly one hit");
    let snippet = hits[0]
        .get("message")
        .and_then(|v| v.get("snippet"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        snippet.contains("branchmind"),
        "snippet should contain the query"
    );

    let ref_path = hits[0]
        .get("ref")
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .expect("ref.path");
    let ref_line = hits[0]
        .get("ref")
        .and_then(|v| v.get("line"))
        .and_then(|v| v.as_u64())
        .expect("ref.line");

    let open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "transcripts_open", "arguments": {
            "workspace": "ws_transcripts",
            "root_dir": root_dir.to_string_lossy(),
            "ref": { "path": ref_path, "line": ref_line },
            "before_lines": 0,
            "after_lines": 0,
            "max_chars": 8000
        } }
    }));
    let open_text = extract_tool_text(&open);
    assert!(
        open_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open should succeed"
    );
    let entries = open_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert_eq!(entries.len(), 1, "expected one entry at the focus line");
    let role = entries[0]
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(role, "user");
    let text = entries[0]
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(text.contains("hello branchmind"));
}

#[test]
fn transcripts_open_suggests_capture_note_and_is_step_aware_when_focused() {
    let mut server = Server::start_initialized(
        "transcripts_open_suggests_capture_note_and_is_step_aware_when_focused",
    );

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_open_capture" } }
    }));

    let bootstrap = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "tasks_bootstrap",
            "arguments": {
                "workspace": "ws_transcripts_open_capture",
                "plan_title": "Plan",
                "task_title": "Task",
                "steps": [
                    { "title": "S1", "success_criteria": ["c1"], "tests": ["t1"], "blockers": ["b1"] }
                ]
            }
        }
    }));
    let bootstrap_text = extract_tool_text(&bootstrap);
    let task_id = bootstrap_text
        .get("result")
        .and_then(|v| v.get("task"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("task id")
        .to_string();

    let _focus = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks_focus_set", "arguments": {
            "workspace": "ws_transcripts_open_capture",
            "task": task_id
        } }
    }));

    let root_dir = make_temp_dir("root_open_capture");
    let file_path = root_dir.join("rollout-open-capture.jsonl");
    let content = r#"
{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-open","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}
{"timestamp":"2026-01-06T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello capture"}]}}
{"timestamp":"2026-01-06T00:00:02.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}}
"#;
    std::fs::write(&file_path, content.trim_start()).expect("write transcript file");

    let open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "transcripts_open", "arguments": {
            "workspace": "ws_transcripts_open_capture",
            "root_dir": root_dir.to_string_lossy(),
            "ref": { "path": "rollout-open-capture.jsonl", "line": 2 },
            "before_lines": 1,
            "after_lines": 1,
            "max_chars": 8000
        } }
    }));
    let open_text = extract_tool_text(&open);

    let suggestions = open_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        suggestions
            .iter()
            .any(|s| s.get("target").and_then(|v| v.as_str()) == Some("macro_branch_note")),
        "expected macro_branch_note capture suggestion"
    );

    let capture = suggestions
        .iter()
        .find(|s| s.get("target").and_then(|v| v.as_str()) == Some("macro_branch_note"))
        .expect("capture suggestion");
    let step = capture
        .get("params")
        .and_then(|v| v.get("meta"))
        .and_then(|v| v.get("step"))
        .and_then(|v| v.as_object())
        .expect("meta.step should exist when a task+step is focused");
    assert!(
        step.get("task_id")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v.starts_with("TASK-")),
        "meta.step.task_id should be a task id"
    );
    assert!(
        step.get("path")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()),
        "meta.step.path should be present"
    );
}

#[test]
fn transcripts_digest_summary_picks_summary_messages() {
    let mut server = Server::start_initialized("transcripts_digest_summary_picks_summary_messages");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_digest" } }
    }));

    let root_dir = make_temp_dir("root_digest");
    let file_path = root_dir.join("rollout-digest.jsonl");
    let content = r#"
{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-2","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}
{"timestamp":"2026-01-06T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"**ИТОГ:** сделал X"}]}}
"#;
    std::fs::write(&file_path, content.trim_start()).expect("write transcript file");

    let digest = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_digest",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": "",
            "mode": "summary",
            "max_items": 5,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );
    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(items.len(), 1);
    let text = items[0]
        .get("message")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(text.contains("ИТОГ"));
}

#[test]
fn transcripts_digest_default_max_items_is_low_noise() {
    let mut server = Server::start_initialized("transcripts_digest_default_max_items_is_low_noise");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_digest_defaults" } }
    }));

    let root_dir = make_temp_dir("root_digest_defaults");
    // Write many small session files; digest should still cap to default max_items.
    for idx in 0..10 {
        let file_path = root_dir.join(format!("rollout-digest-defaults-{idx:02}.jsonl"));
        let content = format!(
            r#"
{{"timestamp":"2026-01-06T00:00:{:02}.000Z","type":"session_meta","payload":{{"id":"sess-defaults-{:02}","timestamp":"2026-01-06T00:00:{:02}.000Z","cwd":"/tmp/project"}}}}
{{"timestamp":"2026-01-06T00:00:{:02}.500Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"**ИТОГ:** сделал {}"}}]}}}}
"#,
            idx,
            idx,
            idx,
            idx,
            idx
        );
        std::fs::write(&file_path, content.trim_start()).expect("write transcript file");
    }

    let digest = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_digest_defaults",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": "",
            "mode": "summary",
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );

    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(
        items.len(),
        6,
        "default max_items should keep digest low-noise"
    );
}

#[test]
fn transcripts_digest_tight_scan_budget_still_reaches_older_summary() {
    let mut server =
        Server::start_initialized("transcripts_digest_tight_scan_budget_still_reaches_older_summary");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_digest_budget_tail" } }
    }));

    let root_dir = make_temp_dir("root_digest_budget_tail");

    // Newer (lexicographically later) file first: large, no summary markers.
    let new_path = root_dir.join("z-new.jsonl");
    let mut new_lines = Vec::new();
    new_lines.push(
        r#"{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-new","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}"#
            .to_string(),
    );
    for idx in 0..900 {
        new_lines.push(format!(
            r#"{{"timestamp":"2026-01-06T00:00:{:02}.000Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"progress {} {} {}"}}]}}}}"#,
            idx % 60,
            idx,
            idx % 60,
            "x".repeat(220)
        ));
    }
    std::fs::write(&new_path, new_lines.join("\n")).expect("write new transcript file");

    // Older file: small, contains a summary marker.
    let old_path = root_dir.join("a-old.jsonl");
    let old_content = r#"
{"timestamp":"2026-01-05T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-old","timestamp":"2026-01-05T00:00:00.000Z","cwd":"/tmp/project"}}
{"timestamp":"2026-01-05T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"**ИТОГ:** found the anchor"}]}}
"#;
    std::fs::write(&old_path, old_content.trim_start()).expect("write old transcript file");

    // Under the old full-scan implementation, the large newest file would exhaust this budget
    // and prevent reaching the older file. The head+tail scan strategy should still find the summary.
    let digest = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_digest_budget_tail",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": "/tmp/project",
            "mode": "summary",
            "max_files": 10,
            "max_bytes_total": 150000,
            "max_items": 3,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );

    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(items.len(), 1, "expected one digest item");
    let text = items[0]
        .get("message")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(text.contains("ИТОГ"), "digest should include the summary marker");
    let ref_path = items[0]
        .get("ref")
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        ref_path.ends_with("a-old.jsonl"),
        "expected the older summary file to be selected"
    );
}

#[test]
fn transcripts_digest_deep_tail_finds_summary_when_file_end_is_noise() {
    let mut server =
        Server::start_initialized("transcripts_digest_deep_tail_finds_summary_when_file_end_is_noise");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_deep_tail" } }
    }));

    let root_dir = make_temp_dir("root_deep_tail");
    let file_path = root_dir.join("rollout-deep-tail.jsonl");

    // Create a file where the only summary message is pushed out of the last 128KiB by a large
    // tail of non-message noise lines. The digest tail scan should retry with a deeper window.
    let mut lines = Vec::new();
    lines.push(r#"{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-deep-tail","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}"#.to_string());
    lines.push(r#"{"timestamp":"2026-01-06T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"**ИТОГ:** deep tail works"}]}}"#.to_string());

    // Noise: not a message payload, repeated to exceed 128KiB.
    let noise = format!(
        r#"{{"timestamp":"2026-01-06T00:00:02.000Z","type":"tool_output","payload":{{"type":"tool_result","text":"{}"}}}}"#,
        "x".repeat(900)
    );
    for _ in 0..260 {
        lines.push(noise.clone());
    }
    std::fs::write(&file_path, lines.join("\n")).expect("write transcript file");

    let digest = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_deep_tail",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": "",
            "mode": "summary",
            "max_files": 5,
            "max_bytes_total": 4 * 1024 * 1024,
            "max_items": 3,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );

    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(items.len(), 1, "expected one digest item");
    let text = items[0]
        .get("message")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(text.contains("ИТОГ"), "expected summary marker");
}

#[test]
fn transcripts_digest_emits_byte_ref_for_huge_files_and_open_accepts_it() {
    let mut server = Server::start_initialized(
        "transcripts_digest_emits_byte_ref_for_huge_files_and_open_accepts_it",
    );

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_byte_ref" } }
    }));

    let root_dir = make_temp_dir("root_byte_ref");
    let file_path = root_dir.join("rollout-huge.jsonl");

    // Build a large file where the only summary message is near the end.
    // The digest must remain bounded under max_bytes_total and should not attempt to
    // resolve ref.line by scanning from file start.
    let mut lines = Vec::new();
    lines.push(r#"{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-huge","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}"#.to_string());
    for idx in 0..1600 {
        lines.push(format!(
            r#"{{"timestamp":"2026-01-06T00:00:{:02}.000Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"progress {} {}"}}]}}}}"#,
            idx % 60,
            idx,
            "x".repeat(420)
        ));
    }
    lines.push(r#"{"timestamp":"2026-01-06T00:59:59.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"**ИТОГ:** byte ref works"}]}}"#.to_string());
    std::fs::write(&file_path, lines.join("\n")).expect("write transcript file");

    let digest = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_byte_ref",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": "",
            "mode": "summary",
            "max_files": 5,
            "max_bytes_total": 180000,
            "max_items": 3,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );

    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(items.len(), 1, "expected one digest item");

    let ref_obj = items[0].get("ref").and_then(|v| v.as_object()).expect("ref");
    let byte = ref_obj.get("byte").and_then(|v| v.as_u64()).expect("ref.byte");
    assert!(byte > 0, "expected a non-zero byte ref for huge files");

    let open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "transcripts_open", "arguments": {
            "workspace": "ws_transcripts_byte_ref",
            "root_dir": root_dir.to_string_lossy(),
            "ref": { "path": "rollout-huge.jsonl", "byte": byte },
            "before_lines": 1,
            "after_lines": 1,
            "max_chars": 8000
        } }
    }));
    let open_text = extract_tool_text(&open);
    assert!(
        open_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open should succeed"
    );
    let entries = open_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("entries");
    assert!(
        entries.iter().any(|e| {
            e.get("text")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.contains("ИТОГ"))
        }),
        "expected opened window to include the summary message"
    );
}

#[test]
fn transcripts_digest_empty_under_scan_budget_emits_warning_and_retry_suggestion() {
    let mut server = Server::start_initialized(
        "transcripts_digest_empty_under_scan_budget_emits_warning_and_retry_suggestion",
    );

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_digest_budget" } }
    }));

    let root_dir = make_temp_dir("root_digest_budget");
    let file_path = root_dir.join("rollout-digest-budget.jsonl");
    let content = r#"
{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{"id":"sess-budget","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/tmp/project"}}
"#;
    std::fs::write(&file_path, content.trim_start()).expect("write transcript file");

    let digest = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_digest_budget",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": "",
            "mode": "summary",
            "max_files": 10,
            "max_bytes_total": 1,
            "max_items": 3,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );

    let warnings = digest_text
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings.iter().any(|w| w.get("code").and_then(|v| v.as_str()) == Some("TRANSCRIPTS_SCAN_TRUNCATED")),
        "expected TRANSCRIPTS_SCAN_TRUNCATED warning"
    );

    let suggestions = digest_text
        .get("suggestions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        suggestions.iter().any(|s| s.get("target").and_then(|v| v.as_str()) == Some("transcripts_digest")),
        "expected a retry suggestion"
    );
}

#[test]
fn transcripts_digest_matches_cwd_prefix_via_canonicalization() {
    let mut server =
        Server::start_initialized("transcripts_digest_matches_cwd_prefix_via_canonicalization");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_canon" } }
    }));

    let root_dir = make_temp_dir("root_canon");
    let project_dir = root_dir.join("project");
    std::fs::create_dir_all(&project_dir).expect("create project dir");

    let canonical_prefix = std::fs::canonicalize(&project_dir)
        .expect("canonicalize project dir")
        .to_string_lossy()
        .to_string();

    // Use a path with `..` segments that canonicalizes back to `project_dir`.
    let dotted = project_dir.join("..").join("project");
    let dotted_cwd = dotted.to_string_lossy().to_string();

    let file_path = root_dir.join("rollout-canon.jsonl");
    let content = format!(
        r#"
{{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{{"id":"sess-canon","timestamp":"2026-01-06T00:00:00.000Z","cwd":"{cwd}"}}}}
{{"timestamp":"2026-01-06T00:00:01.000Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"hello from canon test"}}]}}}}
"#,
        cwd = dotted_cwd
    );
    std::fs::write(&file_path, content.trim_start()).expect("write transcript file");

    let digest = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_canon",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": canonical_prefix,
            "mode": "last",
            "max_items": 5,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );
    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(items.len(), 1, "expected one digest item");
}

#[test]
fn transcripts_digest_matches_cwd_prefix_from_message_hints() {
    let mut server =
        Server::start_initialized("transcripts_digest_matches_cwd_prefix_from_message_hints");

    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_hints" } }
    }));

    let root_dir = make_temp_dir("root_hints");
    let project_dir = root_dir.join("project");
    std::fs::create_dir_all(&project_dir).expect("create project dir");

    let canonical_prefix = std::fs::canonicalize(&project_dir)
        .expect("canonicalize project dir")
        .to_string_lossy()
        .to_string();

    let file_path = root_dir.join("rollout-hints.jsonl");
    let content = format!(
        r#"
{{"timestamp":"2026-01-06T00:00:00.000Z","type":"session_meta","payload":{{"id":"sess-hints","timestamp":"2026-01-06T00:00:00.000Z","cwd":"/home/amir"}}}}
{{"timestamp":"2026-01-06T00:00:00.100Z","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"<environment_context>\\n  <cwd>{cwd}</cwd>\\n</environment_context>"}}]}}}}
{{"timestamp":"2026-01-06T00:00:01.000Z","type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"hello from message hint"}}]}}}}
"#,
        cwd = canonical_prefix
    );
    std::fs::write(&file_path, content.trim_start()).expect("write transcript file");

    let digest = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_digest", "arguments": {
            "workspace": "ws_transcripts_hints",
            "root_dir": root_dir.to_string_lossy(),
            "cwd_prefix": canonical_prefix,
            "mode": "last",
            "max_items": 5,
            "max_chars": 8000
        } }
    }));
    let digest_text = extract_tool_text(&digest);
    assert!(
        digest_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "digest should succeed"
    );
    let items = digest_text
        .get("result")
        .and_then(|v| v.get("digest"))
        .and_then(|v| v.as_array())
        .expect("digest");
    assert_eq!(items.len(), 1, "expected one digest item");
}

#[test]
fn transcripts_open_rejects_path_traversal() {
    let mut server = Server::start_initialized("transcripts_open_rejects_path_traversal");
    let _init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_transcripts_traversal" } }
    }));

    let root_dir = make_temp_dir("root_traversal");
    let open = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "transcripts_open", "arguments": {
            "workspace": "ws_transcripts_traversal",
            "root_dir": root_dir.to_string_lossy(),
            "ref": { "path": "../evil.jsonl", "line": 1 },
            "max_chars": 2000
        } }
    }));
    let open_text = extract_tool_text(&open);
    assert!(
        !open_text
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "expected invalid input error"
    );
}
