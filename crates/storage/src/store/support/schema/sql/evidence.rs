#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE TABLE IF NOT EXISTS evidence_artifacts (
          workspace TEXT NOT NULL,
          entity_kind TEXT NOT NULL,
          entity_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          kind TEXT NOT NULL,
          command TEXT,
          stdout TEXT,
          stderr TEXT,
          exit_code INTEGER,
          diff TEXT,
          content TEXT,
          url TEXT,
          external_uri TEXT,
          meta_json TEXT,
          PRIMARY KEY (workspace, entity_kind, entity_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS evidence_checks (
          workspace TEXT NOT NULL,
          entity_kind TEXT NOT NULL,
          entity_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          check_text TEXT NOT NULL,
          PRIMARY KEY (workspace, entity_kind, entity_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS evidence_attachments (
          workspace TEXT NOT NULL,
          entity_kind TEXT NOT NULL,
          entity_id TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          attachment TEXT NOT NULL,
          PRIMARY KEY (workspace, entity_kind, entity_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS checkpoint_notes (
          workspace TEXT NOT NULL,
          entity_kind TEXT NOT NULL,
          entity_id TEXT NOT NULL,
          checkpoint TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          note TEXT NOT NULL,
          PRIMARY KEY (workspace, entity_kind, entity_id, checkpoint, ordinal)
        );

        CREATE TABLE IF NOT EXISTS checkpoint_evidence (
          workspace TEXT NOT NULL,
          entity_kind TEXT NOT NULL,
          entity_id TEXT NOT NULL,
          checkpoint TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          ref TEXT NOT NULL,
          PRIMARY KEY (workspace, entity_kind, entity_id, checkpoint, ordinal)
        );
"#;
