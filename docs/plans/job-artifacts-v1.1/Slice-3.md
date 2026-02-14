# Slice-3 (open/system/contracts)

## Slice
3. Поддержать `open(artifact://jobs/JOB-*/<key>)`, строгий `system.cmd.list`, и contract drift fixes.

## Scope
- `crates/mcp/src/handlers/branchmind/core/open/*`
- `crates/mcp/src/ops/system/register.rs`
- `crates/mcp/src/ops/system/handlers.rs`
- `crates/mcp/src/handlers/branchmind/definitions/core.rs`
- `crates/mcp/src/ops/jobs/mod.rs`
- `docs/contracts/V1_COMMANDS.md`

## Цель slice
- Реальное чтение артефактов через open без anchor-binding.
- `system.cmd.list` schema + strict unknown args.
- Документы и discovery корректно показывают новые поля и поведение.

## Шаги
1. Добавить новый kind `job_artifact` и роутинг в `open`.
2. Добавить `q` в `system.cmd.list` schema, обработчик fail-closed неизвестных args.
3. Внести include_artifacts в docs/schema и include_content в `open` tool schema.
4. В `ops/jobs/mod.rs` выставить `jobs.complete` anchor `#jobs.complete`.

## Тесты/чекеры
- `system.cmd.list`/`system.schema.get` contract checks.
- Тесты `crates/mcp/tests/*` на `open(artifact://...)` и strict unknown args.

## Blockers
- Нужно убедиться, что и `open` v1 и v2 tool definitions не конфликтуют.

## Deep Review checklist
- `open` не должен требовать anchor binding для artifact URL.
- `open` не возвращает `UNKNOWN_ID` для валидного fallback from summary.
- Strict unknown args не ломает легитимные вызовы с envelope-keys.

## Proof
- `FILE`/`CMD` после выполнения.
