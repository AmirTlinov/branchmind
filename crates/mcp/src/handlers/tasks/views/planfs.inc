use crate::*;
use bm_storage::DocAppendRequest;
use bm_storage::{NewStep, StepDefineRequest, StepPatch, StepSelector};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::support::planfs::{
    looks_like_placeholder, parse_plan_with_front_matter, parse_slice_with_front_matter,
    render_plan_markdown, render_slice_markdown, PlanFsBudgets, PlanFsDod, PlanFsPlan,
    PlanFsReadLimits, PlanFsSectionBundle, PlanFsSlice, PlanFsSliceRef, PlanFsStep, PlanFsTask,
    MAX_SLICE_TASKS, MAX_STEP_LIST_LEN, MIN_SLICE_TASKS, MIN_STEP_LIST_LEN,
};

const PLAN_FILE_NAME: &str = "PLAN.md";
const PLAN_SPEC_DOC_PREFIX: &str = "plan_spec:";
const IMPLICIT_ENVELOPE_KEYS: &[&str] = &["context_budget", "limit", "max_chars", "agent_id"];

#[derive(Clone, Copy)]
enum ExportMode {
    Init,
    Export,
}

#[derive(Clone)]
struct ImportLimits {
    parse: PlanFsReadLimits,
    max_tasks_per_slice: usize,
    max_steps_per_task: usize,
}

#[derive(Clone)]
struct PlanFsTree {
    plan: PlanFsPlan,
    slices: Vec<PlanFsSlice>,
}

#[derive(Default)]
struct WriteSummary {
    created: Vec<String>,
    updated: Vec<String>,
    unchanged: Vec<String>,
}

struct RenderedPlanFs {
    plan: String,
    slices: Vec<(String, String)>,
    slice_files: Vec<String>,
}

struct ApplyStats {
    root_created: usize,
    root_updated: usize,
    child_created: usize,
    child_updated: usize,
    unchanged: usize,
}

impl McpServer {
    pub(crate) fn tool_tasks_planfs_init(&mut self, args: Value) -> Value {
        self.handle_planfs_export_like(args, ExportMode::Init)
    }

    pub(crate) fn tool_tasks_planfs_export(&mut self, args: Value) -> Value {
        self.handle_planfs_export_like(args, ExportMode::Export)
    }

    pub(crate) fn tool_tasks_planfs_import(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        if let Err(err) = check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "plan",
                "target",
                "slug",
                "path",
                "strict",
                "apply",
                "max_slices",
                "max_items_per_list",
                "max_tasks_per_slice",
                "max_steps_per_task",
                "max_file_bytes",
            ],
            "tasks.planfs.import",
        ) {
            return err;
        }

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, kind, _focus) = match resolve_target_id(&mut self.store, &workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !matches!(kind, TaskKind::Task) {
            return ai_error(
                "INVALID_INPUT",
                "tasks.planfs.import requires task target (TASK-*)",
            );
        }

        let apply = match optional_bool(args_obj, "apply") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let strict = match optional_bool(args_obj, "strict") {
            Ok(v) => v.unwrap_or(true),
            Err(resp) => return resp,
        };
        let limits = match parse_import_limits(args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let root = match workspace_repo_root(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let task = match self.store.get_task(&workspace, &task_id) {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let slug = match optional_string(args_obj, "slug") {
            Ok(Some(raw)) => {
                let raw = raw.trim();
                if raw.is_empty() {
                    return ai_error("INVALID_INPUT", "slug must not be empty");
                }
                match normalize_plan_slug(raw) {
                    Some(v) => v,
                    None => {
                        return ai_error(
                            "INVALID_INPUT",
                            "slug must use lowercase letters/digits and '-' separators",
                        );
                    }
                }
            }
            Ok(None) => slugify_task_title(&task.title),
            Err(resp) => return resp,
        };

        let (plan_dir_rel, plan_dir_abs) = match resolve_plan_dir(args_obj, &root, &slug) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let tree = match read_planfs_from_dir(&plan_dir_abs, strict, &limits) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let mut changed = json!({
            "root_created": 0,
            "root_updated": 0,
            "child_created": 0,
            "child_updated": 0,
            "unchanged": 0
        });

        if apply {
            match apply_planfs_to_task(self, &workspace, &task_id, &tree.slices, strict) {
                Ok(stats) => {
                    changed = json!({
                        "root_created": stats.root_created,
                        "root_updated": stats.root_updated,
                        "child_created": stats.child_created,
                        "child_updated": stats.child_updated,
                        "unchanged": stats.unchanged
                    });
                }
                Err(resp) => return resp,
            }
        }

        ai_ok(
            "planfs_import",
            json!({
                "workspace": workspace.as_str(),
                "task": task_id,
                "apply": apply,
                "strict": strict,
                "planfs": {
                    "slug": slug,
                    "path": plan_dir_rel,
                    "plan_file": format!("{plan_dir_rel}/{PLAN_FILE_NAME}"),
                    "slices": tree.plan.slices.len()
                },
                "budgets": {
                    "max_slices": limits.parse.max_slices,
                    "max_items_per_list": limits.parse.max_items_per_list,
                    "max_tasks_per_slice": limits.max_tasks_per_slice,
                    "max_steps_per_task": limits.max_steps_per_task,
                    "max_file_bytes": limits.parse.max_file_bytes
                },
                "changed": changed
            }),
        )
    }

    fn handle_planfs_export_like(&mut self, args: Value, mode: ExportMode) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };

        let cmd = match mode {
            ExportMode::Init => "tasks.planfs.init",
            ExportMode::Export => "tasks.planfs.export",
        };
        if let Err(err) = check_unknown_args(
            args_obj,
            &[
                "workspace",
                "task",
                "plan",
                "target",
                "slug",
                "path",
                "overwrite",
                "from_plan_spec",
                "plan_spec_branch",
                "plan_spec_doc",
            ],
            cmd,
        ) {
            return err;
        }

        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };
        let (task_id, kind, _focus) = match resolve_target_id(&mut self.store, &workspace, args_obj) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        if !matches!(kind, TaskKind::Task) {
            return ai_error(
                "INVALID_INPUT",
                "tasks.planfs.init/export require task target (TASK-*)",
            );
        }

        let overwrite = match optional_bool(args_obj, "overwrite") {
            Ok(v) => v.unwrap_or(matches!(mode, ExportMode::Export)),
            Err(resp) => return resp,
        };
        let from_plan_spec = match optional_bool(args_obj, "from_plan_spec") {
            Ok(v) => v.unwrap_or(false),
            Err(resp) => return resp,
        };
        let plan_spec_branch = match optional_string(args_obj, "plan_spec_branch") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let plan_spec_doc = match optional_string(args_obj, "plan_spec_doc") {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let root = match workspace_repo_root(&mut self.store, &workspace) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let task = match self.store.get_task(&workspace, &task_id) {
            Ok(Some(v)) => v,
            Ok(None) => return ai_error("UNKNOWN_ID", "Unknown task id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        let slug = match optional_string(args_obj, "slug") {
            Ok(Some(raw)) => {
                let raw = raw.trim();
                if raw.is_empty() {
                    return ai_error("INVALID_INPUT", "slug must not be empty");
                }
                match normalize_plan_slug(raw) {
                    Some(v) => v,
                    None => {
                        return ai_error(
                            "INVALID_INPUT",
                            "slug must use lowercase letters/digits and '-' separators",
                        );
                    }
                }
            }
            Ok(None) => slugify_task_title(&task.title),
            Err(resp) => return resp,
        };

        let (plan_dir_rel, plan_dir_abs) = match resolve_plan_dir(args_obj, &root, &slug) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let tree = if from_plan_spec {
            match load_planfs_from_latest_plan_spec(
                self,
                &workspace,
                &task_id,
                &slug,
                plan_spec_branch.as_deref(),
                plan_spec_doc.as_deref(),
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        } else {
            match build_planfs_from_task(self, &workspace, &task_id, &task, &slug) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        };

        if matches!(mode, ExportMode::Init)
            && plan_dir_abs.exists()
            && !overwrite
            && plan_dir_abs.join(PLAN_FILE_NAME).exists()
        {
            return ai_error_with(
                "PRECONDITION_FAILED",
                "planfs directory already initialized",
                Some("Use overwrite=true or choose another slug/path."),
                Vec::new(),
            );
        }

        let rendered = match render_planfs_markdown(&tree) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let write = match write_planfs_dir(&plan_dir_abs, &rendered, overwrite) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        let intent = match mode {
            ExportMode::Init => "planfs_init",
            ExportMode::Export => "planfs_export",
        };
        let plan_spec_snapshot = match persist_plan_spec_snapshot(self, &workspace, &task_id, &tree, cmd) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

        ai_ok(
            intent,
            json!({
                "workspace": workspace.as_str(),
                "task": task_id,
                "overwrite": overwrite,
                "source": if from_plan_spec { "plan_spec_doc" } else { "task_step_tree" },
                "planfs": {
                    "slug": slug,
                    "path": plan_dir_rel,
                    "plan_file": format!("{plan_dir_rel}/{PLAN_FILE_NAME}"),
                    "slice_files": rendered.slice_files,
                    "slices": tree.slices.len()
                },
                "write": {
                    "created": write.created,
                    "updated": write.updated,
                    "unchanged": write.unchanged
                },
                "plan_spec": plan_spec_snapshot
            }),
        )
    }
}

fn load_planfs_from_latest_plan_spec(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    task_id: &str,
    slug: &str,
    branch: Option<&str>,
    doc: Option<&str>,
) -> Result<PlanFsTree, Value> {
    let task_kind = match parse_plan_or_task_kind(task_id) {
        Some(v) => v,
        None => {
            return Err(ai_error(
                "INVALID_INPUT",
                "tasks.planfs.* with from_plan_spec=true requires TASK-* target",
            ));
        }
    };
    if !matches!(task_kind, TaskKind::Task) {
        return Err(ai_error(
            "INVALID_INPUT",
            "tasks.planfs.* with from_plan_spec=true requires TASK-* target",
        ));
    }

    let reasoning = match server
        .store
        .ensure_reasoning_ref(workspace, task_id, task_kind)
    {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };
    let branch = branch
        .and_then(normalize_optional_text)
        .unwrap_or(reasoning.branch);
    let doc = doc
        .and_then(normalize_optional_text)
        .unwrap_or_else(|| plan_spec_doc_for_task(task_id));

    let latest = load_latest_plan_spec_entry(server, workspace, &branch, &doc)?;
    let Some((_entry, payload)) = latest else {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "requested plan_spec doc has no entries",
            Some("Export once from task_step_tree first, then retry with from_plan_spec=true."),
            Vec::new(),
        ));
    };
    let mut tree = parse_plan_spec_payload(payload, task_id, slug)?;
    if tree.plan.plan_slug != slug {
        tree.plan.plan_slug = slug.to_string();
    }
    Ok(tree)
}

