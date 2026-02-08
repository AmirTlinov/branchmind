#![forbid(unsafe_code)]

use crate::dto::*;
use crate::support::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use bm_storage::*;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

const KB_BRANCH: &str = "kb/main";
const KB_GRAPH_DOC: &str = "kb-graph";

fn parse_workspace_id(raw: &str) -> Result<WorkspaceId, String> {
    WorkspaceId::try_new(raw.trim().to_string())
        .map_err(|err| format!("workspace: invalid id: {err:?}"))
}

#[tauri::command]
pub fn projects_scan(
    roots: Option<Vec<String>>,
    max_depth: Option<usize>,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> Result<Vec<ProjectDto>, String> {
    let roots = roots
        .unwrap_or_else(|| default_scan_roots().iter().map(|p| p.to_string_lossy().to_string()).collect());
    let mut root_paths = Vec::new();
    for raw in roots {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        root_paths.push(PathBuf::from(trimmed));
    }

    let max_depth = max_depth.unwrap_or(7).min(20);
    let limit = limit.unwrap_or(200).clamp(1, 2000);
    let timeout_ms = timeout_ms.unwrap_or(2_000).clamp(100, 30_000);

    let storage_dirs = scan_storage_dirs(root_paths, max_depth, limit, timeout_ms)?;

    let mut out = Vec::new();
    let mut seen = BTreeSet::<PathBuf>::new();
    for storage_dir in storage_dirs {
        let canon = canonicalize_best_effort(&storage_dir);
        // scan_storage_dirs may return duplicates that canonicalize to the same physical path
        // (e.g. multiple scan roots pointing to the same mount / symlinked directory).
        if !seen.insert(canon.clone()) {
            continue;
        }
        let db = canon.join("branchmind_rust.db");
        let store = match open_store_read_only(&canon) {
            Ok(store) => store,
            Err(_) => {
                // Skip unreadable stores (best-effort scan).
                continue;
            }
        };
        let workspaces = store
            .list_workspaces(200, 0)
            .map_err(store_err_to_string)?
            .into_iter()
            .map(|row| WorkspaceDto {
                workspace: row.workspace,
                created_at_ms: row.created_at_ms,
                project_guard: row.project_guard,
            })
            .collect::<Vec<_>>();

        let repo_root = guess_repo_root(&canon).map(|p| p.to_string_lossy().to_string());
        let display_name = repo_root
            .as_ref()
            .and_then(|s| PathBuf::from(s).file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| canon.to_string_lossy().to_string());
        let project_id = canon.to_string_lossy().to_string();

        out.push(ProjectDto {
            project_id,
            display_name,
            storage_dir: canon.to_string_lossy().to_string(),
            db_path: db.to_string_lossy().to_string(),
            repo_root,
            workspaces,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn workspaces_list(storage_dir: String, limit: usize, offset: usize) -> Result<Vec<WorkspaceDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let rows = store
        .list_workspaces(limit.min(500), offset.min(50_000))
        .map_err(store_err_to_string)?;
    Ok(rows
        .into_iter()
        .map(|row| WorkspaceDto {
            workspace: row.workspace,
            created_at_ms: row.created_at_ms,
            project_guard: row.project_guard,
        })
        .collect())
}

#[tauri::command]
pub fn focus_get(storage_dir: String, workspace: String) -> Result<Option<String>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    store.focus_get(&workspace).map_err(store_err_to_string)
}

#[tauri::command]
pub fn tasks_list(storage_dir: String, workspace: String, limit: usize, offset: usize) -> Result<Vec<TaskSummaryDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let rows = store
        .list_tasks(&workspace, limit.min(500), offset.min(50_000))
        .map_err(store_err_to_string)?;
    Ok(rows
        .into_iter()
        .map(|row| TaskSummaryDto {
            id: row.id,
            parent_plan_id: row.parent_plan_id,
            title: row.title,
            status: row.status,
            priority: row.priority,
            blocked: row.blocked,
            reasoning_mode: row.reasoning_mode,
            updated_at_ms: row.updated_at_ms,
        })
        .collect())
}

#[tauri::command]
pub fn tasks_get(storage_dir: String, workspace: String, id: String) -> Result<Option<TaskDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let row = store.get_task(&workspace, id.trim()).map_err(store_err_to_string)?;
    Ok(row.map(|row| TaskDto {
        id: row.id,
        revision: row.revision,
        parent_plan_id: row.parent_plan_id,
        title: row.title,
        description: row.description,
        context: row.context,
        status: row.status,
        status_manual: row.status_manual,
        priority: row.priority,
        blocked: row.blocked,
        assignee: row.assignee,
        domain: row.domain,
        phase: row.phase,
        component: row.component,
        reasoning_mode: row.reasoning_mode,
        criteria_confirmed: row.criteria_confirmed,
        tests_confirmed: row.tests_confirmed,
        security_confirmed: row.security_confirmed,
        perf_confirmed: row.perf_confirmed,
        docs_confirmed: row.docs_confirmed,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    }))
}

#[tauri::command]
pub fn plans_get(storage_dir: String, workspace: String, id: String) -> Result<Option<PlanDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let row = store.get_plan(&workspace, id.trim()).map_err(store_err_to_string)?;
    Ok(row.map(|row| PlanDto {
        id: row.id,
        revision: row.revision,
        title: row.title,
        description: row.description,
        context: row.context,
        status: row.status,
        status_manual: row.status_manual,
        priority: row.priority,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    }))
}

