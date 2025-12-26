# Contracts — Parity Matrix (apply_task + branchmind)

This document defines the **parity target** for branchmind-rust: the union of
the `apply_task` and `branchmind` tool surfaces used by agents in the ecosystem.

Status legend:

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
- tasks_scaffold
- tasks_storage
- tasks_task_add / tasks_task_define / tasks_task_delete
- tasks_templates_list

Planned:

- (none)

## branchmind parity (reasoning + VCS-style)

Present:

- branchmind_init / branchmind_status
- branchmind_branch_create / branchmind_branch_list / branchmind_checkout
- branchmind_notes_commit / branchmind_show / branchmind_diff / branchmind_merge / branchmind_export
- branchmind_graph_apply / branchmind_graph_query / branchmind_graph_validate
- branchmind_graph_diff / branchmind_graph_merge
- branchmind_graph_conflicts / branchmind_graph_conflict_show / branchmind_graph_conflict_resolve
- branchmind_think_template / branchmind_think_card / branchmind_think_context
- branchmind_commit / branchmind_log / branchmind_docs_list
- branchmind_branch_delete / branchmind_branch_rename
- branchmind_tag_create / branchmind_tag_list / branchmind_tag_delete
- branchmind_reflog / branchmind_reset
- branchmind_think_add_hypothesis / branchmind_think_add_question / branchmind_think_add_test
- branchmind_think_next / branchmind_think_frontier / branchmind_think_pack
- branchmind_think_query / branchmind_think_lint / branchmind_think_watch
- branchmind_think_link / branchmind_think_set_status
- branchmind_think_nominal_merge / branchmind_think_pin / branchmind_think_pins
- branchmind_think_playbook
- branchmind_think_subgoal_open / branchmind_think_subgoal_close
- branchmind_trace_step / branchmind_trace_sequential_step / branchmind_trace_hydrate / branchmind_trace_validate

Planned:

- (none)
