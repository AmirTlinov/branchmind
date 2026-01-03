#![forbid(unsafe_code)]

use super::support::*;

use serde_json::json;

#[test]
fn branchmind_think_wrappers_add_manage_smoke() {
    let mut server = Server::start_initialized("branchmind_think_wrappers_add_manage_smoke");

    let init = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": "init", "arguments": { "workspace": "ws_think_wrap" } }
    }));
    let init_text = extract_tool_text(&init);
    assert_eq!(
        init_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let note = server.request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": { "name": "think_add_note", "arguments": { "workspace": "ws_think_wrap", "card": "Quick note" } }
    }));
    let note_text = extract_tool_text(&note);
    assert_eq!(
        note_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let decision = server.request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": { "name": "think_add_decision", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Decision", "text": "Proceed" } } }
    }));
    let decision_text = extract_tool_text(&decision);
    assert_eq!(
        decision_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let evidence = server.request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": { "name": "think_add_evidence", "arguments": { "workspace": "ws_think_wrap", "card": "Evidence collected" } }
    }));
    let evidence_text = extract_tool_text(&evidence);
    assert_eq!(
        evidence_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let frame = server.request(json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": { "name": "think_add_frame", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Frame", "text": "Context" } } }
    }));
    let frame_text = extract_tool_text(&frame);
    assert_eq!(
        frame_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let update = server.request(json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": { "name": "think_add_update", "arguments": { "workspace": "ws_think_wrap", "card": "Progress update" } }
    }));
    let update_text = extract_tool_text(&update);
    assert_eq!(
        update_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let hypo1 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "think_add_hypothesis", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Same" } } }
    }));
    let hypo1_text = extract_tool_text(&hypo1);
    let hypo1_id = hypo1_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo1 id")
        .to_string();

    let hypo2 = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "think_add_hypothesis", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Hypo", "text": "Same" } } }
    }));
    let hypo2_text = extract_tool_text(&hypo2);
    let _hypo2_id = hypo2_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("hypo2 id")
        .to_string();

    let question = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": { "name": "think_add_question", "arguments": { "workspace": "ws_think_wrap", "card": { "title": "Question", "text": "Why?" } } }
    }));
    let question_text = extract_tool_text(&question);
    let question_id = question_text
        .get("result")
        .and_then(|v| v.get("card_id"))
        .and_then(|v| v.as_str())
        .expect("question id")
        .to_string();

    let test_card = server.request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": { "name": "think_add_test", "arguments": { "workspace": "ws_think_wrap", "card": "Test it" } }
    }));
    let test_text = extract_tool_text(&test_card);
    assert_eq!(
        test_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let link = server.request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "think_link", "arguments": { "workspace": "ws_think_wrap", "from": question_id.clone(), "rel": "supports", "to": hypo1_id.clone() } }
    }));
    let link_text = extract_tool_text(&link);
    assert_eq!(
        link_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let set_status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": { "name": "think_set_status", "arguments": { "workspace": "ws_think_wrap", "status": "blocked", "targets": [hypo1_id.clone()] } }
    }));
    let set_status_text = extract_tool_text(&set_status);
    assert_eq!(
        set_status_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pin = server.request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": { "name": "think_pin", "arguments": { "workspace": "ws_think_wrap", "targets": [hypo1_id.clone()] } }
    }));
    let pin_text = extract_tool_text(&pin);
    assert_eq!(
        pin_text.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let pins = server.request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "think_pins", "arguments": { "workspace": "ws_think_wrap", "limit": 10 } }
    }));
    let pins_text = extract_tool_text(&pins);
    let pins_list = pins_text
        .get("result")
        .and_then(|v| v.get("pins"))
        .and_then(|v| v.as_array())
        .expect("pins");
    assert!(
        pins_list
            .iter()
            .any(|p| p.get("id").and_then(|v| v.as_str()) == Some(hypo1_id.as_str())),
        "pinned card must be listed"
    );
}