#[tauri::command]
pub fn reasoning_ref_get(
    storage_dir: String,
    workspace: String,
    id: String,
    kind: String,
) -> Result<Option<ReasoningRefDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let kind = match kind.trim().to_ascii_lowercase().as_str() {
        "plan" => TaskKind::Plan,
        "task" => TaskKind::Task,
        other => return Err(format!("kind must be plan|task, got: {other}")),
    };
    let row = store
        .reasoning_ref_get(&workspace, id.trim(), kind)
        .map_err(store_err_to_string)?;
    Ok(row.map(|row| ReasoningRefDto {
        branch: row.branch,
        notes_doc: row.notes_doc,
        graph_doc: row.graph_doc,
        trace_doc: row.trace_doc,
    }))
}

#[tauri::command]
pub fn steps_list(storage_dir: String, workspace: String, task_id: String, limit: usize) -> Result<Vec<StepListDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let rows = store
        .list_task_steps(&workspace, task_id.trim(), None, limit.min(2000))
        .map_err(store_err_to_string)?;
    Ok(rows
        .into_iter()
        .map(|row| StepListDto {
            step_id: row.step_id,
            path: row.path,
            title: row.title,
            completed: row.completed,
            criteria_confirmed: row.criteria_confirmed,
            tests_confirmed: row.tests_confirmed,
            security_confirmed: row.security_confirmed,
            perf_confirmed: row.perf_confirmed,
            docs_confirmed: row.docs_confirmed,
            blocked: row.blocked,
            block_reason: row.block_reason,
            updated_at_ms: row.updated_at_ms,
        })
        .collect())
}

#[derive(Clone, Debug, Deserialize)]
pub struct StepDetailInput {
    pub step_id: Option<String>,
    pub path: Option<String>,
}

#[tauri::command]
pub fn steps_detail(
    storage_dir: String,
    workspace: String,
    task_id: String,
    selector: StepDetailInput,
) -> Result<StepDetailDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let step_id = selector.step_id.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    let path = selector.path.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    let path = match path {
        Some(p) => Some(StepPath::parse(p).map_err(|err| format!("path: invalid: {err:?}"))?),
        None => None,
    };

    let detail = store
        .step_detail(&workspace, task_id.trim(), step_id, path.as_ref())
        .map_err(store_err_to_string)?;
    Ok(step_detail_to_dto(detail))
}

#[tauri::command]
pub fn task_steps_summary(storage_dir: String, workspace: String, task_id: String) -> Result<TaskStepsSummaryDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let summary = store
        .task_steps_summary(&workspace, task_id.trim())
        .map_err(store_err_to_string)?;

    // Upgrade first_open to a full step detail for richer HUD behavior.
    let first_open = if let Some(status) = summary.first_open {
        let detail = store
            .step_detail(&workspace, task_id.trim(), Some(status.step_id.as_str()), None)
            .map_err(store_err_to_string)?;
        Some(step_detail_to_dto(detail))
    } else {
        None
    };

    Ok(TaskStepsSummaryDto {
        total_steps: summary.total_steps,
        completed_steps: summary.completed_steps,
        open_steps: summary.open_steps,
        missing_criteria: summary.missing_criteria,
        missing_tests: summary.missing_tests,
        missing_security: summary.missing_security,
        missing_perf: summary.missing_perf,
        missing_docs: summary.missing_docs,
        first_open,
    })
}

