# Slice-2 (jobs handlers)

## Slice
2. Внедрить guardrails в `jobs.complete` и fallback/preview в `jobs.artifact.get` + `jobs.open(include_artifacts)`.

## Scope
- `crates/mcp/src/handlers/tasks/jobs/terminal_ops/complete.rs`
- `crates/mcp/src/handlers/tasks/jobs/artifact_ops.rs`
- `crates/mcp/src/handlers/tasks/jobs/queue_ops/open.rs`
- `crates/mcp/src/handlers/tasks/definitions/jobs.rs`

## Цель slice
- Для pipeline jobs DONE с `expected_artifacts` сделать materialize в `job_artifacts` и fail-closed при нарушениях.
- `jobs.artifact.get` должен уметь fallback из `job.summary` с warning.
- `jobs.open` с `include_artifacts=true` возвращает stored + expected/missing + actions.

## Шаги
1. В `complete`: читать `job_get`, парсить `meta_json.expected_artifacts`, валидировать `summary` при DONE.
2. Материализовывать артефакты через `job_artifact_create` до `job_complete`.
3. `artifact.get`: если `job_artifact_get` пуст, fallback с warn.
4. `jobs.open`: добавить `include_artifacts` + `actions` по каждому ключу.

## Тесты/чекеры
- `crates/mcp/tests/delegation_jobs.rs` + отдельный `jobs_ai_first_ux` сценарий при необходимости.

## Blockers
- Нужно использовать единый helper из Slice-1 для avoid-drift.

## Deep Review checklist
- Fail-closed `PRECONDITION_FAILED` действительно для `jobs.complete`.
- Нет silent downgrade когда expected pack невалиден.
- Actions in `jobs.open` корректны и bounded (`4000`).

## Proof
- `FILE`/`CMD` указать после merge (ссылка на тесты).
