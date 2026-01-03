#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS focus (
          workspace TEXT PRIMARY KEY,
          focus_id TEXT NOT NULL,
          updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS reasoning_refs (
          workspace TEXT NOT NULL,
          id TEXT NOT NULL,
          kind TEXT NOT NULL,
          branch TEXT NOT NULL,
          notes_doc TEXT NOT NULL,
          graph_doc TEXT NOT NULL,
          trace_doc TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, id)
        );

        CREATE TABLE IF NOT EXISTS documents (
          workspace TEXT NOT NULL,
          branch TEXT NOT NULL,
          doc TEXT NOT NULL,
          kind TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, branch, doc)
        );

        CREATE TABLE IF NOT EXISTS doc_entries (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          workspace TEXT NOT NULL,
          branch TEXT NOT NULL,
          doc TEXT NOT NULL,
          ts_ms INTEGER NOT NULL,
          kind TEXT NOT NULL,
          title TEXT,
          format TEXT,
          meta_json TEXT,
          content TEXT,
          source_event_id TEXT,
          event_type TEXT,
          task_id TEXT,
          path TEXT,
          payload_json TEXT
        );

        CREATE TABLE IF NOT EXISTS vcs_refs (
          workspace TEXT NOT NULL,
          ref TEXT NOT NULL,
          doc TEXT NOT NULL,
          branch TEXT NOT NULL,
          seq INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, ref, doc)
        );

        CREATE TABLE IF NOT EXISTS vcs_reflog (
          workspace TEXT NOT NULL,
          ref TEXT NOT NULL,
          doc TEXT NOT NULL,
          branch TEXT NOT NULL,
          old_seq INTEGER,
          new_seq INTEGER NOT NULL,
          message TEXT,
          ts_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, ref, doc, ts_ms, new_seq)
        );

        CREATE TABLE IF NOT EXISTS vcs_tags (
          workspace TEXT NOT NULL,
          name TEXT NOT NULL,
          doc TEXT NOT NULL,
          branch TEXT NOT NULL,
          seq INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, name)
        );

        CREATE TABLE IF NOT EXISTS branches (
          workspace TEXT NOT NULL,
          name TEXT NOT NULL,
          base_branch TEXT NOT NULL,
          base_seq INTEGER NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY (workspace, name)
        );

        CREATE TABLE IF NOT EXISTS branch_checkout (
          workspace TEXT PRIMARY KEY,
          branch TEXT NOT NULL,
          updated_at_ms INTEGER NOT NULL
        );
"#;
