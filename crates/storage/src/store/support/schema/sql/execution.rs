#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS steps (
          workspace TEXT NOT NULL,
          task_id TEXT NOT NULL,
          step_id TEXT NOT NULL,
          parent_step_id TEXT,
          ordinal INTEGER NOT NULL,
          title TEXT NOT NULL,
          completed INTEGER NOT NULL,
          completed_at_ms INTEGER,
          started_at_ms INTEGER,
          criteria_confirmed INTEGER NOT NULL,
          tests_confirmed INTEGER NOT NULL,
          criteria_auto_confirmed INTEGER NOT NULL DEFAULT 0,
          tests_auto_confirmed INTEGER NOT NULL DEFAULT 1,
          security_confirmed INTEGER NOT NULL DEFAULT 0,
          perf_confirmed INTEGER NOT NULL DEFAULT 0,
          docs_confirmed INTEGER NOT NULL DEFAULT 0,
          proof_tests_mode INTEGER NOT NULL DEFAULT 0,
          proof_security_mode INTEGER NOT NULL DEFAULT 0,
          proof_perf_mode INTEGER NOT NULL DEFAULT 0,
          proof_docs_mode INTEGER NOT NULL DEFAULT 0,
          blocked INTEGER NOT NULL DEFAULT 0,
          block_reason TEXT,
          verification_outcome TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, step_id)
        );

        CREATE TABLE IF NOT EXISTS step_criteria (
          workspace TEXT NOT NULL,
          step_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          text TEXT NOT NULL,
          PRIMARY KEY (workspace, step_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS step_tests (
          workspace TEXT NOT NULL,
          step_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          text TEXT NOT NULL,
          PRIMARY KEY (workspace, step_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS step_blockers (
          workspace TEXT NOT NULL,
          step_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          text TEXT NOT NULL,
          PRIMARY KEY (workspace, step_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS step_notes (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          workspace TEXT NOT NULL,
          task_id TEXT NOT NULL,
          step_id TEXT NOT NULL,
          ts_ms INTEGER NOT NULL,
          note TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS plan_checklist (
          workspace TEXT NOT NULL,
          plan_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          text TEXT NOT NULL,
          PRIMARY KEY (workspace, plan_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS task_items (
          workspace TEXT NOT NULL,
          entity_kind TEXT NOT NULL,
          entity_id TEXT NOT NULL,
          field TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          text TEXT NOT NULL,
          PRIMARY KEY (workspace, entity_kind, entity_id, field, ordinal)
        );

        CREATE TABLE IF NOT EXISTS task_nodes (
          workspace TEXT NOT NULL,
          node_id TEXT NOT NULL,
          task_id TEXT NOT NULL,
          parent_step_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          title TEXT NOT NULL,
          status TEXT NOT NULL,
          status_manual INTEGER NOT NULL DEFAULT 0,
          priority TEXT NOT NULL DEFAULT 'MEDIUM',
          blocked INTEGER NOT NULL DEFAULT 0,
          description TEXT,
          context TEXT,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, node_id)
        );
"#;
