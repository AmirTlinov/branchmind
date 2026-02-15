#![forbid(unsafe_code)]

use crate::ops::ToolName;
use serde_json::{Value, json};

/// Default portal used in discovery surfaces (help/tools.list).
pub(crate) const QUICKSTART_DEFAULT_PORTAL: &str = "tasks";

/// Portals that have curated quickstart recipes (copy/paste + actions[]).
pub(crate) fn quickstart_curated_portals() -> &'static [&'static str] {
    &[
        "status",
        "open",
        "tasks",
        "jobs",
        "workspace",
        "think",
        "graph",
        "vcs",
        "docs",
        "system",
    ]
}

pub(crate) fn quickstart_curated_portals_joined() -> String {
    quickstart_curated_portals().join("|")
}

pub(crate) fn quickstart_example_env(workspace: Option<&str>, portal: &str) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(ws) = workspace {
        obj.insert("workspace".to_string(), Value::String(ws.to_string()));
    }
    obj.insert("op".to_string(), Value::String("quickstart".to_string()));
    obj.insert("args".to_string(), json!({ "portal": portal }));
    obj.insert(
        "budget_profile".to_string(),
        Value::String("portal".to_string()),
    );
    obj.insert(
        "portal_view".to_string(),
        Value::String("compact".to_string()),
    );
    Value::Object(obj)
}

pub(crate) struct QuickstartRecipe {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) purpose: &'static str,
    pub(crate) tool: ToolName,
    pub(crate) args: Value,
    pub(crate) risk: &'static str,
    /// Which quickstart result defaults this recipe relies on (for UI badges).
    pub(crate) uses_defaults: &'static [&'static str],
}

