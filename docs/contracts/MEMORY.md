# Contracts — Reasoning Memory Surface (v0)

The reasoning subsystem is a durable, agent-first working memory:

- append-only logs for “what happened and why”,
- branching and merging for what-if exploration,
- typed graph for linking hypotheses/questions/tests/evidence/decisions.

## Principles

- Store explicit artifacts only (notes, decisions, evidence, diffs).
- Default retrieval is low-noise (log/diff/summary).
- Full artifacts are opt-in and bounded.

## Workspace scoping (MUST)

All stateful tools in this family operate inside an explicit `workspace` (a stable IDE-provided identifier).

- `workspace` is required in v0 to avoid implicit context and silent cross-project writes.

## Tool groups (MVP)

- Repo/workspace:
  - `branchmind_status` / `branchmind_init`
  - `branchmind_branch_create` / `branchmind_branch_list` / `branchmind_checkout`
- Versioned artifacts:
  - `branchmind_notes_commit` (notes-only fast path)
  - `branchmind_commit` / `branchmind_log` / `branchmind_show` / `branchmind_diff`
- Merges and conflicts:
  - `branchmind_merge`
  - `branchmind_graph_merge` + `branchmind_graph_conflict_show` + `branchmind_graph_conflict_resolve`
- Graph:
  - `branchmind_graph_apply` / `branchmind_graph_query` / `branchmind_graph_validate` / `branchmind_graph_diff`
- Thinking structure (optional but powerful):
  - `branchmind_think_template` / `branchmind_think_next` / `branchmind_think_card` / `branchmind_think_context`

## Output budgets

All read-ish tools must accept `max_bytes`/`max_chars`/`max_lines` and return `truncated=true` when applicable.
