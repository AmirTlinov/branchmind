#![forbid(unsafe_code)]

use crate::dto::*;
use crate::support::*;
use bm_core::ids::WorkspaceId;
use bm_core::model::TaskKind;
use bm_core::paths::StepPath;
use bm_storage::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

        let repo_root_path = guess_repo_root(&canon);
        let repo_root = repo_root_path.as_ref().map(|p| p.to_string_lossy().to_string());
        let display_name = repo_root_path
            .as_ref()
            .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| {
                // Avoid showing full absolute paths in the UI: pick the nearest meaningful folder name.
                for anc in canon.ancestors().take(10) {
                    let Some(name) = anc.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if name.is_empty() {
                        continue;
                    }
                    if name.starts_with('.') {
                        continue;
                    }
                    if matches!(name, "projects" | "Documents" | "Документы" | "documents") {
                        continue;
                    }
                    return name.to_string();
                }
                canon.to_string_lossy().to_string()
            });
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

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct ArchitectureScopeInput {
    pub kind: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct ArchitectureLensInput {
    pub scope: Option<ArchitectureScopeInput>,
    pub mode: Option<String>,
    pub include_draft: Option<bool>,
    pub time_window: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct ArchitectureProvenanceInput {
    pub scope: Option<ArchitectureScopeInput>,
    pub node_id: String,
    pub include_draft: Option<bool>,
    pub time_window: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct ArchitectureHotspotsInput {
    pub scope: Option<ArchitectureScopeInput>,
    pub include_draft: Option<bool>,
    pub time_window: Option<String>,
    pub limit: Option<usize>,
}

#[tauri::command]
pub fn architecture_lens_get(
    storage_dir: String,
    workspace: String,
    input: ArchitectureLensInput,
) -> Result<ArchitectureLensDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let built = build_architecture_lens(&mut store, &workspace, &input)?;
    Ok(built.lens)
}

#[tauri::command]
pub fn architecture_provenance_get(
    storage_dir: String,
    workspace: String,
    input: ArchitectureProvenanceInput,
) -> Result<ArchitectureProvenanceDto, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let lens_input = ArchitectureLensInput {
        scope: input.scope.clone(),
        mode: Some("combined".to_string()),
        include_draft: input.include_draft,
        time_window: input.time_window.clone(),
        limit: input.limit,
    };
    let mut built = build_architecture_lens(&mut store, &workspace, &lens_input)?;
    let node_id = input.node_id.trim().to_string();
    if node_id.is_empty() {
        return Err("node_id must not be empty".to_string());
    }
    if !built.node_ids.contains(node_id.as_str()) {
        return Err(format!("UNKNOWN_ID: {node_id}"));
    }
    Ok(ArchitectureProvenanceDto {
        node_id: node_id.clone(),
        records: built.provenance.remove(&node_id).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn architecture_hotspots_get(
    storage_dir: String,
    workspace: String,
    input: ArchitectureHotspotsInput,
) -> Result<Vec<ArchitectureHotspotDto>, String> {
    let storage_dir = validate_storage_dir(&storage_dir)?;
    let mut store = open_store_read_only(&storage_dir)?;
    let workspace = parse_workspace_id(&workspace)?;
    let lens_input = ArchitectureLensInput {
        scope: input.scope,
        mode: Some("combined".to_string()),
        include_draft: input.include_draft,
        time_window: input.time_window,
        limit: input.limit,
    };
    let built = build_architecture_lens(&mut store, &workspace, &lens_input)?;
    Ok(built.lens.hotspots)
}

struct BuiltLens {
    lens: ArchitectureLensDto,
    provenance: HashMap<String, Vec<ArchitectureProvenanceRecordDto>>,
    node_ids: BTreeSet<String>,
}

struct LensState {
    nodes: BTreeMap<String, ArchitectureNodeDto>,
    edges: BTreeMap<(String, String, String), ArchitectureEdgeDto>,
    risks: Vec<ArchitectureRiskDto>,
    provenance: HashMap<String, Vec<ArchitectureProvenanceRecordDto>>,
}

impl LensState {
    fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            risks: Vec::new(),
            provenance: HashMap::new(),
        }
    }

    fn upsert_node(&mut self, node: ArchitectureNodeDto) {
        match self.nodes.get_mut(&node.id) {
            Some(existing) => {
                if node.last_ts_ms > existing.last_ts_ms {
                    existing.last_ts_ms = node.last_ts_ms;
                }
                existing.risk_score = existing.risk_score.max(node.risk_score);
                existing.evidence_score = existing.evidence_score.max(node.evidence_score);
                if existing.status.is_none() {
                    existing.status = node.status;
                }
                for tag in node.tags {
                    if !existing.tags.contains(&tag) {
                        existing.tags.push(tag);
                    }
                }
                for r in node.refs {
                    if !existing.refs.contains(&r) {
                        existing.refs.push(r);
                    }
                }
            }
            None => {
                self.nodes.insert(node.id.clone(), node);
            }
        }
    }

    fn add_edge(&mut self, from: &str, to: &str, rel: &str, risk: bool) {
        if from.trim().is_empty() || to.trim().is_empty() {
            return;
        }
        let key = (from.to_string(), to.to_string(), rel.to_string());
        if let Some(existing) = self.edges.get_mut(&key) {
            existing.weight = existing.weight.saturating_add(1);
            existing.risk = existing.risk || risk;
            return;
        }
        self.edges.insert(
            key,
            ArchitectureEdgeDto {
                from: from.to_string(),
                to: to.to_string(),
                rel: rel.to_string(),
                weight: 1,
                risk,
            },
        );
    }

    fn add_provenance(&mut self, node_id: &str, record: ArchitectureProvenanceRecordDto) {
        self.provenance
            .entry(node_id.to_string())
            .or_default()
            .push(record);
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn normalize_lens_mode(raw: Option<String>) -> String {
    let mode = raw.unwrap_or_else(|| "combined".to_string());
    let mode = mode.trim().to_ascii_lowercase();
    match mode.as_str() {
        "system" | "execution" | "reasoning" | "risk" | "combined" => mode,
        _ => "combined".to_string(),
    }
}

fn normalize_scope(scope: Option<ArchitectureScopeInput>) -> ArchitectureScopeDto {
    let kind = scope
        .as_ref()
        .and_then(|s| s.kind.clone())
        .unwrap_or_else(|| "workspace".to_string())
        .trim()
        .to_ascii_lowercase();
    let kind = match kind.as_str() {
        "workspace" | "plan" | "task" | "anchor" => kind,
        _ => "workspace".to_string(),
    };
    let id = scope
        .and_then(|s| s.id)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    ArchitectureScopeDto { kind, id }
}

fn cutoff_from_window(window: &str, ts_now: i64) -> Option<i64> {
    match window {
        "24h" => Some(ts_now.saturating_sub(86_400_000)),
        "7d" => Some(ts_now.saturating_sub(7 * 86_400_000)),
        _ => None,
    }
}

fn build_architecture_lens(
    store: &mut SqliteStore,
    workspace: &WorkspaceId,
    input: &ArchitectureLensInput,
) -> Result<BuiltLens, String> {
    let ts_now = now_ms();
    let scope = normalize_scope(input.scope.clone());
    if scope.kind != "workspace" && scope.id.is_none() {
        return Err(format!("scope.id is required for kind={}", scope.kind));
    }
    let mode = normalize_lens_mode(input.mode.clone());
    let include_draft = input.include_draft.unwrap_or(false);
    let time_window = input
        .time_window
        .clone()
        .unwrap_or_else(|| "all".to_string())
        .trim()
        .to_ascii_lowercase();
    let time_window = match time_window.as_str() {
        "24h" | "7d" | "all" => time_window,
        _ => "all".to_string(),
    };
    let cutoff_ts = cutoff_from_window(&time_window, ts_now);
    let limit = input.limit.unwrap_or(120).clamp(20, 400);

    let mut state = LensState::new();
    let mut next_actions = BTreeSet::<String>::new();

    // Synthetic system map (stable architecture skeleton).
    for (id, label) in [
        ("sys:core", "core"),
        ("sys:storage", "storage"),
        ("sys:mcp", "mcp"),
        ("sys:runner", "runner"),
        ("sys:viewer", "viewer"),
    ] {
        state.upsert_node(ArchitectureNodeDto {
            id: id.to_string(),
            label: label.to_string(),
            node_type: "system".to_string(),
            layer: "system".to_string(),
            status: Some("ok".to_string()),
            tags: vec![],
            cluster_id: "cluster:system".to_string(),
            risk_score: 0.0,
            evidence_score: 0.0,
            last_ts_ms: ts_now,
            refs: vec![format!("code:crates/{label}")],
        });
    }
    state.add_edge("sys:mcp", "sys:core", "invokes", false);
    state.add_edge("sys:storage", "sys:core", "persists", false);
    state.add_edge("sys:mcp", "sys:storage", "reads_writes", false);
    state.add_edge("sys:runner", "sys:mcp", "delegates", false);
    state.add_edge("sys:viewer", "sys:storage", "read_only", false);

    let anchors_result = store
        .anchors_list(
            workspace,
            AnchorsListRequest {
                text: None,
                kind: None,
                status: None,
                limit,
            },
        )
        .map_err(store_err_to_string)?;
    let mut anchor_rows = anchors_result.anchors;

    if scope.kind == "anchor" {
        if let Some(anchor_id) = scope.id.as_ref() {
            anchor_rows.retain(|a| a.id == *anchor_id);
        }
    }

    if scope.kind == "task" {
        if let Some(task_id) = scope.id.as_ref() {
            let hits = store
                .task_anchors_list(
                    workspace,
                    TaskAnchorsListRequest {
                        task_id: task_id.clone(),
                        limit,
                    },
                )
                .map_err(store_err_to_string)?;
            let set: BTreeSet<String> = hits.anchors.into_iter().map(|h| h.anchor_id).collect();
            anchor_rows.retain(|a| set.contains(&a.id));
        }
    }

    if scope.kind == "plan" {
        if let Some(plan_id) = scope.id.as_ref() {
            let coverage = store
                .plan_anchors_coverage(
                    workspace,
                    PlanAnchorsCoverageRequest {
                        plan_id: plan_id.clone(),
                        top_anchors_limit: limit.min(120),
                    },
                )
                .map_err(store_err_to_string)?;
            let set: BTreeSet<String> = coverage.top_anchors.into_iter().map(|h| h.anchor_id).collect();
            anchor_rows.retain(|a| set.contains(&a.id));
            if coverage.active_missing_anchor > 0 {
                state.risks.push(ArchitectureRiskDto {
                    id: "risk:plan:missing-anchors".to_string(),
                    severity: "high".to_string(),
                    title: "Active tasks without anchor mapping".to_string(),
                    reason: format!(
                        "{} active tasks in plan are missing anchor links",
                        coverage.active_missing_anchor
                    ),
                    node_id: None,
                    refs: vec![format!("PLAN:{plan_id}")],
                });
                next_actions.insert("Attach anchors to active plan tasks".to_string());
            }
        }
    }

    let anchor_ids: Vec<String> = anchor_rows.iter().map(|a| a.id.clone()).collect();
    let key_counts = store
        .count_knowledge_keys_for_anchors(workspace, &anchor_ids)
        .map_err(store_err_to_string)?;

    for anchor in &anchor_rows {
        if let Some(cutoff) = cutoff_ts {
            if anchor.updated_at_ms < cutoff {
                continue;
            }
        }
        let key_count = *key_counts.get(anchor.id.as_str()).unwrap_or(&0);
        let status = if anchor.status.trim().is_empty() {
            None
        } else {
            Some(anchor.status.clone())
        };
        let mut tags = vec![format!("anchor:{}", anchor.kind)];
        if key_count > 0 {
            tags.push(format!("keys:{key_count}"));
        }
        state.upsert_node(ArchitectureNodeDto {
            id: anchor.id.clone(),
            label: anchor.title.clone(),
            node_type: "anchor".to_string(),
            layer: "system".to_string(),
            status,
            tags,
            cluster_id: format!("cluster:anchor:{}", anchor.kind),
            risk_score: if key_count == 0 { 0.35 } else { 0.0 },
            evidence_score: key_count as f64,
            last_ts_ms: anchor.updated_at_ms,
            refs: vec![format!("a:{}", anchor.id)],
        });
        state.add_provenance(
            &anchor.id,
            ArchitectureProvenanceRecordDto {
                kind: "anchor".to_string(),
                id: anchor.id.clone(),
                label: Some(anchor.title.clone()),
                note: anchor.description.clone(),
                ts_ms: Some(anchor.updated_at_ms),
                refs: vec![format!("a:{}", anchor.id)],
            },
        );
        if key_count == 0 {
            state.risks.push(ArchitectureRiskDto {
                id: format!("risk:anchor:{}:no-knowledge", anchor.id),
                severity: "medium".to_string(),
                title: format!("Anchor {} lacks knowledge cards", anchor.id),
                reason: "No anchor knowledge keys found".to_string(),
                node_id: Some(anchor.id.clone()),
                refs: vec![format!("a:{}", anchor.id)],
            });
        }
    }

    // Knowledge cards indexed by anchor+key.
    if !anchor_ids.is_empty() {
        let keys = store
            .knowledge_keys_list_any(
                workspace,
                KnowledgeKeysListAnyRequest {
                    anchor_ids: anchor_ids.clone(),
                    limit: limit.saturating_mul(4),
                },
            )
            .map_err(store_err_to_string)?;
        for key in keys.items {
            let knowledge_id = format!("k:{}:{}", key.anchor_id, key.key);
            state.upsert_node(ArchitectureNodeDto {
                id: knowledge_id.clone(),
                label: key.key.clone(),
                node_type: "knowledge".to_string(),
                layer: "reasoning".to_string(),
                status: Some("canon".to_string()),
                tags: vec![format!("anchor:{}", key.anchor_id)],
                cluster_id: "cluster:knowledge".to_string(),
                risk_score: 0.0,
                evidence_score: 0.8,
                last_ts_ms: key.updated_at_ms,
                refs: vec![key.card_id.clone()],
            });
            state.add_edge(&key.anchor_id, &knowledge_id, "knows", false);
            state.add_provenance(
                &knowledge_id,
                ArchitectureProvenanceRecordDto {
                    kind: "knowledge_key".to_string(),
                    id: key.card_id.clone(),
                    label: Some(format!("{} / {}", key.anchor_id, key.key)),
                    note: None,
                    ts_ms: Some(key.updated_at_ms),
                    refs: vec![key.card_id.clone()],
                },
            );
        }
    }

    let all_tasks = store
        .list_tasks(workspace, limit.saturating_mul(4), 0)
        .map_err(store_err_to_string)?;
    let mut task_rows = all_tasks;
    if scope.kind == "task" {
        if let Some(task_id) = scope.id.as_ref() {
            task_rows.retain(|t| t.id == *task_id);
        }
    } else if scope.kind == "plan" {
        if let Some(plan_id) = scope.id.as_ref() {
            task_rows.retain(|t| t.parent_plan_id == *plan_id);
        }
    } else if scope.kind == "anchor" {
        if anchor_ids.is_empty() {
            task_rows.clear();
        } else {
            let task_hits = store
                .anchor_tasks_list_any(
                    workspace,
                    AnchorTasksListAnyRequest {
                        anchor_ids: anchor_ids.clone(),
                        limit: limit.saturating_mul(2),
                    },
                )
                .map_err(store_err_to_string)?;
            let set: BTreeSet<String> = task_hits.tasks.into_iter().map(|t| t.task_id).collect();
            task_rows.retain(|t| set.contains(&t.id));
        }
    }
    task_rows.truncate(limit.saturating_mul(2));

    let mut task_anchor_map = HashMap::<String, Vec<String>>::new();
    for t in &task_rows {
        let anchors = store
            .task_anchors_list(
                workspace,
                TaskAnchorsListRequest {
                    task_id: t.id.clone(),
                    limit: 50,
                },
            )
            .map_err(store_err_to_string)?;
        task_anchor_map.insert(
            t.id.clone(),
            anchors.anchors.into_iter().map(|a| a.anchor_id).collect(),
        );
    }

    for task in &task_rows {
        if let Some(cutoff) = cutoff_ts {
            if task.updated_at_ms < cutoff {
                continue;
            }
        }
        let blocked = task.blocked || task.status.eq_ignore_ascii_case("blocked");
        let risk_score = if blocked { 1.0 } else { 0.0 };
        let evidence_score = if task.status.eq_ignore_ascii_case("done") {
            0.9
        } else if task.status.eq_ignore_ascii_case("active") {
            0.45
        } else {
            0.2
        };
        state.upsert_node(ArchitectureNodeDto {
            id: task.id.clone(),
            label: task.title.clone(),
            node_type: "task".to_string(),
            layer: "execution".to_string(),
            status: Some(task.status.clone()),
            tags: vec![format!("plan:{}", task.parent_plan_id)],
            cluster_id: format!("cluster:task:{}", task.status.to_ascii_lowercase()),
            risk_score,
            evidence_score,
            last_ts_ms: task.updated_at_ms,
            refs: vec![task.id.clone(), task.parent_plan_id.clone()],
        });
        state.add_provenance(
            &task.id,
            ArchitectureProvenanceRecordDto {
                kind: "task".to_string(),
                id: task.id.clone(),
                label: Some(task.title.clone()),
                note: Some(format!("status={} blocked={}", task.status, blocked)),
                ts_ms: Some(task.updated_at_ms),
                refs: vec![task.id.clone(), task.parent_plan_id.clone()],
            },
        );

        if blocked {
            state.risks.push(ArchitectureRiskDto {
                id: format!("risk:task:{}:blocked", task.id),
                severity: "high".to_string(),
                title: format!("Task {} is blocked", task.id),
                reason: "Execution cannot progress while task is blocked".to_string(),
                node_id: Some(task.id.clone()),
                refs: vec![task.id.clone()],
            });
            next_actions.insert("Resolve blocked tasks first".to_string());
        }

        if let Some(anchors) = task_anchor_map.get(task.id.as_str()) {
            for anchor_id in anchors {
                state.add_edge(&task.id, anchor_id, "touches", blocked);
            }
        }
    }

    // Pull reasoning graph slices for selected tasks.
    for task in task_rows.iter().take(24) {
        let ref_row = store
            .reasoning_ref_get(workspace, task.id.as_str(), TaskKind::Task)
            .map_err(store_err_to_string)?;
        let Some(reasoning_ref) = ref_row else {
            continue;
        };
        let slice = store
            .graph_query(
                workspace,
                reasoning_ref.branch.as_str(),
                reasoning_ref.graph_doc.as_str(),
                GraphQueryRequest {
                    ids: None,
                    types: None,
                    status: None,
                    tags_any: None,
                    tags_all: None,
                    text: None,
                    cursor: None,
                    limit: limit.min(220),
                    include_edges: true,
                    edges_limit: limit.saturating_mul(4),
                },
            )
            .map_err(store_err_to_string)?;

        let mut id_allowed = BTreeSet::<String>::new();
        for node in slice.nodes {
            if node.deleted {
                continue;
            }
            if !include_draft && node.tags.iter().any(|t| t.eq_ignore_ascii_case("v:draft")) {
                continue;
            }
            if let Some(cutoff) = cutoff_ts {
                if node.last_ts_ms < cutoff {
                    continue;
                }
            }
            let node_type = node.node_type.to_ascii_lowercase();
            let evidence_score = if matches!(node_type.as_str(), "evidence" | "decision" | "test") {
                1.0
            } else if node_type == "hypothesis" {
                0.2
            } else {
                0.4
            };
            let mut risk_score: f64 = 0.0;
            if node_type == "hypothesis" {
                risk_score = 0.5;
            }
            if node
                .status
                .as_ref()
                .map(|s| s.eq_ignore_ascii_case("blocked") || s.eq_ignore_ascii_case("conflict"))
                .unwrap_or(false)
            {
                risk_score = risk_score.max(0.9);
            }
            let layer = if matches!(node_type.as_str(), "task" | "step") {
                "execution"
            } else {
                "reasoning"
            };
            let node_id = node.id.clone();
            id_allowed.insert(node_id.clone());
            state.upsert_node(ArchitectureNodeDto {
                id: node_id.clone(),
                label: node
                    .title
                    .clone()
                    .or_else(|| node.text.clone())
                    .unwrap_or_else(|| node_id.clone()),
                node_type: node.node_type.clone(),
                layer: layer.to_string(),
                status: node.status.clone(),
                tags: node.tags.clone(),
                cluster_id: format!("cluster:{}:{}", layer, node_type),
                risk_score,
                evidence_score,
                last_ts_ms: node.last_ts_ms,
                refs: vec![
                    task.id.clone(),
                    format!("{}@{}", reasoning_ref.graph_doc, node.last_seq),
                    node_id.clone(),
                ],
            });
            state.add_provenance(
                &node_id,
                ArchitectureProvenanceRecordDto {
                    kind: "graph_node".to_string(),
                    id: node_id.clone(),
                    label: node.title.clone(),
                    note: Some(format!(
                        "branch={} doc={} type={}",
                        reasoning_ref.branch, reasoning_ref.graph_doc, node.node_type
                    )),
                    ts_ms: Some(node.last_ts_ms),
                    refs: vec![task.id.clone(), format!("{}@{}", reasoning_ref.graph_doc, node.last_seq)],
                },
            );

            if node_type == "hypothesis" {
                state.risks.push(ArchitectureRiskDto {
                    id: format!("risk:hypothesis:{node_id}"),
                    severity: "medium".to_string(),
                    title: "Unproven hypothesis".to_string(),
                    reason: "Hypothesis node requires supporting evidence/tests".to_string(),
                    node_id: Some(node_id.clone()),
                    refs: vec![node_id.clone(), task.id.clone()],
                });
                next_actions.insert("Convert hypotheses into tests/evidence".to_string());
            }
        }

        for edge in slice.edges {
            if edge.deleted {
                continue;
            }
            if !id_allowed.contains(edge.from.as_str()) || !id_allowed.contains(edge.to.as_str()) {
                continue;
            }
            let is_risk = edge.rel.eq_ignore_ascii_case("blocks");
            state.add_edge(edge.from.as_str(), edge.to.as_str(), edge.rel.as_str(), is_risk);
        }
    }

    // Final summaries.
    let blocked_total = state
        .nodes
        .values()
        .filter(|n| {
            n.status
                .as_ref()
                .map(|s| s.eq_ignore_ascii_case("blocked") || s.eq_ignore_ascii_case("conflict"))
                .unwrap_or(false)
        })
        .count();
    let evidence_total = state
        .nodes
        .values()
        .filter(|n| {
            let t = n.node_type.to_ascii_lowercase();
            matches!(t.as_str(), "evidence" | "decision" | "test")
        })
        .count();
    let hypothesis_total = state
        .nodes
        .values()
        .filter(|n| n.node_type.eq_ignore_ascii_case("hypothesis"))
        .count();
    let denominator = evidence_total + hypothesis_total;
    let proven_ratio = if denominator == 0 {
        1.0
    } else {
        evidence_total as f64 / denominator as f64
    };
    if denominator > 0 && proven_ratio < 0.55 {
        next_actions.insert("Increase proof coverage for active architecture threads".to_string());
    }
    if anchor_rows.is_empty() {
        next_actions.insert("Define canonical anchors for project architecture".to_string());
    }

    let mut nodes: Vec<ArchitectureNodeDto> = state.nodes.into_values().collect();
    let mut edges: Vec<ArchitectureEdgeDto> = state.edges.into_values().collect();

    apply_mode_filter(&mode, &mut nodes, &mut edges);

    let node_set: BTreeSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let mut degree = BTreeMap::<String, usize>::new();
    for e in &edges {
        *degree.entry(e.from.clone()).or_insert(0) += 1;
        *degree.entry(e.to.clone()).or_insert(0) += 1;
    }

    let mut clusters_map = BTreeMap::<String, ArchitectureClusterDto>::new();
    for n in &nodes {
        let entry = clusters_map.entry(n.cluster_id.clone()).or_insert(ArchitectureClusterDto {
            id: n.cluster_id.clone(),
            label: n
                .cluster_id
                .trim_start_matches("cluster:")
                .replace(':', " / ")
                .to_string(),
            layer: n.layer.clone(),
            node_count: 0,
            risk_count: 0,
            proven_ratio: 0.0,
        });
        entry.node_count = entry.node_count.saturating_add(1);
        if n.risk_score > 0.45 {
            entry.risk_count = entry.risk_count.saturating_add(1);
        }
        entry.proven_ratio += n.evidence_score;
    }
    for c in clusters_map.values_mut() {
        if c.node_count > 0 {
            c.proven_ratio /= c.node_count as f64;
        }
    }

    let mut hotspots: Vec<ArchitectureHotspotDto> = nodes
        .iter()
        .map(|n| ArchitectureHotspotDto {
            id: n.id.clone(),
            label: n.label.clone(),
            node_type: n.node_type.clone(),
            degree: *degree.get(n.id.as_str()).unwrap_or(&0),
            risk_score: n.risk_score,
            evidence_score: n.evidence_score,
        })
        .collect();
    hotspots.sort_by(|a, b| {
        let sa = (a.degree as f64) + a.risk_score * 4.0 + (1.0 - a.evidence_score);
        let sb = (b.degree as f64) + b.risk_score * 4.0 + (1.0 - b.evidence_score);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    hotspots.truncate(12);

    let summary = ArchitectureSummaryDto {
        anchors_total: nodes.iter().filter(|n| n.node_type == "anchor").count(),
        tasks_total: nodes.iter().filter(|n| n.node_type.eq_ignore_ascii_case("task")).count(),
        knowledge_total: nodes.iter().filter(|n| n.node_type == "knowledge").count(),
        reasoning_nodes_total: nodes
            .iter()
            .filter(|n| n.layer.eq_ignore_ascii_case("reasoning"))
            .count(),
        blocked_total,
        evidence_total,
        hypothesis_total,
        proven_ratio,
    };

    let risks: Vec<ArchitectureRiskDto> = state
        .risks
        .into_iter()
        .filter(|r| r.node_id.as_ref().map(|id| node_set.contains(id)).unwrap_or(true))
        .take(20)
        .collect();

    let mut next_actions_vec: Vec<String> = next_actions.into_iter().collect();
    next_actions_vec.truncate(6);
    if next_actions_vec.is_empty() {
        next_actions_vec.push("No critical blockers detected; continue current execution lane".to_string());
    }

    let lens = ArchitectureLensDto {
        scope,
        mode,
        include_draft,
        time_window,
        generated_at_ms: ts_now,
        summary,
        clusters: clusters_map.into_values().collect(),
        nodes: nodes.clone(),
        edges,
        risks,
        hotspots,
        next_actions: next_actions_vec,
    };

    Ok(BuiltLens {
        lens,
        provenance: state.provenance,
        node_ids: node_set,
    })
}

fn apply_mode_filter(mode: &str, nodes: &mut Vec<ArchitectureNodeDto>, edges: &mut Vec<ArchitectureEdgeDto>) {
    let keep_predicate = |n: &ArchitectureNodeDto| -> bool {
        match mode {
            "system" => n.layer == "system",
            "execution" => n.layer == "execution",
            "reasoning" => n.layer == "reasoning",
            "risk" => n.risk_score > 0.45
                || n
                    .status
                    .as_ref()
                    .map(|s| s.eq_ignore_ascii_case("blocked") || s.eq_ignore_ascii_case("conflict"))
                    .unwrap_or(false),
            _ => true,
        }
    };

    nodes.retain(keep_predicate);
    let allowed: BTreeSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
    edges.retain(|e| allowed.contains(e.from.as_str()) && allowed.contains(e.to.as_str()));
}
