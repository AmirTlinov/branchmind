[LEGEND]
planfs_v1:
  id: SLICE-1
  title: 'Contracts: remove knowledge from v1 surface'
  objective: Contracts updated (MEMORY/PARITY/V1_COMMANDS) and schema discovery reflects removal
  status: todo
  budgets:
    max_files: 16
    max_diff_lines: 1500
    max_context_refs: 24
  dod:
    success_criteria:
    - Contracts updated (MEMORY/PARITY/V1_COMMANDS) and schema discovery reflects removal
    tests:
    - cargo test -p bm_mcp schema (or equivalent)
    blockers:
    - No legacy compatibility; fail closed with clear recovery hints
    rollback:
    - Rollback slice 1 changes.
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
Contracts updated (MEMORY/PARITY/V1_COMMANDS) and schema discovery reflects removal
## Scope
- Keep scope inside this slice boundary.
## Non-goals
- No edits outside slice scope.
## Interfaces
- Do not change external interfaces without explicit contract update.
## Contracts
- Contract-first updates only.
## Tests
- cargo test -p bm_mcp schema (or equivalent)
## Proof
- FILE:docs/plans/flagship-ux-v2-remove-knowledge-cards/Slice-1.md
## Rollback
- Rollback slice 1 changes.
## Risks
- Plan drift between task tree and files.
