# Contracts — v1 Ops (deprecated)

> Deprecated: use `V1_COMMANDS.md` as the SSOT registry. This file is kept for legacy context only.

## Unified input for all *_ops

```json
{
  "workspace": "string?",
  "op": "string",
  "cmd": "string?",              // required only for op="call"
  "args": "object",              // required
  "budget_profile": "portal|default|audit",
  "portal_view": "compact|smart|audit" // alias: view (deprecated)
}
```

## Response envelope (v1)

See `TYPES.md` for the full envelope. v1 adds:

- `actions[]` (deterministic, priority‑ordered)
- `suggestions[]` всегда `[]`

## Golden ops (tools/list)

### status
- `status` — health + NextEngine (actions)

### open
- `open` — open by ref/id

### workspace_ops
- `use`
- `reset`
- `call`

### tasks_ops
- `plan.create`
- `plan.decompose`
- `execute.next`
- `evidence.capture`
- `step.close`
- `call`

### jobs_ops
- `create`
- `list`
- `radar`
- `open`
- `call`

### think_ops
- `reasoning.seed`
- `reasoning.pipeline`
- `idea.branch.create`
- `idea.branch.merge`
- `call`

### graph_ops
- `query`
- `apply`
- `merge`
- `call`

### vcs_ops
- `branch.create`
- `branch.merge`
- `call`

### docs_ops
- `list`
- `show`
- `diff`
- `call`

### system_ops
- `schema.get`
- `call`

## system_ops schema.get

Input:

```json
{ "cmd": "tasks.note" }
```

Output:

```json
{
  "cmd": "tasks.note",
  "args_schema": { ... },
  "example_minimal_args": { ... },
  "example_valid_call": { "op": "call", "cmd": "tasks.note", "args": { ... } },
  "doc_ref": { "path": "docs/contracts/V1_OPS.md", "anchor": "#cmd-registry" },
  "default_budget_profile": "default"
}
```

## Command registry (SSOT)

Все доступные `cmd` перечислены здесь (см. `V1_MIGRATION.md` для маппинга со старыми именами):

<!-- CMD-REGISTRY:BEGIN -->
<!-- (auto‑generated list inserted by ops registry) -->
<!-- CMD-REGISTRY:END -->