fn parse_plan_spec_payload(
    payload: Value,
    task_id: &str,
    slug: &str,
) -> Result<PlanFsTree, Value> {
    let candidate = payload
        .as_object()
        .and_then(|obj| obj.get("plan"))
        .or_else(|| payload.as_object().and_then(|obj| obj.get("planfs")))
        .cloned()
        .unwrap_or_else(|| payload.clone());

    let mut plan: PlanFsPlan = serde_json::from_value(candidate)
        .map_err(|_| ai_error_with(
            "INVALID_INPUT",
            "plan_spec payload does not contain a supported PlanFS plan",
            Some("Regenerate plan_spec via tasks.planfs.export."),
            Vec::new(),
        ))?;

    plan.plan_slug = normalize_optional_text(&plan.plan_slug).unwrap_or_else(|| slug.to_string());
    if normalize_plan_slug(&plan.plan_slug).is_none() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "plan_spec plan_slug is invalid",
            Some("Use lowercase letters/digits and '-' separators."),
            Vec::new(),
        ));
    }

    let slices = payload
        .as_object()
        .and_then(|obj| obj.get("slices"))
        .and_then(|v| v.as_array())
        .map(|raw| {
            raw.iter()
                .map(|entry| {
                    serde_json::from_value::<PlanFsSlice>(entry.clone()).map_err(|_| {
                        ai_error_with(
                            "INVALID_INPUT",
                            "plan_spec contains malformed slice spec",
                            Some("Re-export plan_spec from source task and retry."),
                            Vec::new(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, Value>>()
        })
        .transpose()?
        .unwrap_or_else(|| {
            plan.slices
                .iter()
                .map(|slice_ref| PlanFsSlice {
                    id: slice_ref.id.clone(),
                    title: slice_ref.title.clone(),
                    objective: String::new(),
                    status: slice_ref.status.clone(),
                    budgets: default_slice_budgets(),
                    dod: PlanFsDod {
                        success_criteria: vec!["No explicit DoD.".to_string()],
                        tests: vec!["make check".to_string()],
                        blockers: vec!["No blockers".to_string()],
                        rollback: vec!["Rollback this slice changes if needed.".to_string()],
                    },
                    tasks: Vec::new(),
                    sections: PlanFsSectionBundle {
                        goal: format!("Implement {}", slice_ref.title),
                        scope: vec![format!("Scope: {}.", slice_ref.title)],
                        non_goals: vec!["No changes outside task scope.".to_string()],
                        interfaces: vec!["No interface change by default.".to_string()],
                        contracts: vec!["Keep current behavior stable.".to_string()],
                        tests: vec!["make check".to_string()],
                        proof: vec![format!("CMD: tasks.planfs.import // {}", slice_ref.id)],
                        rollback: vec!["Revert slice changes.".to_string()],
                        risks: vec!["Scope drift from slice edits.".to_string()],
                    },
                })
                .collect::<Vec<_>>()
        });

    if slices.is_empty() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "plan_spec has no slices",
            Some("Export planfs once from task_step_tree first."),
            Vec::new(),
        ));
    }

    if slices.iter().any(|slice| slice.id.trim().is_empty()) {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "plan_spec contains empty slice ids",
            Some("Regenerate plan_spec or fix file references."),
            Vec::new(),
        ));
    }

    let mut refs = Vec::<PlanFsSliceRef>::new();
    for (idx, slice) in slices.iter().enumerate() {
        refs.push(PlanFsSliceRef {
            id: slice.id.clone(),
            title: slice.title.clone(),
            file: format!("Slice-{}.md", idx + 1),
            status: slice.status.clone(),
        });
        if slice.tasks.len() < MIN_SLICE_TASKS || slice.tasks.len() > MAX_SLICE_TASKS {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!(
                    "slice {} has invalid task count ({}), must be {}..{} in strict mode",
                    slice.id,
                    slice.tasks.len(),
                    MIN_SLICE_TASKS,
                    MAX_SLICE_TASKS
                ),
            ));
        }
        for task in &slice.tasks {
            if task.steps.len() < MIN_STEP_LIST_LEN || task.steps.len() > MAX_STEP_LIST_LEN {
                return Err(ai_error(
                    "INVALID_INPUT",
                    &format!(
                        "slice {} task \"{}\" has invalid step count ({}), must be {}..{} in strict mode",
                        slice.id,
                        task.title,
                        task.steps.len(),
                        MIN_STEP_LIST_LEN,
                        MAX_STEP_LIST_LEN
                    ),
                ));
            }
        }
    }

    if task_id.is_empty() {
        return Err(ai_error("INVALID_INPUT", "task id is empty in plan_spec"));
    }

    // Preserve task id out of payload if present as soft check.
    if let Some(expected_task) = payload
        .as_object()
        .and_then(|obj| obj.get("task").and_then(Value::as_str))
        && expected_task != task_id
    {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!("plan_spec task mismatch: payload has {expected_task}, requested {task_id}"),
            Some("Load plan_spec for the target task or regenerate from that task."),
            Vec::new(),
        ));
    }

    let sections = if plan.sections.goal.trim().is_empty() {
        PlanFsSectionBundle {
            goal: format!("Implement task {task_id}"),
            scope: vec!["Task-local scope.".to_string()],
            non_goals: vec!["No unrelated task edits.".to_string()],
            interfaces: vec!["No interface-level changes by default.".to_string()],
            contracts: vec!["Keep contracts stable.".to_string()],
            tests: vec!["make check".to_string()],
            proof: vec![format!("CMD: tasks.planfs.import // {task_id}")],
            rollback: vec!["Revert affected files.".to_string()],
            risks: vec!["Plan drift between plan files and task steps.".to_string()],
        }
    } else {
        plan.sections.clone()
    };

    plan.slices = refs;
    Ok(PlanFsTree { plan: PlanFsPlan { sections, ..plan }, slices })
}

