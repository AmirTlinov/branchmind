import { invokeTauri } from "./tauri";
import type {
  AnchorsListDto,
  DocEntriesSinceDto,
  DocEntriesSinceInput,
  DocSliceDto,
  BranchDto,
  GraphDiffSliceDto,
  GraphQueryInput,
  GraphSliceDto,
  GraphNodeDto,
  KnowledgeSearchDto,
  PlanDto,
  ProjectDto,
  ReasoningRefDto,
  StepDetailDto,
  StepDetailInput,
  StepListDto,
  TaskDto,
  TaskStepsSummaryDto,
  TaskSummaryDto,
  TasksSearchDto,
  WorkspaceDto,
} from "./types";

export const viewerApi = {
  projectsScan: (args: {
    roots?: string[];
    max_depth?: number;
    limit?: number;
    timeout_ms?: number;
  } = {}) => invokeTauri<ProjectDto[]>("projects_scan", args),

  workspacesList: (args: { storage_dir: string; limit: number; offset: number }) =>
    invokeTauri<WorkspaceDto[]>("workspaces_list", args),

  focusGet: (args: { storage_dir: string; workspace: string }) =>
    invokeTauri<string | null>("focus_get", args),

  tasksList: (args: { storage_dir: string; workspace: string; limit: number; offset: number }) =>
    invokeTauri<TaskSummaryDto[]>("tasks_list", args),

  tasksGet: (args: { storage_dir: string; workspace: string; id: string }) =>
    invokeTauri<TaskDto | null>("tasks_get", args),

  plansGet: (args: { storage_dir: string; workspace: string; id: string }) =>
    invokeTauri<PlanDto | null>("plans_get", args),

  reasoningRefGet: (args: { storage_dir: string; workspace: string; id: string; kind: "task" | "plan" }) =>
    invokeTauri<ReasoningRefDto | null>("reasoning_ref_get", args),

  stepsList: (args: { storage_dir: string; workspace: string; task_id: string; limit: number }) =>
    invokeTauri<StepListDto[]>("steps_list", args),

  stepsDetail: (args: { storage_dir: string; workspace: string; task_id: string; selector: StepDetailInput }) =>
    invokeTauri<StepDetailDto>("steps_detail", args),

  taskStepsSummary: (args: { storage_dir: string; workspace: string; task_id: string }) =>
    invokeTauri<TaskStepsSummaryDto>("task_steps_summary", args),

  docsEntriesSince: (args: { storage_dir: string; workspace: string; input: DocEntriesSinceInput }) =>
    invokeTauri<DocEntriesSinceDto>("docs_entries_since", args),

  docsShowTail: (args: {
    storage_dir: string;
    workspace: string;
    branch: string;
    doc: string;
    cursor?: number;
    limit: number;
  }) => invokeTauri<DocSliceDto>("docs_show_tail", args),

  branchesList: (args: { storage_dir: string; workspace: string; limit: number }) =>
    invokeTauri<BranchDto[]>("branches_list", args),

  graphQuery: (args: { storage_dir: string; workspace: string; branch: string; doc: string; input: GraphQueryInput }) =>
    invokeTauri<GraphSliceDto>("graph_query", args),

  graphDiff: (args: {
    storage_dir: string;
    workspace: string;
    from_branch: string;
    to_branch: string;
    doc: string;
    cursor?: number;
    limit: number;
  }) => invokeTauri<GraphDiffSliceDto>("graph_diff", args),

  tasksSearch: (args: { storage_dir: string; workspace: string; text: string; limit: number }) =>
    invokeTauri<TasksSearchDto>("tasks_search", args),

  knowledgeSearch: (args: { storage_dir: string; workspace: string; text: string; limit: number }) =>
    invokeTauri<KnowledgeSearchDto>("knowledge_search", args),

  knowledgeCardGet: (args: { storage_dir: string; workspace: string; card_id: string }) =>
    invokeTauri<GraphNodeDto | null>("knowledge_card_get", args),

  anchorsList: (args: {
    storage_dir: string;
    workspace: string;
    text?: string;
    kind?: string;
    status?: string;
    limit: number;
  }) => invokeTauri<AnchorsListDto>("anchors_list", args),
};
