#![forbid(unsafe_code)]

mod support;

use serde_json::json;
use support::*;

#[test]
fn meaning_map_hud_prompts_when_unknown_and_disappears_after_attach() {
    let mut server = Server::start_initialized_with_args(
        "meaning_map_hud_prompts_when_unknown_and_disappears_after_attach",
        &["--toolset", "daily", "--workspace", "ws_meaning_map"],
    );

    let started = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.macro.start", "args": { "task_title": "Storage: map HUD test" } } }
    }));
    assert!(
        !extract_tool_text_str(&started).starts_with("ERROR:"),
        "macro_start must succeed"
    );

    let snap1 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "fmt": "lines" } } }
    }));
    let snap1_text = extract_tool_text_str(&snap1);
    let snap1_state = snap1_text.lines().next().unwrap_or("");
    assert!(
        snap1_state.contains("pack=mindpack@"),
        "snapshot should surface pack=mindpack@<seq> in the state line for resume-by-meaning, got:\n{snap1_text}"
    );
    assert!(
        snap1_state.contains("where=a:storage"),
        "snapshot state line should include fallback where=a:storage (derived from title), got:\n{snap1_text}"
    );
    let snap1_lines = snap1_text.lines().collect::<Vec<_>>();
    assert!(
        snap1_lines
            .iter()
            .any(|l| l.starts_with("think ") && l.contains("cmd=think.card")),
        "when anchor is missing, snapshot should prompt a think.card command via the think portal, got:\n{snap1_text}"
    );
    let think_line = snap1_lines
        .iter()
        .find(|l| l.starts_with("think ") && l.contains("cmd=think.card"))
        .copied()
        .unwrap_or("");
    assert!(
        think_line.contains("v:canon"),
        "map attach suggestion should be canonical (v:canon) so open(id=a:...) is not empty, got:\n{think_line}"
    );

    let attached = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think", "arguments": { "op": "call", "cmd": "think.card", "args": {
            "step": "focus",
            "card": {
                "id": "CARD-ANCHOR-STORAGE",
                "type": "note",
                "title": "Anchor attach: Storage",
                "text": "Anchor attach note for Storage.",
                "tags": ["a:storage", "v:canon"]
            }
        } } }
    }));
    let attached_payload = extract_tool_text(&attached);
    assert!(
        attached_payload
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "think_card must succeed, got:\n{}",
        extract_tool_text_str(&attached)
    );

    let snap2 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "fmt": "lines" } } }
    }));
    let snap2_text = extract_tool_text_str(&snap2);
    let first_line = snap2_text.lines().next().unwrap_or("");
    let pack2 = first_line
        .split_whitespace()
        .find_map(|t| t.strip_prefix("pack="))
        .unwrap_or("");
    assert!(
        first_line.contains("where=a:storage"),
        "after attach, snapshot state line should include where=a:storage, got:\n{snap2_text}"
    );
    assert!(
        first_line.contains("pack=mindpack@"),
        "after attach, snapshot should still surface pack=mindpack@<seq>, got:\n{snap2_text}"
    );
    assert!(
        pack2.starts_with("mindpack@"),
        "snapshot state line should include a usable pack ref, got: {first_line}"
    );

    // Dedupe invariant: the mindpack should not append a new entry on every snapshot when the
    // underlying state is unchanged. This protects long-running sessions from mindpack spam.
    let snap3 = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "tasks", "arguments": { "op": "call", "cmd": "tasks.snapshot", "args": { "fmt": "lines" } } }
    }));
    let snap3_text = extract_tool_text_str(&snap3);
    let third_line = snap3_text.lines().next().unwrap_or("");
    let pack3 = third_line
        .split_whitespace()
        .find_map(|t| t.strip_prefix("pack="))
        .unwrap_or("");
    assert_eq!(
        pack3, pack2,
        "when nothing changed, snapshot should keep the same mindpack@seq (dedupe), got:\n{snap2_text}\n---\n{snap3_text}"
    );
    // Flagship anti-noise: after anchoring, switch from canonical attach prompts to a single
    // step-scoped skeptic preflight hint embedded in the state line.
    let state2 = snap2_text.lines().next().unwrap_or("");
    assert!(
        state2.contains("| backup "),
        "after attach, snapshot should include a backup hint in the state line, got:\n{snap2_text}"
    );
    assert!(
        state2.contains("skeptic:preflight") && state2.contains("v:draft"),
        "after attach, backup hint should be a skeptic preflight draft (step-scoped), got:\n{state2}"
    );
    assert!(
        !state2.contains("v:canon"),
        "after attach, snapshot should stop prompting canonical anchor-attach commands, got:\n{state2}"
    );

    let open_anchor = server.request(json!( {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "open", "arguments": { "id": "a:storage" } }
    }));
    let open_anchor = extract_tool_text(&open_anchor);
    assert!(
        open_anchor
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "open(anchor) must succeed"
    );
    let cards = open_anchor
        .get("result")
        .and_then(|v| v.get("cards"))
        .and_then(|v| v.as_array())
        .expect("open(anchor).result.cards");
    assert!(
        cards
            .iter()
            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("CARD-ANCHOR-STORAGE")),
        "open(anchor) should include the canonical anchor registry note"
    );
}
