#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::json;

fn call_markdown_tool(
    server: &mut Server,
    id: i64,
    name: &str,
    workspace: &str,
    markdown: &str,
) -> serde_json::Value {
    let resp = server.request(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": {
                "workspace": workspace,
                "markdown": markdown
            }
        }
    }));
    extract_tool_text(&resp)
}

#[test]
fn mcp_auto_init_allows_tools_list_without_notifications() {
    let mut server = Server::start("auto_init_tools_list_v3");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }
    }));
    assert!(
        init.get("result").is_some(),
        "initialize must return result"
    );

    let tools_list = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    let tools = tools_list
        .get("result")
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.as_array())
        .expect("result.tools");

    let mut names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec!["branch", "merge", "think"]);
}

#[test]
fn parser_accepts_strict_single_bm_fence() {
    let mut server = Server::start_initialized("parser_accepts_single_bm_fence");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist limit=3\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn parser_rejects_non_fenced_markdown() {
    let mut server = Server::start_initialized("parser_rejects_non_fenced_markdown");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "commit branch=main commit=c1 message=hello body=world"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}

#[test]
fn parser_rejects_text_outside_bm_fence() {
    let mut server = Server::start_initialized("parser_rejects_text_outside_bm_fence");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "preface\n```bm\nlist\n```\ntrailer"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}

#[test]
fn parser_rejects_unknown_verb_for_tool() {
    let mut server = Server::start_initialized("parser_rejects_unknown_verb_for_tool");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": {
            "name": "merge",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_VERB")
    );
}

#[test]
fn parser_rejects_unknown_top_level_arg() {
    let mut server = Server::start_initialized("parser_rejects_unknown_top_level_arg");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist\n```",
                "unexpected": true
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ARG")
    );
}

#[test]
fn parser_rejects_max_chars_top_level_arg() {
    let mut server = Server::start_initialized("parser_rejects_max_chars_top_level_arg");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 140,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist\n```",
                "max_chars": 1
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ARG")
    );
}

#[test]
fn parser_rejects_duplicate_command_args() {
    let mut server = Server::start_initialized("parser_rejects_duplicate_command_args");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": {
            "name": "think",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlog branch=main branch=other\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}

#[test]
fn parser_rejects_multiple_bm_blocks() {
    let mut server = Server::start_initialized("parser_rejects_multiple_bm_blocks");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist\n```\n```bm\nlist\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}

#[test]
fn parser_rejects_unknown_command_arg() {
    let mut server = Server::start_initialized("parser_rejects_unknown_command_arg");

    let call = server.request(json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": {
            "name": "branch",
            "arguments": {
                "workspace": "ws-parser",
                "markdown": "```bm\nlist limit=5 nonsense=1\n```"
            }
        }
    }));
    let payload = extract_tool_text(&call);
    assert_eq!(
        payload
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNKNOWN_ARG")
    );
}

