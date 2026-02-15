[LEGEND]
planfs_v1:
  id: SLICE-3
  title: 'Validation and readiness: Continue full implementation per latest changes (PLAN-010.3)'
  objective: Validation and readiness scope is delivered with explicit boundaries.
  status: todo
  budgets:
    max_files: 16
    max_diff_lines: 1500
    max_context_refs: 24
  dod:
    success_criteria:
    - Validation and readiness scope is delivered with explicit boundaries.
    - Validation and readiness has deterministic pass/fail evidence.
    tests:
    - 'Validation and readiness: unit/contract checks pass'
    - 'Validation and readiness: regression checks pass'
    blockers:
    - No quick-fixes or hidden side effects.
    - No scope expansion outside this slice.
    rollback:
    - Rollback slice 3 changes.
  tasks:
  - title: Validation and readiness — define exact contracts and invariants
    success_criteria:
    - Contracts are explicit and bounded.
    - Unknowns are listed with falsifier.
    tests:
    - Contract fixtures updated
    blockers:
    - No ambiguous acceptance criteria.
    rollback:
    - Rollback task "Validation and readiness — define exact contracts and invariants" changes and restore previous behavior.
    steps:
    - title: Contracts are explicit and bounded.
      success_criteria:
      - Contracts are explicit and bounded. completed
      tests:
      - Contract fixtures updated
      blockers:
      - No ambiguous acceptance criteria.
      rollback:
      - Rollback step 1 in task "Validation and readiness — define exact contracts and invariants".
    - title: Unknowns are listed with falsifier.
      success_criteria:
      - Unknowns are listed with falsifier. completed
      tests:
      - Contract fixtures updated
      blockers:
      - No ambiguous acceptance criteria.
      rollback:
      - Rollback step 2 in task "Validation and readiness — define exact contracts and invariants".
    - title: Validation and readiness — define exact contracts and invariants — step 3
      success_criteria:
      - Validation and readiness — define exact contracts and invariants — step 3 completed
      tests:
      - Contract fixtures updated
      blockers:
      - No ambiguous acceptance criteria.
      rollback:
      - Rollback step 3 in task "Validation and readiness — define exact contracts and invariants".
  - title: Validation and readiness — implement minimal cohesive change
    success_criteria:
    - Change is minimal and reviewable.
    - Architecture boundaries remain intact.
    tests:
    - Implementation compiles and targeted tests pass
    blockers:
    - No duplicated logic.
    rollback:
    - Rollback task "Validation and readiness — implement minimal cohesive change" changes and restore previous behavior.
    steps:
    - title: Change is minimal and reviewable.
      success_criteria:
      - Change is minimal and reviewable. completed
      tests:
      - Implementation compiles and targeted tests pass
      blockers:
      - No duplicated logic.
      rollback:
      - Rollback step 1 in task "Validation and readiness — implement minimal cohesive change".
    - title: Architecture boundaries remain intact.
      success_criteria:
      - Architecture boundaries remain intact. completed
      tests:
      - Implementation compiles and targeted tests pass
      blockers:
      - No duplicated logic.
      rollback:
      - Rollback step 2 in task "Validation and readiness — implement minimal cohesive change".
    - title: Validation and readiness — implement minimal cohesive change — step 3
      success_criteria:
      - Validation and readiness — implement minimal cohesive change — step 3 completed
      tests:
      - Implementation compiles and targeted tests pass
      blockers:
      - No duplicated logic.
      rollback:
      - Rollback step 3 in task "Validation and readiness — implement minimal cohesive change".
  - title: Validation and readiness — validate, prove, and prepare rollback
    success_criteria:
    - Evidence collected for DoD and policy.
    - Rollback command/path verified.
    tests:
    - Smoke/regression checks recorded
    blockers:
    - No close without proof refs.
    rollback:
    - Rollback task "Validation and readiness — validate, prove, and prepare rollback" changes and restore previous behavior.
    steps:
    - title: Evidence collected for DoD and policy.
      success_criteria:
      - Evidence collected for DoD and policy. completed
      tests:
      - Smoke/regression checks recorded
      blockers:
      - No close without proof refs.
      rollback:
      - Rollback step 1 in task "Validation and readiness — validate, prove, and prepare rollback".
    - title: Rollback command/path verified.
      success_criteria:
      - Rollback command/path verified. completed
      tests:
      - Smoke/regression checks recorded
      blockers:
      - No close without proof refs.
      rollback:
      - Rollback step 2 in task "Validation and readiness — validate, prove, and prepare rollback".
    - title: Validation and readiness — validate, prove, and prepare rollback — step 3
      success_criteria:
      - Validation and readiness — validate, prove, and prepare rollback — step 3 completed
      tests:
      - Smoke/regression checks recorded
      blockers:
      - No close without proof refs.
      rollback:
      - Rollback step 3 in task "Validation and readiness — validate, prove, and prepare rollback".

[CONTENT]
## Goal
Validation and readiness scope is delivered with explicit boundaries.
## Scope
- Keep scope inside this slice boundary.
## Non-goals
- No edits outside slice scope.
## Interfaces
- Do not change external interfaces without explicit contract update.
## Contracts
- Contract-first updates only.
## Tests
- Validation and readiness: unit/contract checks pass
- Validation and readiness: regression checks pass
## Proof
- FILE:docs/plans/e2e-jobs-pipeline-smoke-manual-artifacts/Slice-3.md
## Rollback
- Rollback slice 3 changes.
## Risks
- Plan drift between task tree and files.
