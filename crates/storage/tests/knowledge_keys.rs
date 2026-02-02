#![forbid(unsafe_code)]

use bm_core::ids::WorkspaceId;
use bm_storage::{
    KnowledgeKeysListAnyRequest, SqliteStore, ThinkCardCommitRequest, ThinkCardInput,
};
use std::path::PathBuf;

fn temp_dir(test_name: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_storage_{test_name}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn knowledge_card(card_id: &str, title: &str, text: &str, tags: Vec<String>) -> ThinkCardInput {
    let payload_json = format!(
        "{{\"id\":\"{id}\",\"type\":\"knowledge\",\"title\":\"{title}\",\"text\":\"{text}\",\"status\":\"open\",\"tags\":[{tags}]}}",
        id = card_id,
        title = title,
        text = text,
        tags = tags
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    ThinkCardInput {
        card_id: card_id.to_string(),
        card_type: "knowledge".to_string(),
        title: Some(title.to_string()),
        text: Some(text.to_string()),
        status: Some("open".to_string()),
        tags,
        meta_json: None,
        content: text.to_string(),
        payload_json,
    }
}

#[test]
fn knowledge_keys_index_is_updated_and_supports_evolution() {
    let storage_dir = temp_dir("knowledge_keys_index_is_updated_and_supports_evolution");
    let mut store = SqliteStore::open(&storage_dir).expect("open store");
    let workspace = WorkspaceId::try_new("ws_knowledge_keys").expect("workspace id");
    store.workspace_init(&workspace).expect("init workspace");

    let out = store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: "main".to_string(),
                trace_doc: "kb-trace".to_string(),
                graph_doc: "kb-graph".to_string(),
                card: knowledge_card(
                    "CARD-KN-1",
                    "Invariant",
                    "Must be deterministic.",
                    vec!["a:core".to_string(), "k:determinism".to_string()],
                ),
                supports: vec![],
                blocks: vec![],
            },
        )
        .expect("commit knowledge");
    assert!(out.inserted);
    assert_eq!(out.nodes_upserted, 1);

    let list = store
        .knowledge_keys_list_any(
            &workspace,
            KnowledgeKeysListAnyRequest {
                anchor_ids: vec!["a:core".to_string()],
                limit: 10,
            },
        )
        .expect("list knowledge keys");
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].anchor_id, "a:core");
    assert_eq!(list.items[0].key, "determinism");
    assert_eq!(list.items[0].card_id, "CARD-KN-1");
    let created_at_ms = list.items[0].created_at_ms;
    let updated_at_ms = list.items[0].updated_at_ms;

    // Evolve: same (anchor,key) should update the "latest" pointer to a new card_id
    // while preserving created_at_ms.
    std::thread::sleep(std::time::Duration::from_millis(10));

    store
        .think_card_commit(
            &workspace,
            ThinkCardCommitRequest {
                branch: "main".to_string(),
                trace_doc: "kb-trace".to_string(),
                graph_doc: "kb-graph".to_string(),
                card: knowledge_card(
                    "CARD-KN-OTHER",
                    "Duplicate",
                    "Different id, same key.",
                    vec!["a:core".to_string(), "k:determinism".to_string()],
                ),
                supports: vec![],
                blocks: vec![],
            },
        )
        .expect("second commit");

    let list_2 = store
        .knowledge_keys_list_any(
            &workspace,
            KnowledgeKeysListAnyRequest {
                anchor_ids: vec!["a:core".to_string()],
                limit: 10,
            },
        )
        .expect("list knowledge keys");
    assert_eq!(list_2.items.len(), 1);
    assert_eq!(list_2.items[0].card_id, "CARD-KN-OTHER");
    assert_eq!(
        list_2.items[0].created_at_ms, created_at_ms,
        "created_at_ms must be stable for a key"
    );
    assert!(
        list_2.items[0].updated_at_ms > updated_at_ms,
        "updated_at_ms must move forward when key is updated"
    );
}