#[test]
fn think_commit_without_body_uses_message_as_deterministic_fallback() {
    let mut server = Server::start_initialized("think_commit_without_body_fallback");
    let workspace = "ws-commit-fallback";

    let main = call_markdown_tool(&mut server, 190, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    let commit = call_markdown_tool(
        &mut server,
        191,
        "think",
        workspace,
        "```bm\ncommit branch=main commit=c1 message=hello-world\n```",
    );
    assert_eq!(
        commit.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "commit without explicit body must succeed: {commit}"
    );
    assert_eq!(
        commit
            .get("result")
            .and_then(|v| v.get("commit"))
            .and_then(|v| v.get("body"))
            .and_then(|v| v.as_str()),
        Some("hello-world")
    );

    let show = call_markdown_tool(
        &mut server,
        192,
        "think",
        workspace,
        "```bm\nshow commit=c1\n```",
    );
    assert_eq!(show.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        show.get("result")
            .and_then(|v| v.get("commit"))
            .and_then(|v| v.get("body"))
            .and_then(|v| v.as_str()),
        Some("hello-world"),
        "persisted body must stay non-empty and deterministic"
    );
}

#[test]
fn think_commit_accepts_fenced_body_without_body_arg() {
    let mut server = Server::start_initialized("think_commit_fenced_body_without_arg");
    let workspace = "ws-commit-fenced-body";

    let main = call_markdown_tool(&mut server, 193, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    let commit = call_markdown_tool(
        &mut server,
        194,
        "think",
        workspace,
        "```bm\ncommit branch=main commit=c2 message=hello\nline one\nline two\n```",
    );
    assert_eq!(
        commit.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "commit with fenced body must succeed without body arg: {commit}"
    );
    assert_eq!(
        commit
            .get("result")
            .and_then(|v| v.get("commit"))
            .and_then(|v| v.get("body"))
            .and_then(|v| v.as_str()),
        Some("line one\nline two")
    );
}

#[test]
fn think_log_pagination_cursor_points_to_first_omitted_commit() {
    let mut server = Server::start_initialized("think_log_pagination_cursor");
    let workspace = "ws-pagination";

    let main = call_markdown_tool(&mut server, 20, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    for (id, commit_id) in ["c1", "c2", "c3"].into_iter().enumerate() {
        let payload = call_markdown_tool(
            &mut server,
            30 + id as i64,
            "think",
            workspace,
            &format!(
                "```bm\ncommit branch=main commit={commit_id} message={commit_id} body={commit_id}\n```"
            ),
        );
        assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    }

    let page1 = call_markdown_tool(
        &mut server,
        40,
        "think",
        workspace,
        "```bm\nlog branch=main limit=1\n```",
    );
    assert_eq!(page1.get("success").and_then(|v| v.as_bool()), Some(true));
    let page1_items = page1
        .get("result")
        .and_then(|v| v.get("items"))
        .and_then(|v| v.as_array())
        .expect("result.items");
    assert_eq!(page1_items.len(), 1);
    assert_eq!(
        page1_items[0]
            .get("commit_id")
            .and_then(|v| v.as_str())
            .expect("commit_id"),
        "c3"
    );
    let next_commit_id = page1
        .get("result")
        .and_then(|v| v.get("next_commit_id"))
        .and_then(|v| v.as_str())
        .expect("next_commit_id");
    assert_eq!(next_commit_id, "c2");

    let page2 = call_markdown_tool(
        &mut server,
        41,
        "think",
        workspace,
        &format!("```bm\nlog branch=main from={next_commit_id} limit=1\n```"),
    );
    assert_eq!(page2.get("success").and_then(|v| v.as_bool()), Some(true));
    let page2_first = page2
        .get("result")
        .and_then(|v| v.get("items"))
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .expect("result.items[0]");
    assert_eq!(
        page2_first
            .get("commit_id")
            .and_then(|v| v.as_str())
            .expect("commit_id"),
        "c2"
    );
}

#[test]
fn think_log_limit_zero_returns_empty_page_and_head_cursor() {
    let mut server = Server::start_initialized("think_log_limit_zero_cursor");
    let workspace = "ws-limit-zero";

    let main = call_markdown_tool(&mut server, 50, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    for (id, commit_id) in ["c1", "c2"].into_iter().enumerate() {
        let payload = call_markdown_tool(
            &mut server,
            60 + id as i64,
            "think",
            workspace,
            &format!(
                "```bm\ncommit branch=main commit={commit_id} message={commit_id} body={commit_id}\n```"
            ),
        );
        assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    }

    let page = call_markdown_tool(
        &mut server,
        70,
        "think",
        workspace,
        "```bm\nlog branch=main limit=0\n```",
    );
    assert_eq!(page.get("success").and_then(|v| v.as_bool()), Some(true));
    let items = page
        .get("result")
        .and_then(|v| v.get("items"))
        .and_then(|v| v.as_array())
        .expect("result.items");
    assert!(items.is_empty(), "limit=0 must return no items");
    let next_commit_id = page
        .get("result")
        .and_then(|v| v.get("next_commit_id"))
        .and_then(|v| v.as_str())
        .expect("next_commit_id");
    assert_eq!(next_commit_id, "c2");
}

#[test]
fn merge_into_long_branch_ids_keeps_unique_ids_for_each_source() {
    let mut server = Server::start_initialized("merge_long_branch_ids_unique");
    let workspace = "ws-merge-long";

    let main = call_markdown_tool(&mut server, 80, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    let target = format!("t{}", "x".repeat(109));
    let source_a = format!("s{}a", "q".repeat(107));
    let source_b = format!("s{}b", "q".repeat(107));

    for (id, branch) in [
        (81, target.as_str()),
        (82, source_a.as_str()),
        (83, source_b.as_str()),
    ] {
        let create = call_markdown_tool(
            &mut server,
            id,
            "branch",
            workspace,
            &format!("```bm\ncreate branch={branch} from=main\n```"),
        );
        assert_eq!(
            create.get("success").and_then(|v| v.as_bool()),
            Some(true),
            "branch create should succeed for {branch}: {create}"
        );
    }

    let merge = call_markdown_tool(
        &mut server,
        84,
        "merge",
        workspace,
        &format!("```bm\ninto target={target} from={source_a},{source_b} strategy=squash\n```"),
    );
    assert_eq!(
        merge.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "merge should succeed: {merge}"
    );

    let merged = merge
        .get("result")
        .and_then(|v| v.get("merged"))
        .and_then(|v| v.as_array())
        .expect("result.merged");
    assert_eq!(merged.len(), 2, "both sources should merge");

    let merge_ids = merged
        .iter()
        .filter_map(|item| item.get("merge_id").and_then(|v| v.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    let synth_ids = merged
        .iter()
        .filter_map(|item| item.get("synthesis_commit_id").and_then(|v| v.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(merge_ids.len(), 2, "merge ids must be unique");
    assert_eq!(synth_ids.len(), 2, "synthesis commit ids must be unique");
}

#[test]
fn merge_total_failure_returns_diagnostic_warnings() {
    let mut server = Server::start_initialized("merge_total_failure_returns_warnings");
    let workspace = "ws-merge-fail";

    let main = call_markdown_tool(&mut server, 90, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    let merge = call_markdown_tool(
        &mut server,
        91,
        "merge",
        workspace,
        "```bm\ninto target=main from=missing_a,missing_b strategy=squash\n```",
    );
    assert_eq!(merge.get("success").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        merge
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("MERGE_FAILED")
    );

    let warnings = merge
        .get("warnings")
        .and_then(|v| v.as_array())
        .expect("warnings array");
    assert_eq!(warnings.len(), 2, "all source failures must be returned");
    assert!(
        warnings
            .iter()
            .all(|w| { w.get("code").and_then(|v| v.as_str()) == Some("MERGE_SOURCE_FAILED") }),
        "warnings must include structured source failure entries"
    );

    let failures = merge
        .get("result")
        .and_then(|v| v.get("failures"))
        .and_then(|v| v.as_array())
        .expect("result.failures");
    assert_eq!(failures.len(), 2, "result.failures must mirror warnings");
}

#[test]
fn branch_create_accepts_parent_alias_and_rejects_conflict_with_from() {
    let mut server = Server::start_initialized("branch_create_parent_alias");
    let workspace = "ws-branch-parent";

    let main = call_markdown_tool(&mut server, 100, "branch", workspace, "```bm\nmain\n```");
    assert_eq!(main.get("success").and_then(|v| v.as_bool()), Some(true));

    let create_with_parent = call_markdown_tool(
        &mut server,
        101,
        "branch",
        workspace,
        "```bm\ncreate branch=child parent=main\n```",
    );
    assert_eq!(
        create_with_parent.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "parent alias should be accepted: {create_with_parent}"
    );
    assert_eq!(
        create_with_parent
            .get("result")
            .and_then(|v| v.get("branch"))
            .and_then(|v| v.get("parent_branch_id"))
            .and_then(|v| v.as_str()),
        Some("main")
    );

    let create_conflict = call_markdown_tool(
        &mut server,
        102,
        "branch",
        workspace,
        "```bm\ncreate branch=child2 from=main parent=main\n```",
    );
    assert_eq!(
        create_conflict.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        create_conflict
            .get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("INVALID_INPUT")
    );
}