fn persist_plan_spec_snapshot(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    task_id: &str,
    tree: &PlanFsTree,
    cmd: &str,
) -> Result<Value, Value> {
    let task_kind = match parse_plan_or_task_kind(task_id) {
        Some(v) => v,
        None => {
            return Err(ai_error(
                "INVALID_INPUT",
                "tasks.planfs.* requires TASK-* target for plan_spec snapshot persistence",
            ));
        }
    };
    if !matches!(task_kind, TaskKind::Task) {
        return Err(ai_error(
            "INVALID_INPUT",
            "tasks.planfs.* requires TASK-* target for plan_spec snapshot persistence",
        ));
    }

    let reasoning = match server
        .store
        .ensure_reasoning_ref(workspace, task_id, task_kind)
    {
        Ok(v) => v,
        Err(StoreError::UnknownId) => return Err(ai_error("UNKNOWN_ID", "Unknown task id")),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    let branch = reasoning.branch;
    let doc_name = plan_spec_doc_for_task(task_id);
    let payload = canonicalize_json(json!({
        "schema": "plan_spec.v1",
        "source_cmd": cmd,
        "task": task_id,
        "plan": tree.plan,
        "slices": tree.slices,
    }));

    if let Some((latest, existing_payload)) =
        load_latest_plan_spec_entry(server, workspace, &branch, &doc_name)?
        && existing_payload == payload
    {
        return Ok(json!({
            "enabled": true,
            "status": "unchanged",
            "branch": branch,
            "doc": doc_name,
            "seq": latest.seq,
            "source_cmd": cmd
        }));
    }

    let content = match serde_json::to_string_pretty(&payload) {
        Ok(v) => v,
        Err(_) => return Err(ai_error("STORE_ERROR", "failed to serialize plan_spec payload")),
    };
    let meta_json = json!({
        "schema": "plan_spec.v1",
        "source_cmd": cmd
    })
    .to_string();

    let appended = match server.store.doc_append_plan_spec(
        workspace,
        DocAppendRequest {
            branch: branch.clone(),
            doc: doc_name.clone(),
            title: Some(format!("plan_spec snapshot from {cmd}")),
            format: Some("plan_spec.v1".to_string()),
            meta_json: Some(meta_json),
            content,
        },
    ) {
        Ok(v) => v,
        Err(StoreError::InvalidInput(msg)) => return Err(ai_error("INVALID_INPUT", msg)),
        Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
    };

    Ok(json!({
        "enabled": true,
        "status": "appended",
        "branch": branch,
        "doc": doc_name,
        "seq": appended.seq,
        "source_cmd": cmd
    }))
}

fn plan_spec_doc_for_task(task_id: &str) -> String {
    format!("{PLAN_SPEC_DOC_PREFIX}{task_id}")
}

fn load_latest_plan_spec_entry(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    branch: &str,
    doc: &str,
) -> Result<Option<(bm_storage::DocEntryRow, Value)>, Value> {
    let slice = server
        .store
        .doc_show_tail(workspace, branch, doc, None, 1)
        .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
    let Some(entry) = slice.entries.into_iter().last() else {
        return Ok(None);
    };
    let Some(content) = entry.content.as_deref() else {
        return Ok(None);
    };

    let parsed: Value = serde_json::from_str(content).map_err(|_| {
        ai_error_with(
            "INVALID_INPUT",
            "latest plan_spec entry is not valid JSON",
            Some("Rewrite plan_spec entry with canonical JSON object content."),
            Vec::new(),
        )
    })?;
    let canonical = canonicalize_json(parsed);
    if !canonical.is_object() {
        return Err(ai_error_with(
            "INVALID_INPUT",
            "plan_spec content must be a JSON object",
            Some("Use canonical plan_spec object with `plan` and `slices`."),
            Vec::new(),
        ));
    }
    Ok(Some((entry, canonical)))
}

fn parse_import_limits(args_obj: &serde_json::Map<String, Value>) -> Result<ImportLimits, Value> {
    Ok(ImportLimits {
        parse: PlanFsReadLimits {
            max_file_bytes: clamp_usize(
                optional_usize(args_obj, "max_file_bytes")?,
                2_048,
                512_000,
                96_000,
            ),
            max_slices: clamp_usize(optional_usize(args_obj, "max_slices")?, 1, 128, 24),
            max_items_per_list: clamp_usize(optional_usize(args_obj, "max_items_per_list")?, 1, 64, 24),
        },
        max_tasks_per_slice: clamp_usize(
            optional_usize(args_obj, "max_tasks_per_slice")?,
            MIN_SLICE_TASKS,
            MAX_SLICE_TASKS,
            MAX_SLICE_TASKS,
        ),
        max_steps_per_task: clamp_usize(
            optional_usize(args_obj, "max_steps_per_task")?,
            MIN_STEP_LIST_LEN,
            MAX_STEP_LIST_LEN,
            MAX_STEP_LIST_LEN,
        ),
    })
}

fn clamp_usize(raw: Option<usize>, min: usize, max: usize, default: usize) -> usize {
    let mut value = raw.unwrap_or(default);
    value = value.clamp(min, max);
    value
}

fn check_unknown_args(
    args_obj: &serde_json::Map<String, Value>,
    allowed: &[&str],
    cmd: &str,
) -> Result<(), Value> {
    let mut unknown = args_obj
        .keys()
        .filter(|k| {
            !allowed.iter().any(|a| a == &k.as_str())
                && !IMPLICIT_ENVELOPE_KEYS.iter().any(|ik| ik == &k.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    unknown.dedup();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(ai_error_with(
        "INVALID_INPUT",
        &format!("unknown args for {cmd}: {}", unknown.join(", ")),
        Some("Remove unknown args and retry."),
        Vec::new(),
    ))
}

fn read_planfs_from_dir(dir: &Path, strict: bool, limits: &ImportLimits) -> Result<PlanFsTree, Value> {
    if !dir.exists() {
        return Err(ai_error_with(
            "UNKNOWN_ID",
            &format!("planfs directory not found: {}", dir.to_string_lossy()),
            Some("Initialize/export planfs first or provide a valid path."),
            Vec::new(),
        ));
    }
    if !dir.is_dir() {
        return Err(ai_error("INVALID_INPUT", "planfs path must be a directory"));
    }

    let plan_file = dir.join(PLAN_FILE_NAME);
    let plan_text = read_limited_text(&plan_file, limits.parse.max_file_bytes)?;
    let (mut plan, refs) = parse_plan_with_front_matter(&plan_text, strict, &limits.parse)?;
    if refs.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            "PLAN.md does not reference any slices",
        ));
    }
    if refs.len() > limits.parse.max_slices {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "PLAN.md slices exceeds max_slices budget: {} > {}",
                refs.len(),
                limits.parse.max_slices
            ),
            Some("Raise max_slices or split the plan into smaller batches."),
            Vec::new(),
        ));
    }

    let mut slice_map: HashMap<String, PlanFsSlice> = HashMap::new();
    for slice_ref in &refs {
        let file = dir.join(&slice_ref.file);
        if !file.exists() {
            return Err(ai_error_with(
                "INVALID_INPUT",
                &format!("missing referenced slice file: {}", slice_ref.file),
                Some("Create/update all files listed in PLAN.md slices."),
                Vec::new(),
            ));
        }
        let text = read_limited_text(&file, limits.parse.max_file_bytes)?;
        let slice = parse_slice_with_front_matter(&text, strict, &limits.parse)?;
        if slice.id != slice_ref.id {
            return Err(ai_error_with(
                "INVALID_INPUT",
                &format!(
                    "slice id mismatch: PLAN.md has {}, slice file has {}",
                    slice_ref.id, slice.id
                ),
                Some("Use the exact slice id in both plan and slice front matter."),
                Vec::new(),
            ));
        }
        enforce_slice_import_budget(&slice, limits)?;
        enforce_section_minimums(&slice)?;
        slice_map.insert(slice.id.clone(), slice);
    }

    let mut slices = Vec::with_capacity(refs.len());
    for slice_ref in refs.iter() {
        let slice = match slice_map.remove(&slice_ref.id) {
            Some(v) => v,
            None => {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    &format!("missing parsed slice for id {}", slice_ref.id),
                    Some("Ensure every PLAN.md slice file is present and valid."),
                    Vec::new(),
                ));
            }
        };
        slices.push(slice);
    }

    plan.slices = refs;
    Ok(PlanFsTree { plan, slices })
}

