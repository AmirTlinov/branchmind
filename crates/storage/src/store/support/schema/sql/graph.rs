#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS graph_node_versions (
          workspace TEXT NOT NULL,
          branch TEXT NOT NULL,
          doc TEXT NOT NULL,
          seq INTEGER NOT NULL,
          ts_ms INTEGER NOT NULL,
          node_id TEXT NOT NULL,
          node_type TEXT,
          title TEXT,
          text TEXT,
          tags TEXT,
          status TEXT,
          meta_json TEXT,
          deleted INTEGER NOT NULL,
          PRIMARY KEY (workspace, branch, doc, node_id, seq)
        );

        CREATE TABLE IF NOT EXISTS graph_edge_versions (
          workspace TEXT NOT NULL,
          branch TEXT NOT NULL,
          doc TEXT NOT NULL,
          seq INTEGER NOT NULL,
          ts_ms INTEGER NOT NULL,
          from_id TEXT NOT NULL,
          rel TEXT NOT NULL,
          to_id TEXT NOT NULL,
          meta_json TEXT,
          deleted INTEGER NOT NULL,
          PRIMARY KEY (workspace, branch, doc, from_id, rel, to_id, seq)
        );

        CREATE TABLE IF NOT EXISTS graph_conflicts (
          workspace TEXT NOT NULL,
          conflict_id TEXT NOT NULL,
          kind TEXT NOT NULL,
          key TEXT NOT NULL,
          from_branch TEXT NOT NULL,
          into_branch TEXT NOT NULL,
          doc TEXT NOT NULL,
          base_cutoff_seq INTEGER NOT NULL,

          base_seq INTEGER,
          base_ts_ms INTEGER,
          base_deleted INTEGER,
          base_node_type TEXT,
          base_title TEXT,
          base_text TEXT,
          base_tags TEXT,
          base_status TEXT,
          base_meta_json TEXT,
          base_from_id TEXT,
          base_rel TEXT,
          base_to_id TEXT,
          base_edge_meta_json TEXT,

          theirs_seq INTEGER,
          theirs_ts_ms INTEGER,
          theirs_deleted INTEGER,
          theirs_node_type TEXT,
          theirs_title TEXT,
          theirs_text TEXT,
          theirs_tags TEXT,
          theirs_status TEXT,
          theirs_meta_json TEXT,
          theirs_from_id TEXT,
          theirs_rel TEXT,
          theirs_to_id TEXT,
          theirs_edge_meta_json TEXT,

          ours_seq INTEGER,
          ours_ts_ms INTEGER,
          ours_deleted INTEGER,
          ours_node_type TEXT,
          ours_title TEXT,
          ours_text TEXT,
          ours_tags TEXT,
          ours_status TEXT,
          ours_meta_json TEXT,
          ours_from_id TEXT,
          ours_rel TEXT,
          ours_to_id TEXT,
          ours_edge_meta_json TEXT,

          status TEXT NOT NULL,
          resolution TEXT,
          created_at_ms INTEGER NOT NULL,
          resolved_at_ms INTEGER,

          PRIMARY KEY (workspace, conflict_id)
        );
"#;
