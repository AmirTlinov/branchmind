#![forbid(unsafe_code)]

use crate::WorkspaceId;
use crate::ops::{Action, ActionPriority, ToolName};
use serde_json::json;

#[derive(Clone, Debug)]
pub(crate) struct NextEngineReport {
    pub(crate) headline: String,
    pub(crate) refs: Vec<String>,
    pub(crate) actions: Vec<Action>,
    pub(crate) state_fingerprint: String,
    pub(crate) focus_id: Option<String>,
    pub(crate) checkout: Option<String>,
}

/// NextEngine v1 (minimal viable):
/// - deterministic,
/// - low-noise,
/// - action-first.
///
/// We intentionally keep this conservative at first:
/// - show *navigation* actions (`open`, `tasks.snapshot`) before mutating actions,
/// - avoid deep heuristics (they belong in domain signals / lint engines).
pub(crate) fn derive_next(
    server: &mut crate::McpServer,
    workspace: &WorkspaceId,
) -> NextEngineReport {
    let focus_id = server.store.focus_get(workspace).ok().flatten();
    let checkout = server.store.branch_checkout_get(workspace).ok().flatten();

    // Delegation health (persisted): queue + runner status.
    // This stays low-noise and deterministic; we only use it to gate a small number of actions.
    let inbox = server.store.jobs_status_counts(workspace).ok();
    let queued = inbox.as_ref().map(|c| c.queued).unwrap_or(0);
    let running = inbox.as_ref().map(|c| c.running).unwrap_or(0);

    let now_ms = crate::support::now_ms_i64();
    let runner_status = server.store.runner_status_snapshot(workspace, now_ms).ok();
    let runner_is_offline = runner_status
        .as_ref()
        .is_some_and(|s| s.status.as_str() == "offline");

    // Best-effort DX: if jobs are queued and the runner is offline, we may auto-start the
    // first-party runner. This is allowed by the repo rules (first-party only) and guarded by:
    // - explicit config flag (runner_autostart_enabled)
    // - per-workspace backoff (in-memory, to avoid repeated spawns on status refresh)
    //
    // UX still surfaces an explicit "runner.start" action so agents never get stuck.
    let _runner_autostart_active =
        server.maybe_autostart_runner(workspace, now_ms, queued as usize, runner_is_offline);

    let state_fingerprint = format!(
        "ws={};focus={};checkout={};jobs(q={},r={});runner={}",
        workspace.as_str(),
        focus_id.as_deref().unwrap_or("-"),
        checkout.as_deref().unwrap_or("-"),
        queued,
        running,
        runner_status
            .as_ref()
            .map(|s| s.status.as_str())
            .unwrap_or("-"),
    );

    let mut refs = Vec::<String>::new();
    if let Some(focus) = focus_id.as_ref() {
        refs.push(focus.clone());
    }

    let mut actions = Vec::<Action>::new();
    if let Some(focus) = focus_id.as_ref() {
        actions.push(Action {
            action_id: "next::open.focus".to_string(),
            priority: ActionPriority::High,
            tool: ToolName::Open.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "id": focus,
                "include_content": true,
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Открыть текущий focus (быстрый контекст).".to_string(),
            risk: "Низкий".to_string(),
        });

        // Delegation portal UX: when jobs are queued but no runner lease is active, provide a
        // high-signal "start runner" button (golden op). This is safe and idempotent.
        if queued > 0 && runner_is_offline {
            actions.push(Action {
                action_id: "next::runner.start".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "runner.start",
                    "args": {},
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Запустить bm_runner, чтобы JOB-* начали исполняться.".to_string(),
                risk: "Низкий (first-party runner only).".to_string(),
            });
        }

        actions.push(Action {
            action_id: "next::tasks.snapshot".to_string(),
            priority: ActionPriority::Medium,
            tool: ToolName::TasksOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "call",
                "cmd": "tasks.snapshot",
                "args": { "view": "smart" },
                "budget_profile": "portal"
            }),
            why: "Показать snapshot (focus + next signals).".to_string(),
            risk: "Низкий".to_string(),
        });

        if queued > 0 || running > 0 {
            actions.push(Action {
                action_id: "next::jobs.radar".to_string(),
                priority: ActionPriority::Low,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "radar",
                    "args": {},
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Показать делегацию (очередь, раннеры, последние события).".to_string(),
                risk: "Низкий".to_string(),
            });
        }
        actions.push(Action {
            action_id: "next::think.knowledge.recall".to_string(),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "knowledge.recall",
                "args": { "limit": 12 },
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Подтянуть самые свежие knowledge cards (bounded) перед решением.".to_string(),
            risk: "Низкий".to_string(),
        });
        NextEngineReport {
            headline: "Focus set: review snapshot then take the next smallest action.".to_string(),
            refs,
            actions,
            state_fingerprint,
            focus_id,
            checkout,
        }
    } else {
        let tasks_count = server.store.count_tasks(workspace).ok().unwrap_or(0);

        if queued > 0 && runner_is_offline {
            actions.push(Action {
                action_id: "next::runner.start".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "runner.start",
                    "args": {},
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Запустить bm_runner, чтобы JOB-* начали исполняться.".to_string(),
                risk: "Низкий (first-party runner only).".to_string(),
            });
        }

        if tasks_count <= 0 {
            // Flagship UX invariant: NextEngine actions must be runnable without placeholders.
            // When the workspace is truly empty, guide the agent to create the first task via
            // the macro (sets focus, creates reasoning refs lazily, emits events).
            actions.push(Action {
                action_id: "next::tasks.macro.start".to_string(),
                priority: ActionPriority::High,
                tool: ToolName::TasksOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "call",
                    "cmd": "tasks.macro.start",
                    "args": { "task_title": "First task" },
                    "budget_profile": "default",
                    "view": "compact"
                }),
                why: "Создать первую задачу и установить focus (golden start).".to_string(),
                risk: "Низкий (создаёт новую задачу).".to_string(),
            });
        } else {
            // No focus, but tasks exist. Prefer opening snapshots of the first couple tasks so
            // the agent can select a target without expanding into full context.
            let tasks = server
                .store
                .list_tasks(workspace, 2, 0)
                .ok()
                .unwrap_or_default();
            if tasks.is_empty() {
                actions.push(Action {
                    action_id: "next::tasks.context".to_string(),
                    priority: ActionPriority::High,
                    tool: ToolName::TasksOps.as_str().to_string(),
                    args: json!({
                        "workspace": workspace.as_str(),
                        "op": "call",
                        "cmd": "tasks.context",
                        "args": {},
                        "budget_profile": "default",
                        "view": "compact"
                    }),
                    why: "Показать планы и задачи, чтобы выбрать focus.".to_string(),
                    risk: "Низкий".to_string(),
                });
            } else {
                for (idx, row) in tasks.iter().enumerate() {
                    let priority = if idx == 0 {
                        ActionPriority::High
                    } else {
                        ActionPriority::Medium
                    };
                    actions.push(Action {
                        action_id: format!("next::tasks.snapshot.{}", row.id.as_str()),
                        priority,
                        tool: ToolName::TasksOps.as_str().to_string(),
                        args: json!({
                            "workspace": workspace.as_str(),
                            "op": "call",
                            "cmd": "tasks.snapshot",
                            "args": { "task": row.id.as_str(), "view": "smart" },
                            "budget_profile": "portal"
                        }),
                        why: "Открыть snapshot для выбора focus.".to_string(),
                        risk: "Низкий".to_string(),
                    });
                }

                // Optional: if the workspace is active, let the agent see the full chooser.
                actions.push(Action {
                    action_id: "next::tasks.context".to_string(),
                    priority: ActionPriority::Low,
                    tool: ToolName::TasksOps.as_str().to_string(),
                    args: json!({
                        "workspace": workspace.as_str(),
                        "op": "call",
                        "cmd": "tasks.context",
                        "args": {},
                        "budget_profile": "default",
                        "view": "compact"
                    }),
                    why: "Показать полный список планов/задач (если нужно).".to_string(),
                    risk: "Низкий".to_string(),
                });

                actions.push(Action {
                    action_id: "next::tasks.macro.start".to_string(),
                    priority: ActionPriority::Low,
                    tool: ToolName::TasksOps.as_str().to_string(),
                    args: json!({
                        "workspace": workspace.as_str(),
                        "op": "call",
                        "cmd": "tasks.macro.start",
                        "args": { "task_title": "New task" },
                        "budget_profile": "default",
                        "view": "compact"
                    }),
                    why: "Создать новую задачу (если текущие не подходят).".to_string(),
                    risk: "Низкий (создаёт новую задачу).".to_string(),
                });
            }
        }

        if queued > 0 || running > 0 {
            actions.push(Action {
                action_id: "next::jobs.radar".to_string(),
                priority: ActionPriority::Low,
                tool: ToolName::JobsOps.as_str().to_string(),
                args: json!({
                    "workspace": workspace.as_str(),
                    "op": "radar",
                    "args": {},
                    "budget_profile": "portal",
                    "view": "compact"
                }),
                why: "Показать делегацию (очередь, раннеры, последние события).".to_string(),
                risk: "Низкий".to_string(),
            });
        }

        actions.push(Action {
            action_id: "next::think.knowledge.recall".to_string(),
            priority: ActionPriority::Low,
            tool: ToolName::ThinkOps.as_str().to_string(),
            args: json!({
                "workspace": workspace.as_str(),
                "op": "knowledge.recall",
                "args": { "limit": 12 },
                "budget_profile": "portal",
                "view": "compact"
            }),
            why: "Подтянуть самые свежие knowledge cards (bounded) перед решением.".to_string(),
            risk: "Низкий".to_string(),
        });
        NextEngineReport {
            headline: "No focus: pick a task snapshot (or create the first task) to begin."
                .to_string(),
            refs,
            actions,
            state_fingerprint,
            focus_id,
            checkout,
        }
    }
}
