#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        -- Runner liveness leases (delegation).
        --
        -- Deterministic, no-heuristics runner status is represented as an explicit lease:
        -- a runner periodically renews its lease; if it expires, it is offline.
        --
        -- The server never tries to infer runner liveness from job events.
        CREATE TABLE IF NOT EXISTS runner_leases (
          workspace TEXT NOT NULL,
          runner_id TEXT NOT NULL,
          status TEXT NOT NULL,              -- idle|live (runner-owned)
          active_job_id TEXT,                -- optional JOB-*
          lease_expires_at_ms INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          meta_json TEXT,
          PRIMARY KEY (workspace, runner_id)
        );

        CREATE INDEX IF NOT EXISTS runner_leases_by_workspace_expires
          ON runner_leases(workspace, lease_expires_at_ms);
"#;
