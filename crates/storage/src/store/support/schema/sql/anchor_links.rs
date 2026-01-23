#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS anchor_links (
          workspace TEXT NOT NULL,
          anchor_id TEXT NOT NULL,
          branch TEXT NOT NULL,
          graph_doc TEXT NOT NULL,
          card_id TEXT NOT NULL,
          card_type TEXT NOT NULL,
          last_ts_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, anchor_id, branch, graph_doc, card_id)
        );
"#;
