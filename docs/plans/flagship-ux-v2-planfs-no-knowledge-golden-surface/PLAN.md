[LEGEND]
planfs_v1:
  plan_slug: flagship-ux-v2-planfs-no-knowledge-golden-surface
  title: Implement PlanFS v1 + remove Knowledge + golden-by-default UX
  objective: 'Implement breaking v2.0 UX/contract changes: remove knowledge cards, add PlanFS v1 (init/import/export), add plan_spec doc kind for branching/merge, simplify cmd/schema listing (golden by default), integrate jobs delegation with PlanFS slice context, enforce sequential reasoning checkpoints.'
  constraints:
  - Contract-first changes only; keep behavior deterministic and fail-closed.
  policy: strict
  slices:
  - id: SLICE-1
    title: 'Slice-1: Remove Knowledge полностью (contracts+handlers+storage+migrations+lint/help)'
    file: Slice-1.md
    status: todo
  - id: SLICE-2
    title: 'Slice-2: system.cmd.list/schema.list mode=golden|all + обновление quickstart/tutorial'
    file: Slice-2.md
    status: todo
  - id: SLICE-3
    title: 'Slice-3: PlanFS v1 init/import/export + строгая валидация + idempotent renderer'
    file: Slice-3.md
    status: todo
  - id: SLICE-4
    title: 'Slice-4: Plan branching/merge via docs kind=plan_spec + export to files'
    file: Slice-4.md
    status: todo
  - id: SLICE-5
    title: 'Slice-5: Delegation интеграция с PlanFS (target_ref → bounded slice context; CODE_REF gates)'
    file: Slice-5.md
    status: todo
  - id: SLICE-6
    title: 'Slice-6: Sequential thinking enforcement в strict reasoning_mode (структурные checkpoints)'
    file: Slice-6.md
    status: todo

[CONTENT]
## Goal
Implement breaking v2.0 UX/contract changes: remove knowledge cards, add PlanFS v1 (init/import/export), add plan_spec doc kind for branching/merge, simplify cmd/schema listing (golden by default), integrate jobs delegation with PlanFS slice context, enforce sequential reasoning checkpoints.
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
- CMD: tasks.planfs.export --task TASK-110
## Rollback
- Rollback per-slice changes if Verify turns red.
## Risks
- Agent drift or partial implementation between slices.
