# Contracts — Parity Matrix (apply_task + branchmind)

This document defines the **parity target** for branchmind-rust: the union of
the `apply_task` and `branchmind` tool surfaces used by agents in the ecosystem.

Status key:

- **present** — implemented in branchmind-rust v0
- **planned** — parity target for this initiative (implementation pending)

## apply_task parity (tasks_*)

Present:

- tasks_create
- tasks_context
- tasks_edit
- tasks_decompose
- tasks_define
- tasks_note
- tasks_verify
- tasks_done
- tasks_close_step
- tasks_focus_get / tasks_focus_set / tasks_focus_clear
- tasks_delta
- tasks_radar
- tasks_batch
- tasks_block
- tasks_bootstrap
- tasks_close_task
- tasks_complete
- tasks_contract
- tasks_context_pack
- tasks_delete
- tasks_evidence_capture
- tasks_handoff
- tasks_history
- tasks_lint
- tasks_mirror
- tasks_patch
- tasks_plan
- tasks_progress
- tasks_redo / tasks_undo
- tasks_resume
- tasks_resume_pack
- tasks_resume_super
- tasks_scaffold
- tasks_storage
- tasks_task_add / tasks_task_define / tasks_task_delete
- tasks_templates_list
- tasks_macro_start / tasks_macro_close_step / tasks_macro_finish

Planned:

- (none)

## branchmind parity (reasoning + VCS-style)

Present:

- init / status / workspace_use / workspace_reset / help / diagnostics
- branch_create / branch_list / checkout
- macro_branch_note
- notes_commit / show / diff / merge / export
- context_pack
- graph_apply / graph_query / graph_validate
- graph_diff / graph_merge
- graph_conflicts / graph_conflict_show / graph_conflict_resolve
- think_template / think_card / think_context
- think_pipeline
- commit / log / docs_list
- branch_delete / branch_rename
- tag_create / tag_list / tag_delete
- reflog / reset
- think_add_hypothesis / think_add_question / think_add_test
- think_add_note / think_add_decision / think_add_evidence / think_add_knowledge / think_add_frame / think_add_update
- think_next / think_frontier / think_pack
- think_query / think_lint / think_watch
- think_link / think_set_status
- think_nominal_merge / think_pin / think_pins
- think_playbook
- think_subgoal_open / think_subgoal_close
- trace_step / trace_sequential_step / trace_hydrate / trace_validate
- transcripts_search / transcripts_open / transcripts_digest
- knowledge_list

Planned:

- (none)