pub(crate) fn quickstart_recipes_for_portal(
    portal_tool: ToolName,
    workspace: Option<&str>,
    checkout_branch: Option<&str>,
    default_branch: &str,
) -> Vec<QuickstartRecipe> {
    let default_branch = default_branch.trim();
    let checkout_branch = checkout_branch.unwrap_or(default_branch).trim();

    let portal_env = |op: &str, cmd: Option<&str>, args: Value| -> Value {
        let mut obj = serde_json::Map::new();
        if let Some(ws) = workspace {
            obj.insert("workspace".to_string(), Value::String(ws.to_string()));
        }
        obj.insert("op".to_string(), Value::String(op.to_string()));
        if let Some(cmd) = cmd {
            obj.insert("cmd".to_string(), Value::String(cmd.to_string()));
        }
        obj.insert("args".to_string(), args);
        obj.insert(
            "budget_profile".to_string(),
            Value::String("portal".to_string()),
        );
        obj.insert(
            "portal_view".to_string(),
            Value::String("compact".to_string()),
        );
        Value::Object(obj)
    };

    let direct_env = |base: Value| -> Value {
        let mut obj = base.as_object().cloned().unwrap_or_default();
        if let Some(ws) = workspace {
            obj.entry("workspace".to_string())
                .or_insert(Value::String(ws.to_string()));
        }
        obj.entry("budget_profile".to_string())
            .or_insert(Value::String("portal".to_string()));
        obj.entry("portal_view".to_string())
            .or_insert(Value::String("compact".to_string()));
        Value::Object(obj)
    };

    let mut recipes = Vec::<QuickstartRecipe>::new();
    let mut push = |id: &'static str,
                    title: &'static str,
                    purpose: &'static str,
                    tool: ToolName,
                    args: Value,
                    risk: &'static str,
                    uses_defaults: &'static [&'static str]| {
        recipes.push(QuickstartRecipe {
            id,
            title,
            purpose,
            tool,
            args,
            risk,
            uses_defaults,
        });
    };

    match portal_tool {
        ToolName::Status => {
            push(
                "status",
                "Status",
                "Workspace status + next actions (NextEngine).",
                ToolName::Status,
                direct_env(json!({})),
                "Низкий",
                &[],
            );
            push(
                "status-audit",
                "Status (audit view)",
                "Same status call, but with a larger output budget.",
                ToolName::Status,
                direct_env(json!({ "view": "audit" })),
                "Низкий",
                &[],
            );
            push(
                "status-compact",
                "Status (compact view)",
                "Minimal status view when you need the current selection only.",
                ToolName::Status,
                direct_env(json!({ "view": "compact" })),
                "Низкий",
                &[],
            );
        }
        ToolName::Open => {
            push(
                "open-anchor",
                "Open anchor (safe)",
                "Open an anchor snapshot (works even if unregistered; may be empty).",
                ToolName::Open,
                direct_env(json!({ "id": "a:core", "limit": 20 })),
                "Низкий",
                &[],
            );
            push(
                "open-anchor-drafts",
                "Open anchor (include drafts)",
                "Same anchor open, but include draft-lane cards.",
                ToolName::Open,
                direct_env(json!({ "id": "a:core", "limit": 20, "include_drafts": true })),
                "Низкий",
                &[],
            );
            push(
                "open-anchor-content",
                "Open anchor (with content)",
                "Same safe anchor open, but include common content blocks.",
                ToolName::Open,
                direct_env(json!({ "id": "a:core", "include_content": true, "limit": 20 })),
                "Низкий",
                &[],
            );
        }
        ToolName::TasksOps => {
            push(
                "macro-start",
                "Create a task (macro)",
                "Start a task using a minimal template.",
                ToolName::TasksOps,
                portal_env(
                    "call",
                    Some("tasks.macro.start"),
                    json!({ "task_title": "First task", "template": "basic-task" }),
                ),
                "Низкий",
                &[],
            );
            push(
                "snapshot",
                "Unified snapshot",
                "Refresh focus snapshot and get the next step.",
                ToolName::TasksOps,
                portal_env("call", Some("tasks.snapshot"), json!({ "view": "smart" })),
                "Низкий",
                &[],
            );
            push(
                "exec-summary-critical-regressions",
                "Exec summary + critical regressions",
                "One-command preset: execution summary + critical regressions from lint.",
                ToolName::TasksOps,
                portal_env("call", Some("tasks.exec.summary"), json!({})),
                "Низкий",
                &[],
            );
            push(
                "search",
                "Search tasks/plans",
                "Find TASK-* / PLAN-* by text and get openable ids.",
                ToolName::TasksOps,
                portal_env("search", None, json!({ "text": "quota", "limit": 12 })),
                "Низкий",
                &[],
            );
            push(
                "execute-next",
                "NextEngine (tasks)",
                "Compute the next best actions for the current focus.",
                ToolName::TasksOps,
                portal_env("execute.next", None, json!({})),
                "Низкий",
                &[],
            );
        }
        ToolName::JobsOps => {
            push(
                "exec-summary",
                "Exec summary (minimal)",
                "One-command teamlead pulse: what matters now + critical regressions + next actions.",
                ToolName::JobsOps,
                portal_env("exec.summary", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "control-center",
                "Control center",
                "Full control center (deep dive): inbox + execution/proof health + mesh + action-pack.",
                ToolName::JobsOps,
                portal_env(
                    "call",
                    Some("jobs.control.center"),
                    json!({ "view": "smart", "limit": 50 }),
                ),
                "Низкий",
                &[],
            );
            push(
                "rotate-stalled",
                "Rotate stalled",
                "Rotate stalled RUNNING jobs (cancel + recreate) with one macro.",
                ToolName::JobsOps,
                portal_env(
                    "macro.rotate.stalled",
                    None,
                    json!({ "stall_after_s": 600, "limit": 5 }),
                ),
                "Низкий",
                &[],
            );
            push(
                "radar",
                "Jobs radar",
                "Low-noise overview of queued/running jobs + attention signals.",
                ToolName::JobsOps,
                portal_env("radar", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "dispatch-slice",
                "Dispatch slice",
                "Create one execution slice/job with routing metadata (intent-only macro).",
                ToolName::JobsOps,
                portal_env(
                    "macro.dispatch.slice",
                    None,
                    json!({
                        "title": "Quick slice",
                        "prompt": "Run make check and summarize the result."
                    }),
                ),
                "Низкий",
                &[],
            );
            push(
                "runner-start",
                "Start runner",
                "Start the local runner if it's offline.",
                ToolName::JobsOps,
                portal_env("runner.start", None, json!({})),
                "Низкий",
                &[],
            );
        }
        ToolName::WorkspaceOps => {
            push(
                "list",
                "List workspaces",
                "Show known workspaces + bound_path (transparent path→id bindings).",
                ToolName::WorkspaceOps,
                portal_env("list", None, json!({ "limit": 50 })),
                "Низкий",
                &[],
            );
            push(
                "use-current",
                "Use current workspace",
                "Pin the currently selected workspace explicitly for this session.",
                ToolName::WorkspaceOps,
                portal_env("use", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "reset",
                "Reset override",
                "Clear workspace override and return to default/auto workspace.",
                ToolName::WorkspaceOps,
                portal_env("reset", None, json!({})),
                "Низкий",
                &[],
            );
        }
        ToolName::ThinkOps => {
            push(
                "seed",
                "Seed reasoning card",
                "Create a typed reasoning card (hypothesis/question/test/evidence/decision).",
                ToolName::ThinkOps,
                portal_env("reasoning.seed", None, json!({ "type": "hypothesis" })),
                "Низкий",
                &[],
            );
            push(
                "sequential-step",
                "Sequential trace checkpoint",
                "Record one structured hypothesis→test→evidence→decision checkpoint (strict-friendly default discipline).",
                ToolName::ThinkOps,
                portal_env(
                    "call",
                    Some("think.trace.sequential.step"),
                    json!({
                        "thought": "Checkpoint: hypothesis/test/evidence/decision status.",
                        "thoughtNumber": 1,
                        "totalThoughts": 1,
                        "nextThoughtNeeded": false,
                        "meta": {
                            "checkpoint": "gate",
                            "hypothesis": "Current patch should satisfy gate.",
                            "test": "Run make check and inspect first failure.",
                            "evidence": "Attach concise command output or artifact ref.",
                            "decision": "Proceed with minimal corrective patch or stop."
                        }
                    }),
                ),
                "Низкий",
                &[],
            );
            push(
                "atlas-suggest",
                "Suggest atlas bindings",
                "Propose 10–30 directory-based anchors (uses repo_root='.' by default).",
                ToolName::ThinkOps,
                portal_env(
                    "call",
                    Some("think.atlas.suggest"),
                    json!({ "repo_root": ".", "granularity": "depth2", "limit": 30 }),
                ),
                "Низкий",
                &[],
            );
            push(
                "anchor-resolve",
                "Resolve anchor for path",
                "Jump from code path to the best matching anchor binding (atlas).",
                ToolName::ThinkOps,
                portal_env(
                    "call",
                    Some("think.anchor.resolve"),
                    json!({ "path": "crates/mcp/src", "limit": 12 }),
                ),
                "Низкий",
                &[],
            );
            push(
                "atlas-bindings-list",
                "List atlas bindings",
                "Inspect the path → anchor index (make the “jump magic” fully transparent).",
                ToolName::ThinkOps,
                portal_env(
                    "call",
                    Some("think.atlas.bindings.list"),
                    json!({ "prefix": "crates", "limit": 50 }),
                ),
                "Низкий",
                &[],
            );
            push(
                "pipeline",
                "Reasoning pipeline",
                "Create hypothesis→question→test→evidence→decision chain in one call.",
                ToolName::ThinkOps,
                portal_env(
                    "reasoning.pipeline",
                    None,
                    json!({
                        "title": "Pipeline from quickstart",
                        "hypothesis": "Current approach will pass make check.",
                        "question": "What can fail first?",
                        "test": "Run make check and inspect first red.",
                        "evidence": "Attach failing output snippet.",
                        "decision": "Pick minimal patch and re-run verify."
                    }),
                ),
                "Низкий",
                &[],
            );
        }
        ToolName::GraphOps => {
            push(
                "query",
                "Graph query",
                "Query current graph view (defaults to checkout branch/doc).",
                ToolName::GraphOps,
                portal_env("query", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "apply",
                "Graph apply (upsert)",
                "Apply a small graph mutation (writes).",
                ToolName::GraphOps,
                portal_env(
                    "apply",
                    None,
                    json!({
                        "ops": [
                            { "op": "node_upsert", "id": "seed", "type": "idea", "title": "Seed" }
                        ]
                    }),
                ),
                "Средний",
                &[],
            );
            push(
                "merge",
                "Graph merge (dry-run)",
                "Preview a merge back (checkout → default; using defaults.checkout_branch/default_branch; safe by default).",
                ToolName::GraphOps,
                portal_env(
                    "merge",
                    None,
                    json!({
                        "from": checkout_branch,
                        "into": default_branch,
                        "dry_run": true,
                        "limit": 50
                    }),
                ),
                "Низкий",
                &["checkout_branch", "default_branch"],
            );
        }
        ToolName::VcsOps => {
            push(
                "branch-list",
                "List branches",
                "List known branches for this workspace.",
                ToolName::VcsOps,
                portal_env("call", Some("vcs.branch.list"), json!({ "limit": 50 })),
                "Низкий",
                &[],
            );
            push(
                "log",
                "Log (current checkout)",
                "Show commit/log info for the current checkout branch (bounded).",
                ToolName::VcsOps,
                portal_env("call", Some("vcs.log"), json!({ "limit": 20 })),
                "Низкий",
                &[],
            );
            push(
                "checkout-default",
                "Checkout default branch",
                "Set checkout branch to the workspace default branch (using defaults.default_branch; safe default).",
                ToolName::VcsOps,
                portal_env(
                    "call",
                    Some("vcs.checkout"),
                    json!({ "ref": default_branch }),
                ),
                "Низкий",
                &["default_branch"],
            );
            push(
                "branch-create",
                "Create a branch",
                "Create a new branch (customize name).",
                ToolName::VcsOps,
                portal_env(
                    "branch.create",
                    None,
                    json!({ "name": "feature/quickstart" }),
                ),
                "Низкий",
                &[],
            );
        }
        ToolName::DocsOps => {
            push(
                "list",
                "List docs",
                "List docs available on the current checkout.",
                ToolName::DocsOps,
                portal_env("list", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "show-notes",
                "Show notes tail",
                "Show tail of notes doc (bounded).",
                ToolName::DocsOps,
                portal_env("show", None, json!({ "doc_kind": "notes", "limit": 20 })),
                "Низкий",
                &[],
            );
            push(
                "diff-noop",
                "Diff (safe no-op)",
                "Example diff call (default → checkout; using defaults.checkout_branch/default_branch; customize branches).",
                ToolName::DocsOps,
                portal_env(
                    "diff",
                    None,
                    json!({
                        "from": default_branch,
                        "to": checkout_branch,
                        "doc": "notes",
                        "limit": 20
                    }),
                ),
                "Низкий",
                &["checkout_branch", "default_branch"],
            );
            push(
                "merge-dry-run",
                "Merge (dry-run)",
                "Preview merge (checkout → default; using defaults.checkout_branch/default_branch; safe by default; customize from/into).",
                ToolName::DocsOps,
                portal_env(
                    "merge",
                    None,
                    json!({
                        "from": checkout_branch,
                        "into": default_branch,
                        "doc": "notes",
                        "dry_run": true,
                        "limit": 50
                    }),
                ),
                "Низкий",
                &["checkout_branch", "default_branch"],
            );
        }
        ToolName::SystemOps => {
            push(
                "tools",
                "List v1 surface",
                "Show the 10 portal tools and their golden ops.",
                ToolName::SystemOps,
                portal_env("tools.list", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "tutorial",
                "Tutorial",
                "Guided onboarding (golden path).",
                ToolName::SystemOps,
                portal_env("tutorial", None, json!({ "limit": 3 })),
                "Низкий",
                &[],
            );
            push(
                "schema-list",
                "List schemas (tasks)",
                "Discover cmds for the tasks portal (bounded).",
                ToolName::SystemOps,
                portal_env(
                    "schema.list",
                    None,
                    json!({ "portal": "tasks", "limit": 20 }),
                ),
                "Низкий",
                &[],
            );
            push(
                "exec-summary",
                "Exec summary + critical regressions",
                "One-command cross-portal summary (tasks + jobs) with critical regressions.",
                ToolName::SystemOps,
                portal_env("exec.summary", None, json!({})),
                "Низкий",
                &[],
            );
            push(
                "ops-summary",
                "Ops summary",
                "Surface drift guard (cmd counts, golden ops, unplugged ops).",
                ToolName::SystemOps,
                portal_env("ops.summary", None, json!({})),
                "Низкий",
                &[],
            );
        }
    }

    recipes
}
