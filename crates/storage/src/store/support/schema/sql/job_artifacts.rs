#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS job_artifacts (
          workspace TEXT NOT NULL,
          job_id TEXT NOT NULL,
          artifact_key TEXT NOT NULL,
          content_text TEXT NOT NULL,
          content_len INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, job_id, artifact_key)
        );
"#;
