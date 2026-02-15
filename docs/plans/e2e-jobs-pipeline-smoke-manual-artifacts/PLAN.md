[LEGEND]
planfs_v1:
  plan_slug: e2e-jobs-pipeline-smoke-manual-artifacts
  title: SLC-00000005 — Slice for E2E jobs pipeline smoke (manual artifacts)
  objective: Continue full implementation per latest changes
  constraints:
  - '{"budgets":{"max_context_refs":24,"max_diff_lines":1200,"max_files":12},"dod":{"blockers":["No overengineering and no scaffold-only code.","No hidden behavior changes outside slice scope."],"criteria":["Slice outcome is directly tied to plan objective.","Implementation stays within declared budgets."],"tests":["All slice-level checks listed in tasks are executed.","No regression in touched area."]},"non_goals":["Keep scope strictly within one reviewable slice.","No utility-junk or speculative abstractions.","All tests and rollback path must be explicit."],"objective":"Continue full implementation per latest changes","shared_context_refs":["PLAN:PLAN-010"],"tasks":[{"blockers":["No quick-fixes or hidden side effects.","No scope expansion outside this slice."],"steps":[{"blockers":["No ambiguous acceptance criteria."],"success_criteria":["Contracts are explicit and bounded.","Unknowns are listed with falsifier."],"tests":["Contract fixtures updated"],"title":"Context and design — define exact contracts and invariants"},{"blockers":["No duplicated logic."],"success_criteria":["Change is minimal and reviewable.","Architecture boundaries remain intact."],"tests":["Implementation compiles and targeted tests pass"],"title":"Context and design — implement minimal cohesive change"},{"blockers":["No close without proof refs."],"success_criteria":["Evidence collected for DoD and policy.","Rollback command/path verified."],"tests":["Smoke/regression checks recorded"],"title":"Context and design — validate, prove, and prepare rollback"}],"success_criteria":["Context and design scope is delivered with explicit boundaries.","Context and design has deterministic pass/fail evidence."],"tests":["Context and design: unit/contract checks pass","Context and design: regression checks pass"],"title":"Context and design: Continue full implementation per latest changes (PLAN-010.1)"},{"blockers":["No quick-fixes or hidden side effects.","No scope expansion outside this slice."],"steps":[{"blockers":["No ambiguous acceptance criteria."],"success_criteria":["Contracts are explicit and bounded.","Unknowns are listed with falsifier."],"tests":["Contract fixtures updated"],"title":"Implementation — define exact contracts and invariants"},{"blockers":["No duplicated logic."],"success_criteria":["Change is minimal and reviewable.","Architecture boundaries remain intact."],"tests":["Implementation compiles and targeted tests pass"],"title":"Implementation — implement minimal cohesive change"},{"blockers":["No close without proof refs."],"success_criteria":["Evidence collected for DoD and policy.","Rollback command/path verified."],"tests":["Smoke/regression checks recorded"],"title":"Implementation — validate, prove, and prepare rollback"}],"success_criteria":["Implementation scope is delivered with explicit boundaries.","Implementation has deterministic pass/fail evidence."],"tests":["Implementation: unit/contract checks pass","Implementation: regression checks pass"],"title":"Implementation: Continue full implementation per latest changes (PLAN-010.2)"},{"blockers":["No quick-fixes or hidden side effects.","No scope expansion outside this slice."],"steps":[{"blockers":["No ambiguous acceptance criteria."],"success_criteria":["Contracts are explicit and bounded.","Unknowns are listed with falsifier."],"tests":["Contract fixtures updated"],"title":"Validation and readiness — define exact contracts and invariants"},{"blockers":["No duplicated logic."],"success_criteria":["Change is minimal and reviewable.","Architecture boundaries remain intact."],"tests":["Implementation compiles and targeted tests pass"],"title":"Validation and readiness — implement minimal cohesive change"},{"blockers":["No close without proof refs."],"success_criteria":["Evidence collected for DoD and policy.","Rollback command/path verified."],"tests":["Smoke/regression checks recorded"],"title":"Validation and readiness — validate, prove, and prepare rollback"}],"success_criteria":["Validation and readiness scope is delivered with explicit boundaries.","Validation and readiness has deterministic pass/fail evidence."],"tests":["Validation and readiness: unit/contract checks pass","Validation and readiness: regression checks pass"],"title":"Validation and readiness: Continue full implementation per latest changes (PLAN-010.3)"}],"title":"Slice for E2E jobs pipeline smoke (manual artifacts)"}'
  policy: strict
  slices:
  - id: SLICE-1
    title: 'Context and design: Continue full implementation per latest changes (PLAN-010.1)'
    file: Slice-1.md
    status: todo
  - id: SLICE-2
    title: 'Implementation: Continue full implementation per latest changes (PLAN-010.2)'
    file: Slice-2.md
    status: todo
  - id: SLICE-3
    title: 'Validation and readiness: Continue full implementation per latest changes (PLAN-010.3)'
    file: Slice-3.md
    status: todo

[CONTENT]
## Goal
Continue full implementation per latest changes
## Scope
- Implement slices sequentially with green verify gates.
## Non-goals
- No silent scope creep.
## Interfaces
- Any interface change must update contracts/docs.
## Contracts
- Keep MCP schemas and docs aligned.
## Tests
- make check
## Proof
- CMD: tasks.planfs.export --task TASK-112
## Rollback
- Rollback per-slice changes if Verify turns red.
## Risks
- Agent drift or partial implementation between slices.
