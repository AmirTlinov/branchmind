# Contracts — v1 Migration (old tool names → cmd)

> Server rollout note (v2.0.0, 2026-02-15): legacy knowledge commands are removed by design,
> compatibility flag is intentionally not provided. Use repo-local skills + PlanFS docs.

v1 прячет старые tool‑имена за порталами: основной surface — 10 порталов и `op="call" + cmd`.
Legacy tool names **не принимаются** (будет `UNKNOWN_TOOL`). Миграция делается через
`system op=migration.lookup` и этот документ.

Быстрый способ: `system` → `migration.lookup` вернёт новый `cmd` и минимальный вызов.

## Общие правила

- `tasks_<name>` → `cmd: tasks.<name>` с заменой `_` → `.`
  - исключения: `tasks_create` → `tasks.plan.create`, `tasks_decompose` → `tasks.plan.decompose`
  - `tasks_evidence_capture` → `tasks.evidence.capture`, `tasks_close_step` → `tasks.step.close`
  - пример: `tasks_macro_start` → `tasks.macro.start`
- `tasks_jobs_<name>` → `cmd: jobs.<name>`
  - пример: `tasks_jobs_list` → `jobs.list`
- `tasks_runner_heartbeat` → `jobs.runner.heartbeat`
- `graph_*` → `cmd: graph.<name>`
  - пример: `graph_conflict_show` → `graph.conflict.show`
- `think_*` → `cmd: think.<name>` с заменой `_` → `.`
  - пример: `think_add_hypothesis` → `think.add.hypothesis`
- `think_template` → `think.reasoning.seed`
- `think_pipeline` → `think.reasoning.pipeline`
- `anchors_*` / `anchor_*` → `think.anchor.*`
- `branch_*`, `tag_*`, `checkout`, `log`, `reflog`, `reset`, `commit`, `notes_commit` → `vcs.*`
  - `notes_commit` → `vcs.notes.commit`
- `docs_list`/`show`/`diff`/`merge` → `docs.*`
  - `merge` → `docs.merge`
- `knowledge_*` (legacy knowledge namespace) → removed (no direct replacement in v1)

## Примеры

| Старый tool | Новый tool | Пример вызова |
|---|---|---|
| `tasks_macro_start` | `tasks` | `{ "op": "call", "cmd": "tasks.macro.start", "args": { ... } }` |
| `tasks_jobs_create` | `jobs` | `{ "op": "call", "cmd": "jobs.create", "args": { ... } }` |
| `think_card` | `think` | `{ "op": "call", "cmd": "think.card", "args": { ... } }` |
| `graph_query` | `graph` | `{ "op": "call", "cmd": "graph.query", "args": { ... } }` |
| `branch_create` | `vcs` | `{ "op": "call", "cmd": "vcs.branch.create", "args": { ... } }` |
| `docs_list` | `docs` | `{ "op": "call", "cmd": "docs.list", "args": { ... } }` |