fn enforce_slice_import_budget(slice: &PlanFsSlice, limits: &ImportLimits) -> Result<(), Value> {
    if slice.tasks.len() > limits.max_tasks_per_slice {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "slice {} has too many tasks: {} > {}",
                slice.id,
                slice.tasks.len(),
                limits.max_tasks_per_slice
            ),
            Some("Reduce tasks or raise max_tasks_per_slice."),
            Vec::new(),
        ));
    }
    if slice.tasks.is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("slice {} has no tasks", slice.id),
        ));
    }
    for task in &slice.tasks {
        if task.steps.len() > limits.max_steps_per_task {
            return Err(ai_error_with(
                "INVALID_INPUT",
                &format!(
                    "slice {} task \"{}\" has too many steps: {} > {}",
                    slice.id,
                    task.title,
                    task.steps.len(),
                    limits.max_steps_per_task
                ),
                Some("Reduce steps or raise max_steps_per_task."),
                Vec::new(),
            ));
        }
    }
    Ok(())
}

fn enforce_section_minimums(slice: &PlanFsSlice) -> Result<(), Value> {
    for item in &slice.tasks {
        if item.tests.is_empty() {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("slice {} task {} has empty tests", slice.id, item.title),
            ));
        }
        if item.blockers.is_empty() {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("slice {} task {} has empty blockers", slice.id, item.title),
            ));
        }
        if item.success_criteria.is_empty() {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("slice {} task {} has empty success criteria", slice.id, item.title),
            ));
        }
        if item.rollback.is_empty() {
            return Err(ai_error(
                "INVALID_INPUT",
                &format!("slice {} task {} has empty rollback", slice.id, item.title),
            ));
        }
    }
    if slice.dod.success_criteria.is_empty()
        || slice.dod.tests.is_empty()
        || slice.dod.blockers.is_empty()
        || slice.dod.rollback.is_empty()
    {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("slice {} missing DoD fields", slice.id),
        ));
    }
    if slice.objective.trim().is_empty() {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("slice {} missing objective", slice.id),
        ));
    }
    if slice.budgets.max_files == 0
        || slice.budgets.max_diff_lines == 0
        || slice.budgets.max_context_refs == 0
    {
        return Err(ai_error(
            "INVALID_INPUT",
            &format!("slice {} has invalid budgets", slice.id),
        ));
    }
    Ok(())
}

