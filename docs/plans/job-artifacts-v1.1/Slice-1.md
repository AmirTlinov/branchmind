# Slice-1 (Storage + shared helper)

## Slice
1. Ввести shared-helper `crates/mcp/src/support/job_artifacts.rs` для:
   - парсинга `artifact://jobs/...`
   - resolve + validate + fallback
   - canonical JSON + ограничения по `job_id/artifact_key`

## Scope
- `crates/mcp/src/support/job_artifacts.rs` (создание/доработка)
- `crates/storage/src/store/jobs/artifacts.rs`
- `crates/storage/src/store/types/jobs.rs`

## Цель slice
- Добавить `job_artifacts_list` с `limit`.
- Реализовать единый путь чтения артефактов из store с fallback из summary.

## Шаги
1. Добавить `JobArtifactsListRequest.limit`, SQL с `LIMIT`.
2. Завершить/дополнить helper:
   - `parse_job_artifact_ref`
   - `resolve_job_artifact_text`
   - `validate_by_artifact_key`
3. Убедиться, что контрактные ошибки возвращают `Value` с кодами `INVALID_INPUT`/`PRECONDITION_FAILED` как нужно.

## Тесты/чекеры
- Новые unit-тесты в `crates/storage/tests/job_artifacts.rs` для `job_artifacts_list(limit)`.
- Мок/интеграционный тест helper через MCP: fallback из summary + warn.

## Blockers
- Нужна синхронизация с текущими импортами в `crates/mcp/src/support/mod.rs`.

## Deep Review checklist
- Все ли ошибки мапятся в строгие коды?
- Нет ли скрытых обходов для невалидного JSON pack?
- Корректно ли применяются ограничения `max_chars` и `max_events` на чтение?

## Proof
- `CMD`/`FILE` после реализации (см. `Slice-2`).
