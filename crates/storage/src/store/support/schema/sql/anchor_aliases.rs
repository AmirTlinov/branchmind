#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS anchor_aliases (
          workspace TEXT NOT NULL,
          alias_id TEXT NOT NULL,
          anchor_id TEXT NOT NULL,
          PRIMARY KEY (workspace, alias_id)
        );
"#;
