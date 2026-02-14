# PLAN: job-artifacts-v1.1

## Цель
Сделать фичу job-артефактов правдоподобной: `artifact://jobs/JOB-*/<key>` должен реально читать материализованный артефакт или корректный fallback из summary, `jobs.complete` должен валидировать и записывать ожидаемые артефакты, а discovery должен быть честным (без скрытых/устаревших полей).

## Контекст и ограничения
- Ограничение: контракт-first, без изменения схем хранения ядра (SQLite схема уже есть).
- Ограничение: изменения только в MCP/Sсторидже и контрактных доках по регламенту.
- Ограничение: для pipeline jobs применяем fail-closed только при `status=DONE` и наличии `expected_artifacts`.
- Запрещено: добавлять новые внешние крейтов/таблицы.

## Слайды
1. `Slice-1` — Storage + shared helper: единая логика чтения/валидации артефактов и список метаданных.
2. `Slice-2` — Job terminal handlers (`jobs.complete`, `jobs.artifact.get`, `jobs.open`) с include_artifacts и actions.
3. `Slice-3` — Open handler/ системные контракты/open discovery и docs + тесты.

## Definition of Done
- Все три слайда выполнены и проверены тестами из списка ниже.
- `make check` и целевые тесты проходят.
- `docs/contracts/V1_COMMANDS.md` и схемы отражают реально поддерживаемое поведение.

## Риски и rollback
- Риск: регрессии из-за строгой проверки `jobs.complete` для pipeline jobs → rollback: восстановить pre-change в `tool_tasks_jobs_complete`.
- Риск: изменение поведения `jobs.artifact.get` на fallback может скрыть реальные ошибки создания -> rollback: убрать fallback и требовать только store.
- Риск: `open(artifact://...)` ломает routing старых `TASK/PLAN` id -> rollback: проверка по строгому префиксу через `parse_job_artifact_ref`.

## Валидирующий конвейер
- Unit/integration: `cargo test -p bm_storage job_artifacts*`, `cargo test -p bm_mcp open_job_artifact...`, `cargo test -p bm_mcp jobs_complete...`, `cargo test -p bm_mcp delegation_jobs::tasks_jobs_complete*`.
- Regression: `cargo test -p bm_mcp schema` и `jobs.ai_first` сценарии.

## Привязка к слайсам
- Slice-1: `Slice-1.md`
- Slice-2: `Slice-2.md`
- Slice-3: `Slice-3.md`
