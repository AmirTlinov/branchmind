#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS ops_history (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          workspace TEXT NOT NULL,
          task_id TEXT,
          path TEXT,
          intent TEXT NOT NULL,
          payload_json TEXT NOT NULL,
          before_json TEXT,
          after_json TEXT,
          undoable INTEGER NOT NULL DEFAULT 1,
          undone INTEGER NOT NULL DEFAULT 0,
          ts_ms INTEGER NOT NULL
        );
"#;