fn render_planfs_markdown(doc: &PlanFsTree) -> Result<RenderedPlanFs, Value> {
    let plan_map = doc
        .slices
        .iter()
        .map(|slice| (slice.id.clone(), slice))
        .collect::<HashMap<_, _>>();

    let plan_text = render_plan_markdown(&doc.plan)?;
    let mut slices_out = Vec::with_capacity(doc.slices.len());
    let mut slice_files = Vec::with_capacity(doc.slices.len());
    for slice_ref in &doc.plan.slices {
        let slice = plan_map.get(&slice_ref.id).ok_or_else(|| {
            ai_error_with(
                "INVALID_INPUT",
                &format!("plan references slice {} but payload has no data", slice_ref.id),
                Some("Rebuild plan_spec from task tree or repair plan/slice payload."),
                Vec::new(),
            )
        })?;
        let text = render_slice_markdown(slice)?;
        slices_out.push((slice_ref.file.clone(), text));
        slice_files.push(slice_ref.file.clone());
    }
    Ok(RenderedPlanFs {
        plan: plan_text,
        slices: slices_out,
        slice_files,
    })
}

fn write_planfs_dir(dir: &Path, rendered: &RenderedPlanFs, overwrite: bool) -> Result<WriteSummary, Value> {
    if let Err(err) = fs::create_dir_all(dir) {
        return Err(ai_error(
            "STORE_ERROR",
            &format!("planfs: cannot create directory: {err}"),
        ));
    }

    let mut summary = WriteSummary::default();
    write_text_file(&dir.join(PLAN_FILE_NAME), &rendered.plan, overwrite, &mut summary)?;
    for (file, content) in &rendered.slices {
        write_text_file(&dir.join(file), content, overwrite, &mut summary)?;
    }
    Ok(summary)
}

fn write_text_file(path: &Path, content: &str, overwrite: bool, summary: &mut WriteSummary) -> Result<(), Value> {
    let rel = path.to_string_lossy().to_string();
    match fs::read_to_string(path) {
        Ok(existing) => {
            if existing == content {
                summary.unchanged.push(rel);
                return Ok(());
            }
            if !overwrite {
                return Err(ai_error_with(
                    "PRECONDITION_FAILED",
                    &format!("file already exists and differs: {}", path.to_string_lossy()),
                    Some("Set overwrite=true to update existing planfs files."),
                    Vec::new(),
                ));
            }
            fs::write(path, content).map_err(|err| {
                ai_error(
                    "STORE_ERROR",
                    &format!("planfs: cannot write file {}: {err}", path.to_string_lossy()),
                )
            })?;
            summary.updated.push(rel);
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::write(path, content).map_err(|write_err| {
                ai_error(
                    "STORE_ERROR",
                    &format!(
                        "planfs: cannot write file {}: {write_err}",
                        path.to_string_lossy()
                    ),
                )
            })?;
            summary.created.push(rel);
        }
        Err(err) => {
            return Err(ai_error(
                "STORE_ERROR",
                &format!("planfs: cannot read file {}: {err}", path.to_string_lossy()),
            ));
        }
    }
    Ok(())
}

