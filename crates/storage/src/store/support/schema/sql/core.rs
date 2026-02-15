#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workspaces (
          workspace TEXT PRIMARY KEY,
          created_at_ms INTEGER NOT NULL,
          project_guard TEXT
        );

        -- Path → workspace bindings (DX: allow workspace to be selected by filesystem paths).
        -- `path` should be a canonical absolute directory path.
        CREATE TABLE IF NOT EXISTS workspace_paths (
          path TEXT PRIMARY KEY,
          workspace TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          last_used_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS counters (
          workspace TEXT NOT NULL,
          name TEXT NOT NULL,
          value INTEGER NOT NULL,
          PRIMARY KEY (workspace, name)
        );
"#;
