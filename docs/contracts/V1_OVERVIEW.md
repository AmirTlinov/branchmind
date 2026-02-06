# Contracts — v1 Overview (actions-first, cmd-first)

BranchMind v1 стандартизирует MCP‑поверхность вокруг **10 инструментов**, **cmd‑registry** и
**actions‑first** UX. В v1 существует только **строгая 10‑портальная поверхность**:
`tools/list` показывает только 10 порталов, а любые другие имена инструментов возвращают
`UNKNOWN_TOOL`.
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

## Progressive disclosure (tools/list toolset lens)

`tools/list` принимает опциональный параметр:

```json
{ "toolset": "core|daily|full" }
```

Инварианты:

- **Список инструментов всегда одинаковый** (те же 10 порталов).
- Меняется только “линза раскрытия” для схем порталов:
  - `core` рекламирует только `tier=gold` ops (минимальный шум),
  - `daily` рекламирует `gold + advanced`,
  - `full` рекламирует `gold + advanced + internal`.
- Независимо от toolset, long‑tail остаётся доступен через `op="call" + cmd`.

## actions‑first

Единственный механизм “что дальше” — `actions[]`.

- `suggestions[]` в v1 всегда `[]`.
- Любой `INVALID_INPUT` возвращает минимум 2 действия:
  - `system` → `schema.get(cmd)`
  - соответствующий портал → минимальный валидный пример вызова

## Budget profiles

`budget_profile`: `portal | default | audit`.

- Профиль задаёт **жёсткие caps** для вывода и scope‑бюджетов.
- Профиль используется для `status`, `open`, `system schema.get` и всех ops‑порталов (`workspace`/`tasks`/`jobs`/`think`/`graph`/`vcs`/`docs`/`system`).

## NextEngine (единый “что дальше”)

`status` и `tasks` (`execute.next`) используют одну реализацию `NextEngine::derive(...)`.

## Документы v1

- `V1_COMMANDS.md` — cmd registry + golden ops
- `DELEGATION.md` — multi‑executor delegation
- `TYPES.md` — envelope v1
