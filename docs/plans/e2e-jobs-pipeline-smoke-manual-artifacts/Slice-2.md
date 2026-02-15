[LEGEND]
planfs_v1:
  id: SLICE-2
  title: 'Implementation: Continue full implementation per latest changes (PLAN-010.2)'
  objective: Implementation scope is delivered with explicit boundaries.
  status: todo
  budgets:
    max_files: 16
    max_diff_lines: 1500
    max_context_refs: 24
  dod:
    success_criteria:
    - Implementation scope is delivered with explicit boundaries.
    - Implementation has deterministic pass/fail evidence.
    tests:
    - 'Implementation: unit/contract checks pass'
    - 'Implementation: regression checks pass'
    blockers:
    - No quick-fixes or hidden side effects.
    - No scope expansion outside this slice.
    rollback:
    - Rollback slice 2 changes.
  tasks:
  - title: Implementation — define exact contracts and invariants
    success_criteria:
    - Contracts are explicit and bounded.
    - Unknowns are listed with falsifier.
    tests:
    - Contract fixtures updated
    blockers:
    - No ambiguous acceptance criteria.
    rollback:
    - Rollback task "Implementation — define exact contracts and invariants" changes and restore previous behavior.
    steps:
    - title: Contracts are explicit and bounded.
      success_criteria:
      - Contracts are explicit and bounded. completed
      tests:
      - Contract fixtures updated
      blockers:
      - No ambiguous acceptance criteria.
      rollback:
      - Rollback step 1 in task "Implementation — define exact contracts and invariants".
    - title: Unknowns are listed with falsifier.
      success_criteria:
      - Unknowns are listed with falsifier. completed
      tests:
      - Contract fixtures updated
      blockers:
      - No ambiguous acceptance criteria.
      rollback:
      - Rollback step 2 in task "Implementation — define exact contracts and invariants".
    - title: Implementation — define exact contracts and invariants — step 3
      success_criteria:
      - Implementation — define exact contracts and invariants — step 3 completed
      tests:
      - Contract fixtures updated
      blockers:
      - No ambiguous acceptance criteria.
      rollback:
      - Rollback step 3 in task "Implementation — define exact contracts and invariants".
  - title: Implementation — implement minimal cohesive change
    success_criteria:
    - Change is minimal and reviewable.
    - Architecture boundaries remain intact.
    tests:
    - Implementation compiles and targeted tests pass
    blockers:
    - No duplicated logic.
    rollback:
    - Rollback task "Implementation — implement minimal cohesive change" changes and restore previous behavior.
    steps:
    - title: Change is minimal and reviewable.
      success_criteria:
      - Change is minimal and reviewable. completed
      tests:
      - Implementation compiles and targeted tests pass
      blockers:
      - No duplicated logic.
      rollback:
      - Rollback step 1 in task "Implementation — implement minimal cohesive change".
    - title: Architecture boundaries remain intact.
      success_criteria:
      - Architecture boundaries remain intact. completed
      tests:
      - Implementation compiles and targeted tests pass
      blockers:
      - No duplicated logic.
      rollback:
      - Rollback step 2 in task "Implementation — implement minimal cohesive change".
    - title: Implementation — implement minimal cohesive change — step 3
      success_criteria:
      - Implementation — implement minimal cohesive change — step 3 completed
      tests:
      - Implementation compiles and targeted tests pass
      blockers:
      - No duplicated logic.
      rollback:
      - Rollback step 3 in task "Implementation — implement minimal cohesive change".
  - title: Implementation — validate, prove, and prepare rollback
    success_criteria:
    - Evidence collected for DoD and policy.
    - Rollback command/path verified.
    tests:
    - Smoke/regression checks recorded
    blockers:
    - No close without proof refs.
    rollback:
    - Rollback task "Implementation — validate, prove, and prepare rollback" changes and restore previous behavior.
    steps:
    - title: Evidence collected for DoD and policy.
      success_criteria:
      - Evidence collected for DoD and policy. completed
      tests:
      - Smoke/regression checks recorded
      blockers:
      - No close without proof refs.
      rollback:
      - Rollback step 1 in task "Implementation — validate, prove, and prepare rollback".
    - title: Rollback command/path verified.
      success_criteria:
      - Rollback command/path verified. completed
      tests:
      - Smoke/regression checks recorded
      blockers:
      - No close without proof refs.
      rollback:
      - Rollback step 2 in task "Implementation — validate, prove, and prepare rollback".
    - title: Implementation — validate, prove, and prepare rollback — step 3
      success_criteria:
      - Implementation — validate, prove, and prepare rollback — step 3 completed
      tests:
      - Smoke/regression checks recorded
      blockers:
      - No close without proof refs.
      rollback:
      - Rollback step 3 in task "Implementation — validate, prove, and prepare rollback".

[CONTENT]
## Goal
Implementation scope is delivered with explicit boundaries.
## Scope
- Keep scope inside this slice boundary.
## Non-goals
- No edits outside slice scope.
## Interfaces
- Do not change external interfaces without explicit contract update.
## Contracts
- Contract-first updates only.
## Tests
- Implementation: unit/contract checks pass
- Implementation: regression checks pass
## Proof
- FILE:docs/plans/e2e-jobs-pipeline-smoke-manual-artifacts/Slice-2.md
## Rollback
- Rollback slice 2 changes.
## Risks
- Plan drift between task tree and files.
