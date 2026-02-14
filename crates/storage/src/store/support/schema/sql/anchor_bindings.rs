#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS anchor_bindings (
          workspace TEXT NOT NULL,
          anchor_id TEXT NOT NULL,
          kind TEXT NOT NULL,
          repo_rel TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, anchor_id, kind, repo_rel)
        );
"#;
