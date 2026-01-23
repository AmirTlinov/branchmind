#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS jobs (
          workspace TEXT NOT NULL,
          id TEXT NOT NULL,
          revision INTEGER NOT NULL,
          status TEXT NOT NULL,
          title TEXT NOT NULL,
          kind TEXT NOT NULL,
          priority TEXT NOT NULL DEFAULT 'MEDIUM',
          task_id TEXT,
          anchor_id TEXT,
          runner TEXT,
          claim_expires_at_ms INTEGER,
          prompt TEXT,
          summary TEXT,
          meta_json TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          completed_at_ms INTEGER,
          PRIMARY KEY (workspace, id)
        );

        CREATE TABLE IF NOT EXISTS job_events (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          workspace TEXT NOT NULL,
          job_id TEXT NOT NULL,
          ts_ms INTEGER NOT NULL,
          kind TEXT NOT NULL,
          message TEXT NOT NULL,
          percent INTEGER,
          refs_json TEXT,
          meta_json TEXT
        );
"#;
