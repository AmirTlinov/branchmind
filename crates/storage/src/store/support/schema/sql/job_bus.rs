#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS job_bus_messages (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          workspace TEXT NOT NULL,
          ts_ms INTEGER NOT NULL,
          thread_id TEXT NOT NULL,
          from_agent_id TEXT NOT NULL,
          from_job_id TEXT,
          to_agent_id TEXT,
          kind TEXT NOT NULL,
          summary TEXT NOT NULL,
          refs_json TEXT,
          payload_json TEXT,
          idempotency_key TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS job_bus_offsets (
          workspace TEXT NOT NULL,
          consumer_id TEXT NOT NULL,
          thread_id TEXT NOT NULL,
          after_seq INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, consumer_id, thread_id)
        );
"#;
