[LEGEND]
planfs_v1:
  id: SLICE-5
  title: 'Slice-5: Delegation интеграция с PlanFS (target_ref → bounded slice context; CODE_REF gates)'
  objective: jobs.macro.dispatch.* accept planfs slice target_ref and include bounded excerpt
  status: todo
  budgets:
    max_files: 16
    max_diff_lines: 1500
    max_context_refs: 24
  dod:
    success_criteria:
    - jobs.macro.dispatch.* accept planfs slice target_ref and include bounded excerpt
    - Scout pack enforces CODE_REF/path binding for key claims (fail-closed)
    - No copy/paste prompts required; uses PlanFS slice context
    - Verify passes
    tests:
    - make check
    blockers:
    - No blockers at the moment.
    rollback:
    - Rollback slice 5 changes.
  tasks:
  - title: Execution lane 1
    success_criteria:
    - Execution lane 1 completed
    tests:
    - make check
    blockers:
    - No blockers at the moment.
    rollback:
    - Rollback Execution lane 1 changes.
    steps:
    - title: Execution lane 1 — implement
      success_criteria:
      - Execution lane 1 implementation done
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 1 implementation.
    - title: Execution lane 1 — validate
      success_criteria:
      - Execution lane 1 validated
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 1 validation.
    - title: Execution lane 1 — finalize
      success_criteria:
      - Execution lane 1 finalized
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 1 finalization.
  - title: Execution lane 2
    success_criteria:
    - Execution lane 2 completed
    tests:
    - make check
    blockers:
    - No blockers at the moment.
    rollback:
    - Rollback Execution lane 2 changes.
    steps:
    - title: Execution lane 2 — implement
      success_criteria:
      - Execution lane 2 implementation done
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 2 implementation.
    - title: Execution lane 2 — validate
      success_criteria:
      - Execution lane 2 validated
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 2 validation.
    - title: Execution lane 2 — finalize
      success_criteria:
      - Execution lane 2 finalized
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 2 finalization.
  - title: Execution lane 3
    success_criteria:
    - Execution lane 3 completed
    tests:
    - make check
    blockers:
    - No blockers at the moment.
    rollback:
    - Rollback Execution lane 3 changes.
    steps:
    - title: Execution lane 3 — implement
      success_criteria:
      - Execution lane 3 implementation done
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 3 implementation.
    - title: Execution lane 3 — validate
      success_criteria:
      - Execution lane 3 validated
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 3 validation.
    - title: Execution lane 3 — finalize
      success_criteria:
      - Execution lane 3 finalized
      tests:
      - make check
      blockers:
      - No blockers at the moment.
      rollback:
      - Rollback Execution lane 3 finalization.

[CONTENT]
## Goal
jobs.macro.dispatch.* accept planfs slice target_ref and include bounded excerpt
## Scope
- Keep scope inside this slice boundary.
## Non-goals
- No edits outside slice scope.
## Interfaces
- Do not change external interfaces without explicit contract update.
## Contracts
- Contract-first updates only.
## Tests
- make check
## Proof
- FILE:docs/plans/flagship-ux-v2-planfs-no-knowledge-golden-surface/Slice-5.md
## Rollback
- Rollback slice 5 changes.
## Risks
- Plan drift between task tree and files.
