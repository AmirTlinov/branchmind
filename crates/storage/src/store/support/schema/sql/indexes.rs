#![forbid(unsafe_code)]

pub(super) const SQL: &str = r#"

        CREATE INDEX IF NOT EXISTS idx_events_workspace_seq ON events(workspace, seq);
        CREATE INDEX IF NOT EXISTS idx_doc_entries_lookup ON doc_entries(workspace, branch, doc, seq);
        CREATE INDEX IF NOT EXISTS idx_doc_entries_workspace_seq ON doc_entries(workspace, seq);
        CREATE INDEX IF NOT EXISTS idx_doc_entries_workspace_branch ON doc_entries(workspace, branch);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_doc_entries_event_dedup ON doc_entries(workspace, branch, doc, source_event_id) WHERE source_event_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_graph_node_versions_seq ON graph_node_versions(workspace, branch, doc, seq);
        CREATE INDEX IF NOT EXISTS idx_graph_node_versions_key ON graph_node_versions(workspace, branch, doc, node_id, seq);
        CREATE INDEX IF NOT EXISTS idx_graph_edge_versions_seq ON graph_edge_versions(workspace, branch, doc, seq);
        CREATE INDEX IF NOT EXISTS idx_graph_edge_versions_key ON graph_edge_versions(workspace, branch, doc, from_id, rel, to_id, seq);
        CREATE INDEX IF NOT EXISTS idx_graph_conflicts_lookup ON graph_conflicts(workspace, into_branch, doc, status, created_at_ms);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_conflicts_dedup
          ON graph_conflicts(workspace, from_branch, into_branch, doc, kind, key, base_cutoff_seq, theirs_seq, ours_seq);
        CREATE INDEX IF NOT EXISTS idx_tasks_parent_plan ON tasks(workspace, parent_plan_id, id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_steps_root_unique ON steps(workspace, task_id, ordinal) WHERE parent_step_id IS NULL;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_steps_child_unique ON steps(workspace, task_id, parent_step_id, ordinal) WHERE parent_step_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_steps_lookup ON steps(workspace, task_id, parent_step_id, ordinal);
        CREATE INDEX IF NOT EXISTS idx_steps_task_completed ON steps(workspace, task_id, completed, created_at_ms);
        CREATE INDEX IF NOT EXISTS idx_step_notes_step_seq ON step_notes(workspace, task_id, step_id, seq);
        CREATE INDEX IF NOT EXISTS idx_task_items_entity ON task_items(workspace, entity_kind, entity_id, field);
        CREATE INDEX IF NOT EXISTS idx_task_nodes_parent ON task_nodes(workspace, task_id, parent_step_id, ordinal);
        CREATE INDEX IF NOT EXISTS idx_ops_history_task ON ops_history(workspace, task_id, seq);
        "#;