fn step_detail_to_dto(detail: StepDetail) -> StepDetailDto {
    StepDetailDto {
        step_id: detail.step_id,
        path: detail.path,
        title: detail.title,
        next_action: detail.next_action,
        stop_criteria: detail.stop_criteria,
        success_criteria: detail.success_criteria,
        tests: detail.tests,
        blockers: detail.blockers,
        criteria_confirmed: detail.criteria_confirmed,
        tests_confirmed: detail.tests_confirmed,
        security_confirmed: detail.security_confirmed,
        perf_confirmed: detail.perf_confirmed,
        docs_confirmed: detail.docs_confirmed,
        completed: detail.completed,
        blocked: detail.blocked,
        block_reason: detail.block_reason,
        proof_tests_mode: detail.proof_tests_mode.as_str().to_string(),
        proof_security_mode: detail.proof_security_mode.as_str().to_string(),
        proof_perf_mode: detail.proof_perf_mode.as_str().to_string(),
        proof_docs_mode: detail.proof_docs_mode.as_str().to_string(),
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocEntriesSinceInput {
    pub branch: String,
    pub doc: String,
    pub since_seq: i64,
    pub limit: usize,
    pub kind: Option<String>,
}

#[tauri::command]
pub fn docs_entries_since(
    storage_dir: String,
    workspace: String,
    input: DocEntriesSinceInput,
) -> Result<DocEntriesSinceDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;

    let kind = match input.kind.as_deref().map(|s| s.trim().to_ascii_lowercase()) {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) if s == "note" => Some(DocEntryKind::Note),
        Some(s) if s == "event" => Some(DocEntryKind::Event),
        Some(other) => return Err(format!("kind must be note|event, got: {other}")),
    };

    let request = DocEntriesSinceRequest {
        branch: input.branch,
        doc: input.doc,
        since_seq: input.since_seq,
        limit: input.limit.min(500),
        kind,
    };
    let result = store
        .doc_entries_since(&workspace, request)
        .map_err(store_err_to_string)?;
    Ok(DocEntriesSinceDto {
        entries: result.entries.into_iter().map(doc_entry_to_dto).collect(),
        total: result.total,
    })
}

#[tauri::command]
pub fn docs_show_tail(
    storage_dir: String,
    workspace: String,
    branch: String,
    doc: String,
    cursor: Option<i64>,
    limit: usize,
) -> Result<DocSliceDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;

    let slice = store
        .doc_show_tail(
            &workspace,
            branch.trim(),
            doc.trim(),
            cursor,
            limit.min(200),
        )
        .map_err(store_err_to_string)?;

    Ok(DocSliceDto {
        entries: slice.entries.into_iter().map(doc_entry_to_dto).collect(),
        next_cursor: slice.next_cursor,
        has_more: slice.has_more,
    })
}

fn doc_entry_to_dto(row: DocEntryRow) -> DocEntryDto {
    DocEntryDto {
        seq: row.seq,
        ts_ms: row.ts_ms,
        branch: row.branch,
        doc: row.doc,
        kind: row.kind.as_str().to_string(),
        title: row.title,
        format: row.format,
        meta_json: row.meta_json,
        content: row.content,
        source_event_id: row.source_event_id,
        event_type: row.event_type,
        task_id: row.task_id,
        path: row.path,
        payload_json: row.payload_json,
    }
}

#[tauri::command]
pub fn branches_list(storage_dir: String, workspace: String, limit: usize) -> Result<Vec<BranchDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let rows = store.branch_list(&workspace, limit.min(500)).map_err(store_err_to_string)?;
    Ok(rows
        .into_iter()
        .map(|row| BranchDto {
            name: row.name,
            base_branch: row.base_branch,
            base_seq: row.base_seq,
            created_at_ms: row.created_at_ms,
        })
        .collect())
}

#[derive(Clone, Debug, Deserialize)]
pub struct GraphQueryInput {
    pub ids: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub status: Option<String>,
    pub tags_any: Option<Vec<String>>,
    pub tags_all: Option<Vec<String>>,
    pub text: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<usize>,
    pub include_edges: Option<bool>,
    pub edges_limit: Option<usize>,
}

#[tauri::command]
pub fn graph_query(
    storage_dir: String,
    workspace: String,
    branch: String,
    doc: String,
    input: GraphQueryInput,
) -> Result<GraphSliceDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;

    let request = GraphQueryRequest {
        ids: input.ids,
        types: input.types,
        status: input.status,
        tags_any: input.tags_any,
        tags_all: input.tags_all,
        text: input.text,
        cursor: input.cursor,
        limit: input.limit.unwrap_or(200).clamp(1, 200),
        include_edges: input.include_edges.unwrap_or(true),
        edges_limit: input.edges_limit.unwrap_or(500).clamp(0, 1000),
    };

    let slice = store
        .graph_query(&workspace, branch.trim(), doc.trim(), request)
        .map_err(store_err_to_string)?;

    Ok(GraphSliceDto {
        nodes: slice.nodes.into_iter().map(graph_node_to_dto).collect(),
        edges: slice.edges.into_iter().map(graph_edge_to_dto).collect(),
        next_cursor: slice.next_cursor,
        has_more: slice.has_more,
    })
}

