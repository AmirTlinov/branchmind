#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS anchors (
          workspace TEXT NOT NULL,
          id TEXT NOT NULL,
          title TEXT NOT NULL,
          kind TEXT NOT NULL,
          description TEXT,
          refs_json TEXT,
          parent_id TEXT,
          depends_on_json TEXT,
          status TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, id)
        );
"#;
