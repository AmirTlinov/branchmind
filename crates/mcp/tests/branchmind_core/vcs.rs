#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_vcs_smoke() {
    let mut server = Server::start_initialized("branchmind_vcs_smoke");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "system", "arguments": { "op": "call", "cmd": "system.init", "args": { "workspace": "ws_vcs" } } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let commit1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.commit", "args": { "workspace": "ws_vcs", "artifact": "artifact-1", "message": "m1" } } }
    }));
    let commit1_text = extract_tool_text(&commit1);
    let seq1 = commit1_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("commit seq1");

    let commit2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.commit", "args": { "workspace": "ws_vcs", "artifact": "artifact-2", "message": "m2" } } }
    }));
    let commit2_text = extract_tool_text(&commit2);
    let seq2 = commit2_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.get("seq"))
        .and_then(|v| v.as_i64())
        .expect("commit seq2");
    assert!(seq2 > seq1, "commit seq must advance");

    let log = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.log", "args": { "workspace": "ws_vcs", "limit": 10 } } }
    }));
    let log_text = extract_tool_text(&log);
    let commits = log_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .expect("commits");
    assert!(commits.len() >= 2, "log must include commits");

    let tag = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.tag.create", "args": { "workspace": "ws_vcs", "name": "v1", "from": seq1.to_string() } } }
    }));
    let tag_text = extract_tool_text(&tag);
    assert_eq!(
        tag_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let tags = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.tag.list", "args": { "workspace": "ws_vcs" } } }
    }));
    let tags_text = extract_tool_text(&tags);
    let tags_list = tags_text
        .get("result")
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_array())
        .expect("tags list");
    assert!(
        tags_list
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("v1")),
        "tag v1 must exist"
    );

    let reset = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.reset", "args": { "workspace": "ws_vcs", "ref": seq1.to_string() } } }
    }));
    let reset_text = extract_tool_text(&reset);
    assert_eq!(
        reset_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let log_after = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.log", "args": { "workspace": "ws_vcs", "limit": 10 } } }
    }));
    let log_after_text = extract_tool_text(&log_after);
    let commits_after = log_after_text
        .get("result")
        .and_then(|v| v.get("commits"))
        .and_then(|v| v.as_array())
        .expect("commits after reset");
    assert!(
        !commits_after
            .iter()
            .any(|c| c.get("seq").and_then(|v| v.as_i64()) == Some(seq2)),
        "reset must drop later commits from log view"
    );

    let reflog = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.reflog", "args": { "workspace": "ws_vcs", "limit": 10 } } }
    }));
    let reflog_text = extract_tool_text(&reflog);
    let reflog_entries = reflog_text
        .get("result")
        .and_then(|v| v.get("entries"))
        .and_then(|v| v.as_array())
        .expect("reflog entries");
    assert!(!reflog_entries.is_empty(), "reflog must have entries");

    let docs = server.request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": { "name": "docs", "arguments": { "op": "call", "cmd": "docs.list", "args": { "workspace": "ws_vcs" } } }
    }));
    let docs_text = extract_tool_text(&docs);
    let docs_list = docs_text
        .get("result")
        .and_then(|v| v.get("docs"))
        .and_then(|v| v.as_array())
        .expect("docs list");
    assert!(
        docs_list
            .iter()
            .any(|d| d.get("doc").and_then(|v| v.as_str()) == Some("notes")),
        "notes doc must exist"
    );

    let branch = server.request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.create", "args": { "workspace": "ws_vcs", "name": "topic" } } }
    }));
    let branch_text = extract_tool_text(&branch);
    assert_eq!(
        branch_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let rename = server.request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.rename", "args": { "workspace": "ws_vcs", "old": "topic", "new": "topic-renamed" } } }
    }));
    let rename_text = extract_tool_text(&rename);
    assert_eq!(
        rename_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let delete = server.request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "vcs", "arguments": { "op": "call", "cmd": "vcs.branch.delete", "args": { "workspace": "ws_vcs", "name": "topic-renamed" } } }
    }));
    let delete_text = extract_tool_text(&delete);
    let deleted = delete_text
        .get("result")
        .and_then(|v| v.get("deleted"))
        .and_then(|v| v.as_bool())
        .expect("delete result");
    assert!(deleted, "branch must be deleted");
}