#[tauri::command]
pub fn graph_diff(
    storage_dir: String,
    workspace: String,
    from_branch: String,
    to_branch: String,
    doc: String,
    cursor: Option<i64>,
    limit: usize,
) -> Result<GraphDiffSliceDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let slice = store
        .graph_diff(
            &workspace,
            from_branch.trim(),
            to_branch.trim(),
            doc.trim(),
            cursor,
            limit.min(200),
        )
        .map_err(store_err_to_string)?;

    Ok(GraphDiffSliceDto {
        changes: slice
            .changes
            .into_iter()
            .map(|change| match change {
                bm_core::graph::GraphDiffChange::Node { to } => {
                    GraphDiffChangeDto::Node { to: graph_node_to_dto(to) }
                }
                bm_core::graph::GraphDiffChange::Edge { to } => {
                    GraphDiffChangeDto::Edge { to: graph_edge_to_dto(to) }
                }
            })
            .collect(),
        next_cursor: slice.next_cursor,
        has_more: slice.has_more,
    })
}

fn graph_node_to_dto(row: bm_core::graph::GraphNode) -> GraphNodeDto {
    GraphNodeDto {
        id: row.id,
        node_type: row.node_type,
        title: row.title,
        text: row.text,
        tags: row.tags,
        status: row.status,
        meta_json: row.meta_json,
        deleted: row.deleted,
        last_seq: row.last_seq,
        last_ts_ms: row.last_ts_ms,
    }
}

fn graph_edge_to_dto(row: bm_core::graph::GraphEdge) -> GraphEdgeDto {
    GraphEdgeDto {
        from: row.from,
        rel: row.rel,
        to: row.to,
        meta_json: row.meta_json,
        deleted: row.deleted,
        last_seq: row.last_seq,
        last_ts_ms: row.last_ts_ms,
    }
}

#[tauri::command]
pub fn tasks_search(storage_dir: String, workspace: String, text: String, limit: usize) -> Result<TasksSearchDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let result = store
        .search_tasks(
            &workspace,
            TasksSearchRequest {
                text,
                limit: limit.min(200),
            },
        )
        .map_err(store_err_to_string)?;
    Ok(TasksSearchDto {
        tasks: result
            .tasks
            .into_iter()
            .map(|hit| TaskSearchHitDto {
                id: hit.id,
                plan_id: hit.plan_id,
                title: hit.title,
                updated_at_ms: hit.updated_at_ms,
            })
            .collect(),
        has_more: result.has_more,
    })
}

#[tauri::command]
pub fn knowledge_search(
    storage_dir: String,
    workspace: String,
    text: String,
    limit: usize,
) -> Result<KnowledgeSearchDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let result = store
        .knowledge_keys_search(
            &workspace,
            KnowledgeKeysSearchRequest {
                text,
                limit: limit.min(200),
            },
        )
        .map_err(store_err_to_string)?;
    Ok(KnowledgeSearchDto {
        items: result
            .items
            .into_iter()
            .map(|row| KnowledgeKeyDto {
                anchor_id: row.anchor_id,
                key: row.key,
                card_id: row.card_id,
                created_at_ms: row.created_at_ms,
                updated_at_ms: row.updated_at_ms,
            })
            .collect(),
        has_more: result.has_more,
    })
}

#[tauri::command]
pub fn knowledge_card_get(
    storage_dir: String,
    workspace: String,
    card_id: String,
) -> Result<Option<GraphNodeDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let card_id = card_id.trim().to_string();
    if card_id.is_empty() {
        return Err("card_id must not be empty".to_string());
    }

    let slice = store
        .graph_query(
            &workspace,
            KB_BRANCH,
            KB_GRAPH_DOC,
            GraphQueryRequest {
                ids: Some(vec![card_id]),
                types: None,
                status: None,
                tags_any: None,
                tags_all: None,
                text: None,
                cursor: None,
                limit: 5,
                include_edges: false,
                edges_limit: 0,
            },
        )
        .map_err(store_err_to_string)?;

    Ok(slice.nodes.into_iter().next().map(graph_node_to_dto))
}

#[tauri::command]
pub fn anchors_list(
    storage_dir: String,
    workspace: String,
    text: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    limit: usize,
) -> Result<AnchorsListDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let result = store
        .anchors_list(
            &workspace,
            AnchorsListRequest {
                text,
                kind,
                status,
                limit: limit.min(200),
            },
        )
        .map_err(store_err_to_string)?;
    Ok(AnchorsListDto {
        anchors: result
            .anchors
            .into_iter()
            .map(|a| AnchorDto {
                id: a.id,
                title: a.title,
                kind: a.kind,
                description: a.description,
                status: Some(a.status).filter(|s| !s.trim().is_empty()),
                parent_id: a.parent_id,
                refs: a.refs,
                depends_on: a.depends_on,
                aliases: a.aliases,
                created_at_ms: a.created_at_ms,
                updated_at_ms: a.updated_at_ms,
            })
            .collect(),
        has_more: result.has_more,
    })
}
