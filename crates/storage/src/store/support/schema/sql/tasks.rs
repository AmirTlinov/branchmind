#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS plans (
          workspace TEXT NOT NULL,
          id TEXT NOT NULL,
          revision INTEGER NOT NULL,
          title TEXT NOT NULL,
          contract TEXT,
          contract_json TEXT,
          description TEXT,
          context TEXT,
          status TEXT NOT NULL DEFAULT 'TODO',
          status_manual INTEGER NOT NULL DEFAULT 0,
          priority TEXT NOT NULL DEFAULT 'MEDIUM',
          plan_doc TEXT,
          plan_current INTEGER NOT NULL DEFAULT 0,
          criteria_confirmed INTEGER NOT NULL DEFAULT 0,
          tests_confirmed INTEGER NOT NULL DEFAULT 0,
          criteria_auto_confirmed INTEGER NOT NULL DEFAULT 0,
          tests_auto_confirmed INTEGER NOT NULL DEFAULT 1,
          security_confirmed INTEGER NOT NULL DEFAULT 0,
          perf_confirmed INTEGER NOT NULL DEFAULT 0,
          docs_confirmed INTEGER NOT NULL DEFAULT 0,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
          workspace TEXT NOT NULL,
          id TEXT NOT NULL,
          revision INTEGER NOT NULL,
          parent_plan_id TEXT NOT NULL,
          title TEXT NOT NULL,
          description TEXT,
          status TEXT NOT NULL DEFAULT 'TODO',
          status_manual INTEGER NOT NULL DEFAULT 0,
          priority TEXT NOT NULL DEFAULT 'MEDIUM',
          blocked INTEGER NOT NULL DEFAULT 0,
          assignee TEXT,
          domain TEXT,
          phase TEXT,
          component TEXT,
          context TEXT,
          criteria_confirmed INTEGER NOT NULL DEFAULT 0,
          tests_confirmed INTEGER NOT NULL DEFAULT 0,
          criteria_auto_confirmed INTEGER NOT NULL DEFAULT 0,
          tests_auto_confirmed INTEGER NOT NULL DEFAULT 1,
          security_confirmed INTEGER NOT NULL DEFAULT 0,
          perf_confirmed INTEGER NOT NULL DEFAULT 0,
          docs_confirmed INTEGER NOT NULL DEFAULT 0,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, id)
        );

        CREATE TABLE IF NOT EXISTS events (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          workspace TEXT NOT NULL,
          ts_ms INTEGER NOT NULL,
          task_id TEXT,
          path TEXT,
          type TEXT NOT NULL,
          payload_json TEXT NOT NULL
        );
"#;
