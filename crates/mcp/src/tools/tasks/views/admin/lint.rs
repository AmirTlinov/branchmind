#![forbid(unsafe_code)]

use crate::*;
use serde_json::{Value, json};

impl McpServer {
    fn lint_issue(
        severity: &str,
        kind: &str,
        code: &str,
        message: impl Into<String>,
        recovery: impl Into<String>,
        target: Option<Value>,
    ) -> Value {
        let mut out = json!({
            "severity": severity,
            "kind": kind,
            "code": code,
            "message": message.into(),
            "recovery": recovery.into()
        });
        if let Some(target) = target
            && !target.is_null()
            && let Some(obj) = out.as_object_mut()
        {
            obj.insert("target".to_string(), target);
        }
        out
    }

    fn lint_patch(
        id: &str,
        purpose: &str,
        tool: &str,
        arguments: Value,
        notes: Option<&str>,
    ) -> Value {
        let mut out = json!({
            "id": id,
            "purpose": purpose,
            "apply": {
                "tool": tool,
                "arguments": arguments
            }
        });
        if let Some(notes) = notes
            && let Some(obj) = out.as_object_mut()
        {
            obj.insert("notes".to_string(), Value::String(notes.to_string()));
        }
        out
    }

    fn patch_priority(id: &str) -> u8 {
        if id.contains("current:clamp") {
            return 0;
        }
        if id.contains("active_limit:park") {
            return 1;
        }
        if id.contains("mark_done") {
            return 2;
        }

        // When `patches_limit` truncates, prioritize what keeps the task executable and provable
        // in the agent loop (next_action + proof + meaning binding) over “nice to have” confirms.
        if id.contains("set_next_action") {
            return 3;
        }
        if id.contains("require_proof_tests") {
            return 4;
        }
        if id.contains("missing_anchor") {
            return 5;
        }

        if id.contains("seed_steps") {
            return 6;
        }
        if id.contains("seed_success_criteria") {
            return 7;
        }
        if id.contains("seed_tests") {
            return 8;
        }
        if id.contains("confirm_criteria") {
            return 9;
        }
        if id.contains("confirm_tests") {
            return 10;
        }

        11
    }