fn build_planfs_from_task(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    task_id: &str,
    task: &bm_storage::TaskRow,
    slug: &str,
) -> Result<PlanFsTree, Value> {
    let all_steps = server
        .store
        .list_task_steps(workspace, task_id, None, 10_000)
        .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

    let mut root_steps = all_steps
        .iter()
        .filter(|row| path_depth(&row.path) == 1)
        .cloned()
        .collect::<Vec<_>>();
    root_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));

    if root_steps.is_empty() {
        return Err(ai_error_with(
            "PRECONDITION_FAILED",
            "task has no slices to export (no root steps)",
            Some("Decompose the task into root steps first (tasks.plan.decompose / tasks.define)."),
            Vec::new(),
        ));
    }

    let mut slices = Vec::<PlanFsSlice>::new();
    let mut refs = Vec::<PlanFsSliceRef>::new();

    for (idx, root) in root_steps.iter().enumerate() {
        let detail = server
            .store
            .step_detail(workspace, task_id, Some(&root.step_id), None)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

        let mut child_steps = all_steps
            .iter()
            .filter(|row| parent_path(&row.path).as_deref() == Some(root.path.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        child_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));

        let mut tasks = Vec::<PlanFsTask>::new();
        for child in child_steps {
            let child_detail = server
                .store
                .step_detail(workspace, task_id, Some(&child.step_id), None)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

            let task_success = nonempty_or(
                sanitize_items(&child_detail.success_criteria),
                vec![child.title.trim().to_string()],
            );
            let task_tests = nonempty_or(
                sanitize_items(&child_detail.tests),
                vec!["make check".to_string()],
            );
            let task_blockers = nonempty_or(
                sanitize_items(&child_detail.blockers),
                vec!["No blockers at the moment.".to_string()],
            );
            let task_rollback = vec![format!(
                "Rollback task \"{}\" changes and restore previous behavior.",
                child.title.trim()
            )];

            let mut step_titles = task_success.clone();
            while step_titles.len() < MIN_STEP_LIST_LEN {
                step_titles.push(format!(
                    "{} — step {}",
                    child.title.trim(),
                    step_titles.len() + 1
                ));
            }
            if step_titles.len() > MAX_STEP_LIST_LEN {
                step_titles.truncate(MAX_STEP_LIST_LEN);
            }

            let steps = step_titles
                .iter()
                .enumerate()
                .map(|(step_idx, title)| PlanFsStep {
                    title: title.clone(),
                    success_criteria: vec![format!("{} completed", title)],
                    tests: task_tests.clone(),
                    blockers: task_blockers.clone(),
                    rollback: vec![format!(
                        "Rollback step {} in task \"{}\".",
                        step_idx + 1,
                        child.title.trim()
                    )],
                })
                .collect::<Vec<_>>();

            tasks.push(PlanFsTask {
                title: child.title.trim().to_string(),
                success_criteria: task_success,
                tests: task_tests,
                blockers: task_blockers,
                rollback: task_rollback,
                steps,
            });
        }

        while tasks.len() < MIN_SLICE_TASKS {
            let lane = tasks.len() + 1;
            let lane_title = format!("Execution lane {lane}");
            tasks.push(PlanFsTask {
                title: lane_title.clone(),
                success_criteria: vec![format!("{lane_title} completed")],
                tests: vec!["make check".to_string()],
                blockers: vec!["No blockers at the moment.".to_string()],
                rollback: vec![format!("Rollback {lane_title} changes.")],
                steps: vec![
                    PlanFsStep {
                        title: format!("{lane_title} — implement"),
                        success_criteria: vec![format!("{lane_title} implementation done")],
                        tests: vec!["make check".to_string()],
                        blockers: vec!["No blockers at the moment.".to_string()],
                        rollback: vec![format!("Rollback {lane_title} implementation.")],
                    },
                    PlanFsStep {
                        title: format!("{lane_title} — validate"),
                        success_criteria: vec![format!("{lane_title} validated")],
                        tests: vec!["make check".to_string()],
                        blockers: vec!["No blockers at the moment.".to_string()],
                        rollback: vec![format!("Rollback {lane_title} validation.")],
                    },
                    PlanFsStep {
                        title: format!("{lane_title} — finalize"),
                        success_criteria: vec![format!("{lane_title} finalized")],
                        tests: vec!["make check".to_string()],
                        blockers: vec!["No blockers at the moment.".to_string()],
                        rollback: vec![format!("Rollback {lane_title} finalization.")],
                    },
                ],
            });
        }
        if tasks.len() > MAX_SLICE_TASKS {
            tasks.truncate(MAX_SLICE_TASKS);
        }

        let slice_idx = idx + 1;
        let slice_id = format!("SLICE-{slice_idx}");
        let slice_title = root.title.trim().to_string();
        let slice_status = planfs_status_from_step(root);
        let objective = detail
            .next_action
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .or_else(|| detail.success_criteria.first().map(|v| v.trim().to_string()))
            .unwrap_or_else(|| slice_title.clone());

        let dod = PlanFsDod {
            success_criteria: nonempty_or(
                sanitize_items(&detail.success_criteria),
                vec![format!("{slice_title} complete")],
            ),
            tests: nonempty_or(
                sanitize_items(&detail.tests),
                vec!["make check".to_string()],
            ),
            blockers: nonempty_or(
                sanitize_items(&detail.blockers),
                vec!["No blockers at the moment.".to_string()],
            ),
            rollback: vec![format!("Rollback slice {slice_idx} changes.")],
        };

        let slice_sections = PlanFsSectionBundle {
            goal: objective.clone(),
            scope: vec!["Keep scope inside this slice boundary.".to_string()],
            non_goals: vec!["No edits outside slice scope.".to_string()],
            interfaces: vec!["Do not change external interfaces without explicit contract update.".to_string()],
            contracts: vec!["Contract-first updates only.".to_string()],
            tests: dod.tests.clone(),
            proof: vec![format!("FILE:docs/plans/{slug}/Slice-{slice_idx}.md")],
            rollback: dod.rollback.clone(),
            risks: vec!["Plan drift between task tree and files.".to_string()],
        };

        refs.push(PlanFsSliceRef {
            id: slice_id.clone(),
            title: slice_title.clone(),
            file: format!("Slice-{slice_idx}.md"),
            status: Some(slice_status.clone()),
        });

        slices.push(PlanFsSlice {
            id: slice_id,
            title: slice_title,
            objective,
            status: Some(slice_status),
            budgets: default_slice_budgets(),
            dod,
            tasks,
            sections: slice_sections,
        });
    }

    let mut plan_title = task.title.trim().to_string();
    let mut objective = task
        .description
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "Deliver task {} with deterministic slice-by-slice execution.",
                task.title.trim()
            )
        });

    let mut constraints = sanitize_plan_constraints(task.context.as_deref());
    if !task.parent_plan_id.trim().is_empty() {
        let plan_row = server
            .store
            .get_plan(workspace, &task.parent_plan_id)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
        if let Some(plan) = plan_row {
            let plan_title_clean = plan.title.trim();
            if !plan_title_clean.is_empty() {
                plan_title = plan_title_clean.to_string();
            }
            if let Some(plan_objective) = plan
                .description
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                objective = plan_objective.to_string();
            }
            let plan_constraints = sanitize_plan_constraints(plan.context.as_deref());
            if !plan_constraints.is_empty() {
                constraints = plan_constraints;
            }
        }
    }
    if constraints.is_empty() {
        constraints
            .push("Contract-first changes only; keep behavior deterministic and fail-closed.".to_string());
    }

    let plan_sections = PlanFsSectionBundle {
        goal: objective.clone(),
        scope: vec!["Implement slices sequentially with green verify gates.".to_string()],
        non_goals: vec!["No silent scope creep.".to_string()],
        interfaces: vec!["Any interface change must update contracts/docs.".to_string()],
        contracts: vec!["Keep MCP schemas and docs aligned.".to_string()],
        tests: vec!["make check".to_string()],
        proof: vec![format!("CMD: tasks.planfs.export --task {task_id}")],
        rollback: vec!["Rollback per-slice changes if Verify turns red.".to_string()],
        risks: vec!["Agent drift or partial implementation between slices.".to_string()],
    };

    let plan = PlanFsPlan {
        plan_slug: slug.to_string(),
        title: plan_title,
        objective,
        constraints,
        policy: "strict".to_string(),
        slices: refs,
        sections: plan_sections,
    };

    Ok(PlanFsTree { plan, slices })
}

