# Goals — BranchMind Rust (Reasoning-only MCP)

BranchMind is a deterministic local MCP server for agent thinking workflows.

## Product scope

Exactly three MCP tools are in scope:

- `branch`
- `think`
- `merge`

All tools use a single strict markdown input surface (` ```bm ... ``` `).

## Primary goals

1) **Deterministic reasoning workflow**
- Branch thought lanes deliberately.
- Append explicit thought commits.
- Merge winning ideas back into target lanes.

2) **Fail-closed contract**
- Unknown tools/verbs/args are rejected.
- Typed errors always include actionable recovery.
- No silent compatibility shims.

3) **Minimal, auditable runtime**
- Local embedded store only.
- No outbound network I/O.
- No arbitrary shell execution in `bm_mcp`.

4) **Agent-efficient UX**
- Small tool surface.
- Bounded outputs.
- Stable schema for easy automation.

## Non-goals

- Task/plan/job orchestration APIs.
- Portal/alias legacy surfaces.
- Delegation runner integration.
- Knowledge-card, graph, and transcript subsystems from legacy lines.

## Release gates

- `make check` passes.
- `tools/list` advertises only `branch`, `think`, `merge`.
- Contracts/docs describe only reasoning-only behavior.
- Legacy storage layouts fail closed with `RESET_REQUIRED`.