    fn select_patches(mut patches: Vec<Value>, limit: usize) -> Vec<Value> {
        if limit == 0 || patches.is_empty() {
            return Vec::new();
        }
        if patches.len() <= limit {
            return patches;
        }
        let mut ranked = patches
            .drain(..)
            .enumerate()
            .map(|(idx, patch)| {
                let id = patch.get("id").and_then(|v| v.as_str()).unwrap_or("");
                (Self::patch_priority(id), idx, patch)
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        ranked
            .into_iter()
            .take(limit)
            .map(|(_, _, patch)| patch)
            .collect()
    }

    fn default_step_seed(title: &str) -> (Vec<&'static str>, Vec<&'static str>) {
        let raw = title.trim().to_ascii_lowercase();
        let is_research = raw.contains("research")
            || raw.contains("investigat")
            || raw.contains("explor")
            || raw.contains("hypothes")
            || raw.contains("analysis")
            || raw.contains("analyz");
        let is_design = raw.contains("design")
            || raw.contains("spec")
            || raw.contains("contract")
            || raw.contains("architecture");
        let is_verify = raw.contains("test")
            || raw.contains("verify")
            || raw.contains("bench")
            || raw.contains("profile")
            || raw.contains("measure");
        let is_ops = raw.contains("deploy")
            || raw.contains("release")
            || raw.contains("runbook")
            || raw.contains("ci")
            || raw.contains("ops");

        if is_research {
            return (
                vec![
                    "Write a falsifiable hypothesis",
                    "Define the minimal falsifier test",
                    "Capture evidence (what happened)",
                    "Decide next action (or stop criteria)",
                ],
                vec!["Minimal falsifier test is executed (or scheduled)"],
            );
        }
        if is_design {
            return (
                vec![
                    "Constraints + non-goals written",
                    "Interfaces/contracts specified",
                    "Risks + rollback documented",
                ],
                vec!["Contract is reviewed against constraints"],
            );
        }
        if is_verify {
            return (
                vec![
                    "Verification is executed",
                    "Results recorded as proof",
                    "Pass/fail decision made",
                ],
                vec!["Run the relevant test/bench suite"],
            );
        }
        if is_ops {
            return (
                vec![
                    "Runbook validated (happy + rollback path)",
                    "Operational risks documented",
                    "Success signal defined",
                ],
                vec!["Dry-run the runbook steps"],
            );
        }
        (
            vec![
                "Change implemented",
                "No regressions in relevant tests",
                "Docs updated or explicitly deferred",
            ],
            vec!["Run the relevant test suite"],
        )
    }

    fn default_anchor_seed(title: &str) -> (String, String, String) {
        let raw = title.trim();
        let lowered = raw.to_ascii_lowercase();

        let kind = if lowered.contains("contract") || lowered.contains("api") {
            "contract"
        } else if lowered.contains("deploy")
            || lowered.contains("release")
            || lowered.contains("runner")
            || lowered.contains("ops")
        {
            "ops"
        } else if lowered.contains("test") || lowered.contains("verify") || lowered.contains("ci") {
            "test-surface"
        } else if lowered.contains("research")
            || lowered.contains("investigat")
            || lowered.contains("hypothes")
            || lowered.contains("analysis")
            || lowered.contains("analyz")
        {
            "research"
        } else {
            "component"
        };

        let mut slug = String::new();
        let mut prev_dash = false;
        for ch in lowered.chars() {
            if slug.len() >= crate::ANCHOR_MAX_SLUG_LEN {
                break;
            }
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                slug.push(ch);
                prev_dash = false;
                continue;
            }
            if slug.is_empty() || prev_dash {
                continue;
            }
            slug.push('-');
            prev_dash = true;
        }
        let slug = slug.trim_matches('-');
        let slug = if slug.is_empty() {
            "unclassified".to_string()
        } else {
            slug.to_string()
        };
        let anchor_id = format!("a:{slug}");

        // Keep the anchor title short and human-friendly, but deterministic.
        let anchor_title = truncate_string(&redact_text(raw), 72);

        (anchor_id, anchor_title, kind.to_string())
    }

    fn build_context_health(
        &mut self,
        workspace: &WorkspaceId,
        target_id: &str,
        kind: TaskKind,
    ) -> Result<Value, StoreError> {
        let mut issues = Vec::new();
        let reasoning_ref = self.store.reasoning_ref_get(workspace, target_id, kind)?;
        let (reasoning, stored) = match reasoning_ref {
            Some(row) => (row, true),
            None => {
                let derived = ReasoningRef::for_entity(kind, target_id);
                (
                    bm_storage::ReasoningRefRow {
                        branch: derived.branch,
                        notes_doc: derived.notes_doc,
                        graph_doc: derived.graph_doc,
                        trace_doc: derived.trace_doc,
                    },
                    false,
                )
            }
        };

        if !stored {
            issues.push(json!({
                "severity": "warning",
                "code": "REASONING_REF_MISSING",
                "message": "reasoning refs are not persisted yet",
                "recovery": "Run tasks_resume_super with read_only=false or think_pipeline to seed reasoning refs."
            }));
        }

        let notes_has = self
            .store
            .doc_show_tail(workspace, &reasoning.branch, &reasoning.notes_doc, None, 1)
            .map(|slice| !slice.entries.is_empty())
            .unwrap_or(false);
        let trace_has = self
            .store
            .doc_show_tail(workspace, &reasoning.branch, &reasoning.trace_doc, None, 1)
            .map(|slice| !slice.entries.is_empty())
            .unwrap_or(false);
        let cards_has = match self.store.graph_query(
            workspace,
            &reasoning.branch,
            &reasoning.graph_doc,
            bm_storage::GraphQueryRequest {
                ids: None,
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 1,
                include_edges: false,
                edges_limit: 0,
            },
        ) {
            Ok(slice) => !slice.nodes.is_empty(),
            Err(StoreError::UnknownBranch) => {
                issues.push(json!({
                    "severity": "warning",
                    "code": "REASONING_BRANCH_MISSING",
                    "message": "reasoning branch is missing",
                    "recovery": "Seed reasoning via think_pipeline or switch read_only=false on resume tools."
                }));
                false
            }
            Err(StoreError::InvalidInput(msg)) => return Err(StoreError::InvalidInput(msg)),
            Err(err) => return Err(err),
        };

        if !notes_has && !trace_has && !cards_has {
            issues.push(json!({
                "severity": "warning",
                "code": "CONTEXT_EMPTY",
                "message": "notes/trace/graph are empty",
                "recovery": "Add a decision/evidence note or run think_pipeline to seed context."
            }));
        }

        if trace_has && !notes_has {
            issues.push(json!({
                "severity": "warning",
                "code": "TRACE_ONLY",
                "message": "trace has events but notes are empty",
                "recovery": "Summarize key decisions in notes to improve recall."
            }));
        }

        let status = if issues.is_empty() { "ok" } else { "warn" };

        Ok(json!({
            "status": status,
            "stats": {
                "notes_present": notes_has,
                "trace_present": trace_has,
                "cards_present": cards_has,
                "reasoning_ref": if stored { "stored" } else { "derived" }
            },
            "issues": issues
        }))
    }

    pub(crate) fn tool_tasks_lint(&mut self, args: Value) -> Value {
        let Some(args_obj) = args.as_object() else {
            return ai_error("INVALID_INPUT", "arguments must be an object");
        };
        let workspace = match require_workspace(args_obj) {
            Ok(w) => w,
            Err(resp) => return resp,
        };

        let (target_id, kind, _focus) =
            match resolve_target_id(&mut self.store, &workspace, args_obj) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        let patches_limit = match optional_usize(args_obj, "patches_limit") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let anchor_suggestions_limit = patches_limit.unwrap_or(5).min(5);

        let mut issues = Vec::new();
        let mut patches = Vec::new();
        let mut actions: Vec<Value> = Vec::new();
        match kind {
            TaskKind::Plan => {
                const ACTIVE_LIMIT: i64 = 3;

                let plan = match self.store.get_plan(&workspace, &target_id) {
                    Ok(Some(plan)) => plan,
                    Ok(None) => return ai_error("UNKNOWN_ID", "Unknown id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let checklist = match self.store.plan_checklist_get(&workspace, &target_id) {
                    Ok(v) => v,
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let total = checklist.steps.len() as i64;
                if total == 0 {
                    issues.push(Self::lint_issue(
                        "warning",
                        "actionless",
                        "NO_CHECKLIST",
                        "plan checklist is empty",
                        "Use tasks_plan to set checklist steps.",
                        Some(json!({ "kind": "plan", "id": target_id })),
                    ));
                }
                if checklist.current < 0 || checklist.current > total {
                    issues.push(Self::lint_issue(
                        "error",
                        "unbounded",
                        "CHECKLIST_INDEX_OUT_OF_RANGE",
                        format!("plan_current out of range: {}", checklist.current),
                        "Use tasks_plan to set a valid current index.",
                        Some(json!({ "kind": "plan", "id": target_id })),
                    ));
                    patches.push(Self::lint_patch(
                        "patch:plan:current:clamp",
                        "Clamp plan_current into [0..steps.len]",
                        "tasks_plan",
                        json!({
                            "workspace": workspace.as_str(),
                            "plan": target_id,
                            "expected_revision": plan.revision,
                            "current": std::cmp::min(std::cmp::max(checklist.current, 0), total)
                        }),
                        Some("This is a safe integrity fix; it does not change checklist steps."),
                    ));
                }

                // Anti-kasha: keep active horizon small by default. Over-limit is not an error,
                // but it is a reliable predictor of “context soup” in new sessions.
                let by_status = match self
                    .store
                    .count_tasks_by_status_for_plan(&workspace, &target_id)
                {
                    Ok(v) => v,
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                let active = by_status.get("ACTIVE").copied().unwrap_or(0);

                // Anchor coverage KPI: active tasks without anchors reliably cause missing anchors
                // and re-invention in new sessions. This lint is the one-shot “pay rent” reminder.
                let coverage = match self.store.plan_anchors_coverage(
                    &workspace,
                    bm_storage::PlanAnchorsCoverageRequest {
                        plan_id: target_id.clone(),
                        top_anchors_limit: 0,
                    },
                ) {
                    Ok(v) => v,
                    Err(StoreError::InvalidInput(msg)) => return ai_error("INVALID_INPUT", msg),
                    Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                    Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                };
                if coverage.active_total > 0 && coverage.active_missing_anchor > 0 {
                    issues.push(Self::lint_issue(
                        "warning",
                        "unnavigable",
                        "ACTIVE_TASKS_MISSING_ANCHOR",
                        format!(
                            "plan has ACTIVE tasks without anchors: {}/{}",
                            coverage.active_missing_anchor, coverage.active_total
                        ),
                        "Attach anchors to ACTIVE tasks so `where=` works and old decisions are findable by meaning.",
                        Some(json!({ "kind": "plan", "id": target_id })),
                    ));

                    let active_tasks = match self
                        .store
                        .list_tasks_for_plan_by_status(&workspace, &target_id, "ACTIVE", 50, 0)
                    {
                        Ok(v) => v,
                        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };

                    let mut suggested = 0usize;
                    for t in active_tasks.iter() {
                        if suggested >= anchor_suggestions_limit {
                            break;
                        }
                        let Ok(list) = self.store.task_anchors_list(
                            &workspace,
                            bm_storage::TaskAnchorsListRequest {
                                task_id: t.id.clone(),
                                limit: 1,
                            },
                        ) else {
                            continue;
                        };
                        if !list.anchors.is_empty() {
                            continue;
                        }

                        let (anchor_id, anchor_title, anchor_kind) =
                            Self::default_anchor_seed(&t.title);

                        let anchor_exists = self
                            .store
                            .anchor_get(
                                &workspace,
                                bm_storage::AnchorGetRequest {
                                    id: anchor_id.clone(),
                                },
                            )
                            .ok()
                            .flatten()
                            .is_some();

                        let mut args = json!({
                            "workspace": workspace.as_str(),
                            "target": t.id.clone(),
                            "anchor": anchor_id,
                            "content": "Anchor binding: attach meaning to this task so resume/map works.\n\n(Replace this note later with real decisions/evidence.)",
                            "card_type": "note",
                            "visibility": "canon"
                        });

                        if !anchor_exists && let Some(obj) = args.as_object_mut() {
                            obj.insert("title".to_string(), Value::String(anchor_title));
                            obj.insert("kind".to_string(), Value::String(anchor_kind));
                        }

                        patches.push(Self::lint_patch(
                            &format!("patch:plan:missing_anchor:attach:{}", t.id),
                            "Attach a minimal anchor to this ACTIVE task (one-command)",
                            "macro_anchor_note",
                            args,
                            Some("This is a low-noise “meaning binding” note. It should be kept short; supersede with real decisions/evidence."),
                        ));
                        suggested += 1;
                    }

                    actions.push(json!({
                        "id": "action:plan:list_active_tasks",
                        "purpose": "List ACTIVE tasks for this plan (to pick anchors)",
                        "apply": {
                            "tool": "tasks_context",
                            "arguments": {
                                "workspace": workspace.as_str(),
                                "plan": target_id,
                                "tasks_status": "ACTIVE",
                                "tasks_limit": 50,
                                "max_chars": 4000
                            }
                        }
                    }));
                }

                if active > ACTIVE_LIMIT {
                    issues.push(Self::lint_issue(
                        "warning",
                        "unbounded",
                        "ACTIVE_LIMIT_EXCEEDED",
                        format!("plan has too many ACTIVE tasks: {active} (limit={ACTIVE_LIMIT})"),
                        "Keep only 1–3 ACTIVE tasks; park the rest into TODO so resume stays low-noise.",
                        Some(json!({ "kind": "plan", "id": target_id })),
                    ));

                    // Prefer keeping the focused task active (if it belongs to this plan).
                    let focus = self.store.focus_get(&workspace).ok().flatten();

                    let mut active_tasks = match self
                        .store
                        .list_tasks_for_plan_by_status(&workspace, &target_id, "ACTIVE", 50, 0)
                    {
                        Ok(v) => v,
                        Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };

                    // Deterministic: tasks are already ordered by id asc.
                    // Keep set is deterministic given (plan, focus, ids).
                    let mut keep: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    if let Some(focus_id) = focus.as_deref()
                        && focus_id.starts_with("TASK-")
                        && active_tasks.iter().any(|t| t.id == focus_id)
                    {
                        keep.insert(focus_id.to_string());
                    }

                    // Fill remaining keep slots.
                    for t in &active_tasks {
                        if keep.len() >= ACTIVE_LIMIT as usize {
                            break;
                        }
                        keep.insert(t.id.clone());
                    }

                    // Suggest parking a few of the remaining ACTIVE tasks (bounded).
                    let mut parked = 0usize;
                    for t in active_tasks.drain(..) {
                        if keep.contains(&t.id) {
                            continue;
                        }
                        if parked >= 5 {
                            break;
                        }
                        patches.push(Self::lint_patch(
                            &format!("patch:plan:active_limit:park:{}", t.id),
                            "Park extra ACTIVE task to TODO (anti-kasha)",
                            "tasks_complete",
                            json!({
                                "workspace": workspace.as_str(),
                                "task": t.id,
                                "expected_revision": t.revision,
                                "status": "TODO"
                            }),
                            Some("This is a safe hygiene move. You can re-activate later when it becomes next."),
                        ));
                        parked += 1;
                    }

                    let has_list_action = actions.iter().any(|action| {
                        action
                            .get("id")
                            .and_then(|v| v.as_str())
                            .is_some_and(|id| id == "action:plan:list_active_tasks")
                    });
                    if !has_list_action {
                        // Provide a navigation action to list all ACTIVE tasks when the plan is overloaded.
                        actions.push(json!({
                            "id": "action:plan:list_active_tasks",
                            "purpose": "List ACTIVE tasks for this plan (to choose what to park)",
                            "apply": {
                                "tool": "tasks_context",
                                "arguments": {
                                    "workspace": workspace.as_str(),
                                    "plan": target_id,
                                    "tasks_status": "ACTIVE",
                                    "tasks_limit": 50,
                                    "max_chars": 4000
                                }
                            }
                        }));
                    }
                }
            }
            TaskKind::Task => match self.store.task_steps_summary(&workspace, &target_id) {
                Ok(summary) => {
                    let task = match self.store.get_task(&workspace, &target_id) {
                        Ok(Some(task)) => task,
                        Ok(None) => return ai_error("UNKNOWN_ID", "Unknown id"),
                        Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
                    };

                    // Actionless ACTIVE tasks are a common “plan soup” failure mode:
                    // users see the task as active, but there is no executable next step.
                    if task.status == "ACTIVE" && summary.total_steps > 0 && summary.open_steps == 0
                    {
                        issues.push(Self::lint_issue(
                            "warning",
                            "actionless",
                            "ACTIVE_NO_OPEN_STEPS",
                            "task is ACTIVE but all steps are completed",
                            "Mark task DONE (or decompose new steps if more work is discovered).",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));
                        patches.push(Self::lint_patch(
                            &format!("patch:task:{}:mark_done", target_id),
                            "Mark task DONE (it has no open steps)",
                            "tasks_complete",
                            json!({
                                "workspace": workspace.as_str(),
                                "task": target_id,
                                "expected_revision": task.revision,
                                "status": "DONE"
                            }),
                            Some("If more work appears, add steps via tasks_decompose and re-activate explicitly."),
                        ));
                    }

                    // Meaning binding: tasks without anchors cannot participate in the meaning map,
                    // so `where=` stays unknown and agents re-invent context after /compact.
                    let anchors = self
                        .store
                        .task_anchors_list(
                            &workspace,
                            bm_storage::TaskAnchorsListRequest {
                                task_id: target_id.clone(),
                                limit: 1,
                            },
                        )
                        .ok()
                        .map(|r| r.anchors.len())
                        .unwrap_or(0);
                    if anchors == 0 {
                        issues.push(Self::lint_issue(
                            "warning",
                            "unnavigable",
                            "MISSING_ANCHOR",
                            "task has no anchors (anchor missing)".to_string(),
                            "Attach an anchor so the task is findable by meaning and old decisions are discoverable.",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));

                        let (anchor_id, anchor_title, anchor_kind) =
                            Self::default_anchor_seed(&task.title);

                        let anchor_exists = self
                            .store
                            .anchor_get(
                                &workspace,
                                bm_storage::AnchorGetRequest {
                                    id: anchor_id.clone(),
                                },
                            )
                            .ok()
                            .flatten()
                            .is_some();

                        let mut args = json!({
                            "workspace": workspace.as_str(),
                            "target": target_id.clone(),
                            "anchor": anchor_id,
                            "content": "Anchor binding: attach meaning to this task so resume/map works.\n\n(Replace this note later with real decisions/evidence.)",
                            "card_type": "note",
                            "visibility": "canon"
                        });
                        if !anchor_exists && let Some(obj) = args.as_object_mut() {
                            obj.insert("title".to_string(), Value::String(anchor_title));
                            obj.insert("kind".to_string(), Value::String(anchor_kind));
                        }

                        patches.push(Self::lint_patch(
                            &format!("patch:task:missing_anchor:attach:{target_id}"),
                            "Attach a minimal anchor to this task (one-command)",
                            "macro_anchor_note",
                            args,
                            Some("This is a low-noise “meaning binding” note. It should be kept short; supersede with real decisions/evidence."),
                        ));
                    }

                    if summary.total_steps == 0 {
                        issues.push(Self::lint_issue(
                            "warning",
                            "actionless",
                            "NO_STEPS",
                            "task has no steps",
                            "Use tasks_decompose to add steps.",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));
                        patches.push(Self::lint_patch(
                            "patch:task:seed_steps:v1",
                            "Seed a minimal 3-step skeleton",
                            "tasks_decompose",
                            json!({
                                "workspace": workspace.as_str(),
                                "task": target_id,
                                "expected_revision": task.revision,
                                "steps": [
                                    { "title": "Research: clarify unknowns", "success_criteria": ["Hypothesis + falsifier test captured"] },
                                    { "title": "Implement: apply the change", "success_criteria": ["Change implemented", "No regressions"] },
                                    { "title": "Verify: prove it works", "success_criteria": ["Relevant tests executed", "Proof recorded"] }
                                ]
                            }),
                            Some("Refine these titles/criteria incrementally; do not over-plan upfront."),
                        ));
                    }
                    if summary.missing_criteria > 0 {
                        let step_id = self
                            .store
                            .task_first_open_step_id_unconfirmed(&workspace, &target_id, "criteria")
                            .ok()
                            .flatten();
                        let step_detail = step_id.as_deref().and_then(|step_id| {
                            self.store
                                .step_detail(&workspace, &target_id, Some(step_id), None)
                                .ok()
                        });
                        let target = step_detail.as_ref().map(|step| {
                            json!({ "kind": "step", "step_id": step.step_id, "path": step.path })
                        });

                        issues.push(Self::lint_issue(
                            "warning",
                            "unverifiable",
                            "MISSING_CRITERIA",
                            format!("missing criteria checkpoints: {}", summary.missing_criteria),
                            "Define success_criteria (if empty) and confirm criteria checkpoint.",
                            target,
                        ));

                        if let Some(step) = step_detail {
                            if step.success_criteria.is_empty() {
                                let (criteria_seed, _tests_seed) =
                                    Self::default_step_seed(&step.title);
                                patches.push(Self::lint_patch(
                                    &format!("patch:step:{}:seed_success_criteria", step.step_id),
                                    "Seed minimal success_criteria",
                                    "tasks_patch",
                                    json!({
                                        "workspace": workspace.as_str(),
                                        "task": target_id,
                                        "expected_revision": task.revision,
                                        "kind": "step",
                                        "step_id": step.step_id,
                                        "ops": [{
                                            "op": "append",
                                            "field": "success_criteria",
                                            "value": criteria_seed
                                        }]
                                    }),
                                    Some("Edit these criteria to match the real DoD; then confirm via tasks_verify."),
                                ));
                            } else {
                                patches.push(Self::lint_patch(
                                    &format!("patch:step:{}:confirm_criteria", step.step_id),
                                    "Confirm criteria checkpoint",
                                    "tasks_verify",
                                    json!({
                                        "workspace": workspace.as_str(),
                                        "task": target_id,
                                        "expected_revision": task.revision,
                                        "step_id": step.step_id,
                                        "checkpoints": { "criteria": true }
                                    }),
                                    Some("Only confirms that existing success_criteria are acceptable."),
                                ));
                            }
                        }
                    }
                    if summary.missing_tests > 0 {
                        let step_id = self
                            .store
                            .task_first_open_step_id_unconfirmed(&workspace, &target_id, "tests")
                            .ok()
                            .flatten();
                        let step_detail = step_id.as_deref().and_then(|step_id| {
                            self.store
                                .step_detail(&workspace, &target_id, Some(step_id), None)
                                .ok()
                        });
                        let target = step_detail.as_ref().map(|step| {
                            json!({ "kind": "step", "step_id": step.step_id, "path": step.path })
                        });
                        issues.push(Self::lint_issue(
                            "warning",
                            "unproveable",
                            "MISSING_TESTS",
                            format!("missing tests checkpoints: {}", summary.missing_tests),
                            "Add tests (if empty) and confirm tests checkpoint.",
                            target,
                        ));

                        if let Some(step) = step_detail {
                            if step.tests.is_empty() {
                                let (_criteria_seed, tests_seed) =
                                    Self::default_step_seed(&step.title);
                                patches.push(Self::lint_patch(
                                    &format!("patch:step:{}:seed_tests", step.step_id),
                                    "Seed minimal tests list",
                                    "tasks_patch",
                                    json!({
                                        "workspace": workspace.as_str(),
                                        "task": target_id,
                                        "expected_revision": task.revision,
                                        "kind": "step",
                                        "step_id": step.step_id,
                                        "ops": [{
                                            "op": "append",
                                            "field": "tests",
                                            "value": tests_seed
                                        }]
                                    }),
                                    Some("Replace with the real tests/commands you will run; then confirm via tasks_verify."),
                                ));
                            } else {
                                patches.push(Self::lint_patch(
                                    &format!("patch:step:{}:confirm_tests", step.step_id),
                                    "Confirm tests checkpoint",
                                    "tasks_verify",
                                    json!({
                                        "workspace": workspace.as_str(),
                                        "task": target_id,
                                        "expected_revision": task.revision,
                                        "step_id": step.step_id,
                                        "checkpoints": { "tests": true }
                                    }),
                                    Some("Only confirms that existing tests list is acceptable."),
                                ));
                            }
                        }
                    }
                    if summary.missing_security > 0 {
                        issues.push(Self::lint_issue(
                            "warning",
                            "unproveable",
                            "MISSING_SECURITY",
                            format!("missing security checkpoints: {}", summary.missing_security),
                            "Confirm security checkpoint via tasks_verify (only if required).",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));
                    }
                    if summary.missing_perf > 0 {
                        issues.push(Self::lint_issue(
                            "warning",
                            "unproveable",
                            "MISSING_PERF",
                            format!("missing perf checkpoints: {}", summary.missing_perf),
                            "Confirm perf checkpoint via tasks_verify (only if required).",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));
                    }
                    if summary.missing_docs > 0 {
                        issues.push(Self::lint_issue(
                            "warning",
                            "unproveable",
                            "MISSING_DOCS",
                            format!("missing docs checkpoints: {}", summary.missing_docs),
                            "Confirm docs checkpoint via tasks_verify (only if required).",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));
                    }

                    if let Some(first) = &summary.first_open {
                        if first.next_action.as_deref().unwrap_or("").trim().is_empty() {
                            issues.push(Self::lint_issue(
                                "warning",
                                "actionless",
                                "MISSING_NEXT_ACTION",
                                "first open step has no next_action",
                                "Set a single copy/paste next_action so the task remains executable after /compact.",
                                Some(json!({ "kind": "step", "step_id": first.step_id, "path": first.path })),
                            ));

                            let title_lc = first.title.to_lowercase();
                            let next_action_seed = if title_lc.contains("research") {
                                "Write 1 hypothesis + 1 falsifier test."
                            } else if title_lc.contains("verify") || title_lc.contains("prove") {
                                "Run the relevant tests and capture proof (CMD/LINK/REF)."
                            } else if title_lc.contains("implement") {
                                "Implement the change behind the smallest safe switch."
                            } else {
                                "Do the smallest next executable action."
                            };

                            patches.push(Self::lint_patch(
                                &format!("patch:step:{}:set_next_action", first.step_id),
                                "Set next_action (copy/paste)",
                                "tasks_patch",
                                json!({
                                    "workspace": workspace.as_str(),
                                    "task": target_id,
                                    "expected_revision": task.revision,
                                    "kind": "step",
                                    "step_id": first.step_id,
                                    "ops": [{
                                        "op": "set",
                                        "field": "next_action",
                                        "value": next_action_seed
                                    }]
                                }),
                                Some("Edit the wording to match your real next command; keep it as 1 atomic action."),
                            ));
                        }

                        let title_lc = first.title.to_lowercase();
                        let looks_like_research = title_lc.contains("research");
                        if looks_like_research
                            && first
                                .stop_criteria
                                .as_deref()
                                .unwrap_or("")
                                .trim()
                                .is_empty()
                        {
                            issues.push(Self::lint_issue(
                                "warning",
                                "unbounded",
                                "RESEARCH_MISSING_STOP_CRITERIA",
                                "research step has no stop_criteria",
                                "Set stop_criteria so long investigations do not become endless loops.",
                                Some(json!({ "kind": "step", "step_id": first.step_id, "path": first.path })),
                            ));
                            patches.push(Self::lint_patch(
                                &format!("patch:step:{}:set_stop_criteria", first.step_id),
                                "Set stop_criteria (bounded)",
                                "tasks_patch",
                                json!({
                                    "workspace": workspace.as_str(),
                                    "task": target_id,
                                    "expected_revision": task.revision,
                                    "kind": "step",
                                    "step_id": first.step_id,
                                    "ops": [{
                                        "op": "set",
                                        "field": "stop_criteria",
                                        "value": "Stop after 2h or after the falsifier test result is recorded."
                                    }]
                                }),
                                Some("Tune the time/budget/signal. If you can't define a falsifier, the research question is too vague."),
                            ));
                        }
                    }

                    if summary.total_steps > 0 && summary.open_steps_require_proof_tests == 0 {
                        issues.push(Self::lint_issue(
                            "warning",
                            "unproveable",
                            "MISSING_PROOF_PLAN",
                            "task has no open step with proof_tests_mode=require",
                            "Mark at least one step as proof-required so DONE cannot happen without evidence.",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));

                        let proof_step_id = self
                            .store
                            .list_task_steps(&workspace, &target_id, None, 50)
                            .ok()
                            .and_then(|steps| {
                                steps
                                    .into_iter()
                                    .filter(|s| !s.completed)
                                    .find(|s| {
                                        let t = s.title.to_lowercase();
                                        t.contains("verify") || t.contains("prove")
                                    })
                                    .map(|s| s.step_id)
                            })
                            .or_else(|| summary.first_open.as_ref().map(|s| s.step_id.clone()));

                        if let Some(step_id) = proof_step_id {
                            patches.push(Self::lint_patch(
                                &format!("patch:step:{step_id}:require_proof_tests"),
                                "Require tests proof on a step",
                                "tasks_patch",
                                json!({
                                    "workspace": workspace.as_str(),
                                    "task": target_id,
                                    "expected_revision": task.revision,
                                    "kind": "step",
                                    "step_id": step_id,
                                    "ops": [{
                                        "op": "set",
                                        "field": "proof_tests_mode",
                                        "value": "require"
                                    }]
                                }),
                                Some("Pick the step where you will actually run verifications; then attach proof via tasks_evidence_capture."),
                            ));
                        }
                    }

                    // Proof modes (strict) are the “DONE means DONE” mechanism: if a step requires proof
                    // but none is recorded, we should guide users to attach evidence instead of looping.
                    if let Some(first) = &summary.first_open {
                        let mut add_proof_patch = |checkpoint: &str, patch_id: String| {
                            patches.push(Self::lint_patch(
                                &patch_id,
                                "Capture proof evidence (receipt + refs)",
                                "tasks_evidence_capture",
                                json!({
                                    "workspace": workspace.as_str(),
                                    "task": target_id,
                                    "expected_revision": task.revision,
                                    "step_id": first.step_id,
                                    "checkpoint": checkpoint,
                                    "checks": [
                                        "CMD: <paste the command you ran>",
                                        "LINK: <paste a URL if applicable>",
                                        "REF: <CARD-*/TASK-*/notes@seq if applicable>"
                                    ]
                                }),
                                Some("Replace placeholders with real receipts. This attaches durable proof without completing the step."),
                            ));
                        };

                        if first.proof_tests_mode == bm_storage::ProofMode::Require
                            && !first.proof_tests_present
                        {
                            issues.push(Self::lint_issue(
                                "warning",
                                "unproveable",
                                "MISSING_PROOF_TESTS",
                                "tests proof is required but no evidence is recorded",
                                "Attach proof evidence for checkpoint=tests (CMD/LINK/refs).",
                                Some(json!({ "kind": "step", "step_id": first.step_id, "path": first.path })),
                            ));
                            add_proof_patch(
                                "tests",
                                format!("patch:step:{}:capture_proof:tests", first.step_id),
                            );
                        }
                        if first.proof_security_mode == bm_storage::ProofMode::Require
                            && !first.proof_security_present
                        {
                            issues.push(Self::lint_issue(
                                "warning",
                                "unproveable",
                                "MISSING_PROOF_SECURITY",
                                "security proof is required but no evidence is recorded",
                                "Attach proof evidence for checkpoint=security (CMD/LINK/refs).",
                                Some(json!({ "kind": "step", "step_id": first.step_id, "path": first.path })),
                            ));
                            add_proof_patch(
                                "security",
                                format!("patch:step:{}:capture_proof:security", first.step_id),
                            );
                        }
                        if first.proof_perf_mode == bm_storage::ProofMode::Require
                            && !first.proof_perf_present
                        {
                            issues.push(Self::lint_issue(
                                "warning",
                                "unproveable",
                                "MISSING_PROOF_PERF",
                                "perf proof is required but no evidence is recorded",
                                "Attach proof evidence for checkpoint=perf (CMD/LINK/refs).",
                                Some(json!({ "kind": "step", "step_id": first.step_id, "path": first.path })),
                            ));
                            add_proof_patch(
                                "perf",
                                format!("patch:step:{}:capture_proof:perf", first.step_id),
                            );
                        }
                        if first.proof_docs_mode == bm_storage::ProofMode::Require
                            && !first.proof_docs_present
                        {
                            issues.push(Self::lint_issue(
                                "warning",
                                "unproveable",
                                "MISSING_PROOF_DOCS",
                                "docs proof is required but no evidence is recorded",
                                "Attach proof evidence for checkpoint=docs (CMD/LINK/refs).",
                                Some(json!({ "kind": "step", "step_id": first.step_id, "path": first.path })),
                            ));
                            add_proof_patch(
                                "docs",
                                format!("patch:step:{}:capture_proof:docs", first.step_id),
                            );
                        }
                    }
                    if let Ok(blockers) = self.store.task_open_blockers(&workspace, &target_id, 1)
                        && !blockers.is_empty()
                    {
                        issues.push(Self::lint_issue(
                            "warning",
                            "unnavigable",
                            "BLOCKED_STEPS",
                            "task has blocked steps",
                            "Use tasks_resume_super (explore) to locate blockers; clear via tasks_block.",
                            Some(json!({ "kind": "task", "id": target_id })),
                        ));
                    }
                }
                Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
                Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
            },
        }

        let context_health = match self.build_context_health(&workspace, &target_id, kind) {
            Ok(v) => v,
            Err(StoreError::UnknownId) => return ai_error("UNKNOWN_ID", "Unknown id"),
            Err(err) => return ai_error("STORE_ERROR", &format_store_error(err)),
        };

        // Deterministic ordering: severity -> kind -> code -> target.
        issues.sort_by(|a, b| {
            let sev_a = a.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            let sev_b = b.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            let sev_rank = |sev: &str| if sev == "error" { 0 } else { 1 };
            let kind_a = a.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let kind_b = b.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let code_a = a.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let code_b = b.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let tgt_a = a
                .get("target")
                .and_then(|v| v.get("step_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tgt_b = b
                .get("target")
                .and_then(|v| v.get("step_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            (sev_rank(sev_a), kind_a, code_a, tgt_a).cmp(&(sev_rank(sev_b), kind_b, code_b, tgt_b))
        });
        if let Some(limit) = patches_limit {
            patches = Self::select_patches(patches, limit);
        }
        patches.sort_by(|a, b| {
            let id_a = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let id_b = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            id_a.cmp(id_b)
        });
        actions.sort_by(|a, b| {
            let id_a = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let id_b = b.get("id").and_then(|v| v.as_str()).unwrap_or("");
            id_a.cmp(id_b)
        });

        let (errors, warnings) = issues.iter().fold((0, 0), |acc, item| {
            match item.get("severity").and_then(|v| v.as_str()) {
                Some("error") => (acc.0 + 1, acc.1),
                Some("warning") => (acc.0, acc.1 + 1),
                _ => acc,
            }
        });

        ai_ok(
            "lint",
            json!({
                "workspace": workspace.as_str(),
                "target": { "id": target_id, "kind": kind.as_str() },
                "summary": {
                    "errors": errors,
                    "warnings": warnings,
                    "total": errors + warnings
                },
                "issues": issues,
                "patches": patches,
                "actions": actions,
                "context_health": context_health
            }),
        )
    }
}