fn apply_planfs_to_task(
    server: &mut McpServer,
    workspace: &WorkspaceId,
    task_id: &str,
    slices: &[PlanFsSlice],
    strict: bool,
) -> Result<ApplyStats, Value> {
    let mut stats = ApplyStats {
        root_created: 0,
        root_updated: 0,
        child_created: 0,
        child_updated: 0,
        unchanged: 0,
    };

    if slices.is_empty() {
        return Err(ai_error("INVALID_INPUT", "planfs import has no slices"));
    }

    let mut all_steps = server
        .store
        .list_task_steps(workspace, task_id, None, 10_000)
        .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

    let mut root_steps = all_steps
        .iter()
        .filter(|row| path_depth(&row.path) == 1)
        .cloned()
        .collect::<Vec<_>>();
    root_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));

    if root_steps.is_empty() {
        let new_root_steps = slices
            .iter()
            .map(|slice| NewStep {
                title: slice.title.clone(),
                success_criteria: slice.dod.success_criteria.clone(),
            })
            .collect::<Vec<_>>();
        server
            .store
            .steps_decompose(workspace, task_id, None, None, new_root_steps)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
        stats.root_created += slices.len();
        all_steps = server
            .store
            .list_task_steps(workspace, task_id, None, 10_000)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
        root_steps = all_steps
            .iter()
            .filter(|row| path_depth(&row.path) == 1)
            .cloned()
            .collect::<Vec<_>>();
        root_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));
    }

    if root_steps.len() != slices.len() && strict {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "root step count mismatch: storage has {}, planfs has {}",
                root_steps.len(),
                slices.len()
            ),
            Some("Align root steps or import with strict=false."),
            Vec::new(),
        ));
    }

    if root_steps.len() < slices.len() {
        let missing = &slices[root_steps.len()..];
        let new_steps = missing
            .iter()
            .map(|slice| NewStep {
                title: slice.title.clone(),
                success_criteria: slice.dod.success_criteria.clone(),
            })
            .collect::<Vec<_>>();
        server
            .store
            .steps_decompose(workspace, task_id, None, None, new_steps)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
        stats.root_created += missing.len();
        all_steps = server
            .store
            .list_task_steps(workspace, task_id, None, 10_000)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
        root_steps = all_steps
            .iter()
            .filter(|row| path_depth(&row.path) == 1)
            .cloned()
            .collect::<Vec<_>>();
        root_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));
    }

    for (root, slice) in root_steps.iter().take(slices.len()).zip(slices.iter()) {
        let detail = server
            .store
            .step_detail(workspace, task_id, Some(&root.step_id), None)
            .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

        let desired_sc = nonempty_or(
            sanitize_items(&slice.dod.success_criteria),
            vec![format!("{} complete", slice.title)],
        );
        let desired_tests = nonempty_or(sanitize_items(&slice.dod.tests), vec!["make check".to_string()]);
        let desired_blockers = nonempty_or(
            sanitize_items(&slice.dod.blockers),
            vec!["No blockers at the moment.".to_string()],
        );

        let root_changed = detail.title.trim() != slice.title.trim()
            || sanitize_items(&detail.success_criteria) != desired_sc
            || sanitize_items(&detail.tests) != desired_tests
            || sanitize_items(&detail.blockers) != desired_blockers
            || detail
                .next_action
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                != slice.objective.trim();

        if root_changed {
            let request = StepDefineRequest {
                task_id: task_id.to_string(),
                expected_revision: None,
                agent_id: None,
                selector: StepSelector {
                    step_id: Some(root.step_id.clone()),
                    path: None,
                },
                patch: StepPatch {
                    title: Some(slice.title.clone()),
                    success_criteria: Some(desired_sc.clone()),
                    tests: Some(desired_tests.clone()),
                    blockers: Some(desired_blockers.clone()),
                    next_action: Some(Some(slice.objective.clone())),
                    stop_criteria: None,
                    proof_tests_mode: None,
                    proof_security_mode: None,
                    proof_perf_mode: None,
                    proof_docs_mode: None,
                },
            };
            server
                .store
                .step_define(workspace, request)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
            stats.root_updated += 1;
        } else {
            stats.unchanged += 1;
        }

        let root_path = root.path.clone();
        let mut child_steps = all_steps
            .iter()
            .filter(|row| parent_path(&row.path).as_deref() == Some(root_path.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        child_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));

        if child_steps.is_empty() {
            let new_child_steps = slice
                .tasks
                .iter()
                .map(|task| NewStep {
                    title: task.title.clone(),
                    success_criteria: task
                        .steps
                        .iter()
                        .map(|step| step.title.clone())
                        .collect::<Vec<_>>(),
                })
                .collect::<Vec<_>>();
            let parent_step_path = bm_core::paths::StepPath::parse(&root_path)
                .map_err(|_| ai_error("STORE_ERROR", "invalid root step path"))?;
            server
                .store
                .steps_decompose(workspace, task_id, None, Some(&parent_step_path), new_child_steps)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
            stats.child_created += slice.tasks.len();
            all_steps = server
                .store
                .list_task_steps(workspace, task_id, None, 10_000)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
            child_steps = all_steps
                .iter()
                .filter(|row| parent_path(&row.path).as_deref() == Some(root_path.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            child_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));
        }

        if child_steps.len() < slice.tasks.len() {
            let missing = &slice.tasks[child_steps.len()..];
            let new_child_steps = missing
                .iter()
                .map(|task| NewStep {
                    title: task.title.clone(),
                    success_criteria: task
                        .steps
                        .iter()
                        .map(|step| step.title.clone())
                        .collect::<Vec<_>>(),
                })
                .collect::<Vec<_>>();
            let parent_step_path = bm_core::paths::StepPath::parse(&root_path)
                .map_err(|_| ai_error("STORE_ERROR", "invalid root step path"))?;
            server
                .store
                .steps_decompose(workspace, task_id, None, Some(&parent_step_path), new_child_steps)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
            stats.child_created += missing.len();
            all_steps = server
                .store
                .list_task_steps(workspace, task_id, None, 10_000)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
            child_steps = all_steps
                .iter()
                .filter(|row| parent_path(&row.path).as_deref() == Some(root_path.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            child_steps.sort_by(|a, b| step_path_key(&a.path).cmp(&step_path_key(&b.path)));
        }

        if child_steps.len() > slice.tasks.len() && strict {
            return Err(ai_error_with(
                "INVALID_INPUT",
                &format!(
                    "slice {} task count mismatch: storage has {}, planfs has {}",
                    slice.id,
                    child_steps.len(),
                    slice.tasks.len()
                ),
                Some("Align child steps manually or import into an empty slice subtree."),
                Vec::new(),
            ));
        }

        for (child, desired_task) in child_steps.iter().take(slice.tasks.len()).zip(slice.tasks.iter()) {
            let child_detail = server
                .store
                .step_detail(workspace, task_id, Some(&child.step_id), None)
                .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;

            let desired_sc = nonempty_or(
                desired_task
                    .steps
                    .iter()
                    .map(|step| step.title.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect::<Vec<_>>(),
                desired_task.success_criteria.clone(),
            );
            let desired_tests = nonempty_or(
                sanitize_items(&desired_task.tests),
                desired_tests.clone(),
            );
            let desired_blockers = nonempty_or(
                sanitize_items(&desired_task.blockers),
                desired_blockers.clone(),
            );

            let child_changed = child_detail.title.trim() != desired_task.title.trim()
                || sanitize_items(&child_detail.success_criteria) != desired_sc
                || sanitize_items(&child_detail.tests) != desired_tests
                || sanitize_items(&child_detail.blockers) != desired_blockers;

            if child_changed {
                let request = StepDefineRequest {
                    task_id: task_id.to_string(),
                    expected_revision: None,
                    agent_id: None,
                    selector: StepSelector {
                        step_id: Some(child.step_id.clone()),
                        path: None,
                    },
                    patch: StepPatch {
                        title: Some(desired_task.title.clone()),
                        success_criteria: Some(desired_sc),
                        tests: Some(desired_tests),
                        blockers: Some(desired_blockers),
                        next_action: None,
                        stop_criteria: None,
                        proof_tests_mode: None,
                        proof_security_mode: None,
                        proof_perf_mode: None,
                        proof_docs_mode: None,
                    },
                };
                server
                    .store
                    .step_define(workspace, request)
                    .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
                stats.child_updated += 1;
            } else {
                stats.unchanged += 1;
            }
        }
    }

    Ok(stats)
}

fn default_slice_budgets() -> PlanFsBudgets {
    PlanFsBudgets {
        max_files: 16,
        max_diff_lines: 1_500,
        max_context_refs: 24,
    }
}

fn workspace_repo_root(
    store: &mut bm_storage::SqliteStore,
    workspace: &WorkspaceId,
) -> Result<PathBuf, Value> {
    let root = store
        .workspace_path_primary_get(workspace)
        .map_err(|err| ai_error("STORE_ERROR", &format_store_error(err)))?;
    let Some(root) = root else {
        return Err(ai_error_with(
            "PRECONDITION_FAILED",
            "workspace has no bound repo path",
            Some("Bind workspace by calling status with workspace=\"/absolute/path/to/repo\"."),
            Vec::new(),
        ));
    };

    let abs = PathBuf::from(root);
    if !abs.exists() || !abs.is_dir() {
        return Err(ai_error_with(
            "PRECONDITION_FAILED",
            "workspace bound path does not exist or is not a directory",
            Some("Re-bind workspace to an existing repository path."),
            Vec::new(),
        ));
    }
    Ok(abs)
}

fn resolve_plan_dir(
    args_obj: &serde_json::Map<String, Value>,
    root: &Path,
    slug: &str,
) -> Result<(String, PathBuf), Value> {
    if let Some(raw) = optional_string(args_obj, "path")? {
        let cleaned = raw.trim();
        if cleaned.is_empty() {
            return Err(ai_error("INVALID_INPUT", "path must not be empty"));
        }
        let user_path = PathBuf::from(cleaned);
        let (relative, absolute) = if user_path.is_absolute() {
            if !user_path.starts_with(root) {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    "absolute path must stay under workspace root",
                    Some("Use a path inside the workspace repository."),
                    Vec::new(),
                ));
            }
            let rel = user_path
                .strip_prefix(root)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| PathBuf::new());
            (rel, user_path)
        } else {
            if user_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(ai_error_with(
                    "INVALID_INPUT",
                    "path must not contain '..'",
                    Some("Provide a repo-relative path without parent traversal."),
                    Vec::new(),
                ));
            }
            (user_path.clone(), root.join(&user_path))
        };
        let rel = relative.to_string_lossy().replace('\\', "/");
        if rel.trim().is_empty() {
            return Err(ai_error("INVALID_INPUT", "path must not resolve to repo root"));
        }
        return Ok((rel, absolute));
    }

    let rel = format!("docs/plans/{slug}");
    Ok((rel.clone(), root.join(rel)))
}

