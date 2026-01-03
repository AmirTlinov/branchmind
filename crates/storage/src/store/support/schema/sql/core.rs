#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workspaces (
          workspace TEXT PRIMARY KEY,
          created_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS counters (
          workspace TEXT NOT NULL,
          name TEXT NOT NULL,
          value INTEGER NOT NULL,
          PRIMARY KEY (workspace, name)
        );
"#;
