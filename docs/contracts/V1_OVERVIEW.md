# Contracts — v1 Overview (actions-first, cmd-first)

BranchMind v1 стандартизирует MCP‑поверхность вокруг **10 инструментов**, **cmd‑registry** и
**actions‑first** UX. Старые имена инструментов **не являются частью контракта v1**:
`tools/list` показывает только 10 порталов, а любые legacy tool names возвращают `UNKNOWN_TOOL`.
Для миграции используйте `system op=migration.lookup` и таблицу в `V1_MIGRATION.md`.
Для совместимости некоторых MCP‑клиентов сервер может принимать namespace‑префиксы в имени
инструмента (например, `branchmind.status` / `branchmind/status`).
Также некоторые клиенты отправляют `"arguments": null` для вызовов без аргументов — сервер
трактует `missing|null` как `{}` (но сохраняет не‑object значения как есть, чтобы валидаторы
инструментов возвращали точный `INVALID_INPUT`).

## Surface = 10

В `tools/list` всегда возвращаются только:

1. `status`
2. `open`
3. `workspace`
4. `tasks`
5. `jobs`
6. `think`
7. `graph`
8. `vcs`
9. `docs`
10. `system`

## cmd‑first

Любая операция адресуется через стабильный `cmd` формата `domain.verb[.subverb]`.

- Golden‑ops доступны через `op` (см. `V1_COMMANDS.md`).
- Long‑tail всегда через `op="call" + cmd`.

## actions‑first

Единственный механизм “что дальше” — `actions[]`.

- `suggestions[]` в v1 всегда `[]`.
- Любой `INVALID_INPUT` возвращает минимум 2 действия:
  - `system` → `schema.get(cmd)`
  - `*_ops` → минимальный валидный пример вызова

## Budget profiles

`budget_profile`: `portal | default | audit`.

- Профиль задаёт **жёсткие caps** для вывода и scope‑бюджетов.
- Профиль используется для `status`, `open`, `system schema.get` и всех ops‑порталов (`workspace`/`tasks`/`jobs`/`think`/`graph`/`vcs`/`docs`/`system`).

## NextEngine (единый “что дальше”)

`status` и `tasks` (`execute.next`) используют одну реализацию `NextEngine::derive(...)`.

## Документы v1

- `V1_COMMANDS.md` — cmd registry + golden ops
- `V1_MIGRATION.md` — маппинг старых имён на `cmd`
- `DELEGATION.md` — multi‑executor delegation
- `TYPES.md` — envelope v1