fn slugify_task_title(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "plan".to_string()
    } else {
        out
    }
}

fn normalize_plan_slug(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('-') || trimmed.ends_with('-') {
        return None;
    }
    if trimmed
        .chars()
        .any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'))
    {
        return None;
    }
    Some(trimmed)
}

fn read_limited_text(path: &Path, max_file_bytes: usize) -> Result<String, Value> {
    let meta = fs::metadata(path).map_err(|err| {
        ai_error(
            "STORE_ERROR",
            &format!("planfs: cannot stat file {}: {err}", path.to_string_lossy()),
        )
    })?;
    if meta.len() > max_file_bytes as u64 {
        return Err(ai_error_with(
            "INVALID_INPUT",
            &format!(
                "planfs file exceeds max_file_bytes: {} > {} ({})",
                meta.len(),
                max_file_bytes,
                path.to_string_lossy()
            ),
            Some("Increase max_file_bytes or split content."),
            Vec::new(),
        ));
    }
    fs::read_to_string(path).map_err(|err| {
        ai_error(
            "STORE_ERROR",
            &format!("planfs: cannot read file {}: {err}", path.to_string_lossy()),
        )
    })
}

fn normalize_optional_text(raw: &str) -> Option<String> {
    let cleaned = raw.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut keys = obj.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut out = serde_json::Map::<String, Value>::new();
            for key in keys {
                if let Some(v) = obj.get(&key) {
                    out.insert(key, canonicalize_json(v.clone()));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

fn path_depth(path: &str) -> usize {
    StepPath::parse(path)
        .map(|parsed| parsed.indices().len())
        .unwrap_or(0)
}

fn parent_path(path: &str) -> Option<String> {
    let parsed = StepPath::parse(path).ok()?;
    let indices = parsed.indices();
    if indices.len() <= 1 {
        return None;
    }
    Some(
        indices[..indices.len() - 1]
            .iter()
            .map(|idx| format!("s:{idx}"))
            .collect::<Vec<_>>()
            .join("."),
    )
}

fn step_path_key(path: &str) -> Vec<usize> {
    StepPath::parse(path)
        .map(|parsed| parsed.indices().to_vec())
        .unwrap_or_else(|_| vec![usize::MAX])
}

fn sanitize_items(items: &[String]) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for raw in items {
        let cleaned = raw.trim().trim_start_matches('-').trim().to_string();
        if cleaned.is_empty() {
            continue;
        }
        let key = cleaned.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(cleaned);
        }
    }
    out
}

fn planfs_status_from_step(step: &bm_storage::StepListRow) -> String {
    if step.completed {
        "done".to_string()
    } else if step.blocked {
        "blocked".to_string()
    } else if step.criteria_confirmed
        || step.tests_confirmed
        || step.security_confirmed
        || step.perf_confirmed
        || step.docs_confirmed
    {
        "active".to_string()
    } else {
        "todo".to_string()
    }
}

fn sanitize_plan_constraints(context: Option<&str>) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    let Some(context) = context else {
        return out;
    };

    for raw in context.lines() {
        let cleaned = raw.trim();
        if cleaned.is_empty() {
            continue;
        }
        if looks_like_placeholder(cleaned) || looks_like_machine_context_blob(cleaned) {
            continue;
        }
        let key = cleaned.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(cleaned.to_string());
        }
    }
    out
}

fn looks_like_machine_context_blob(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() > 180 && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.starts_with('{')
        && trimmed.ends_with('}')
        && (lower.contains("\"budgets\"")
            || lower.contains("\"tasks\"")
            || lower.contains("\"dod\"")
            || lower.contains("\"shared_context_refs\""))
    {
        return true;
    }
    false
}

fn nonempty_or(mut items: Vec<String>, fallback: Vec<String>) -> Vec<String> {
    if items.is_empty() {
        fallback
    } else {
        items.shrink_to_fit();
        items
    }
}
