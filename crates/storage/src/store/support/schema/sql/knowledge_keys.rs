#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        -- Knowledge key index (AI-first durable memory).
        --
        -- Purpose:
        -- - Provide a stable identity for evolving knowledge cards via (anchor_id, key).
        --   `card_id` is the current/latest version pointer (history stays in the graph).
        -- - Provide fast recall ordering via updated_at_ms (recency-first).
        --
        -- Notes:
        -- - This table is scoped per workspace.
        -- - It is intentionally small and query-friendly (no JSON).
        CREATE TABLE IF NOT EXISTS knowledge_keys (
          workspace TEXT NOT NULL,
          anchor_id TEXT NOT NULL,
          key TEXT NOT NULL,
          card_id TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, anchor_id, key)
        );
"#;
