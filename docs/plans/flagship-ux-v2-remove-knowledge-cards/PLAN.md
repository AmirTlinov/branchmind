[LEGEND]
planfs_v1:
  plan_slug: flagship-ux-v2-remove-knowledge-cards
  title: Slice-1 — Remove knowledge cards + commands (no legacy)
  objective: Delete think.knowledge.* + think.add.knowledge + promote_to_knowledge; update contracts/docs; remove storage/index/migrations; update lint/help/skills to reference repo-local skills.
  constraints:
  - Contract-first changes only; keep behavior deterministic and fail-closed.
  policy: strict
  slices:
  - id: SLICE-1
    title: 'Contracts: remove knowledge from v1 surface'
    file: Slice-1.md
    status: todo
  - id: SLICE-2
    title: 'Implementation: remove knowledge handlers/storage'
    file: Slice-2.md
    status: todo
  - id: SLICE-3
    title: 'Verification: update tests + docs + regression suite'
    file: Slice-3.md
    status: todo

[CONTENT]
## Goal
Delete think.knowledge.* + think.add.knowledge + promote_to_knowledge; update contracts/docs; remove storage/index/migrations; update lint/help/skills to reference repo-local skills.
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
- CMD: tasks.planfs.export --task TASK-111
## Rollback
- Rollback per-slice changes if Verify turns red.
## Risks
- Agent drift or partial implementation between slices.
