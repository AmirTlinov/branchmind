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
  } = {}) =>
    invokeTauri<ProjectDto[]>("projects_scan", {
      roots: args.roots,
      maxDepth: args.max_depth,
      limit: args.limit,
      timeoutMs: args.timeout_ms,
    }),

  workspacesList: (args: { storage_dir: string; limit: number; offset: number }) =>
    invokeTauri<WorkspaceDto[]>("workspaces_list", {
      storageDir: args.storage_dir,
      limit: args.limit,
      offset: args.offset,
    }),

  focusGet: (args: { storage_dir: string; workspace: string }) =>
    invokeTauri<string | null>("focus_get", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
    }),

  tasksList: (args: { storage_dir: string; workspace: string; limit: number; offset: number }) =>
    invokeTauri<TaskSummaryDto[]>("tasks_list", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      limit: args.limit,
      offset: args.offset,
    }),

  tasksGet: (args: { storage_dir: string; workspace: string; id: string }) =>
    invokeTauri<TaskDto | null>("tasks_get", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      id: args.id,
    }),

  plansGet: (args: { storage_dir: string; workspace: string; id: string }) =>
    invokeTauri<PlanDto | null>("plans_get", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      id: args.id,
    }),

  reasoningRefGet: (args: { storage_dir: string; workspace: string; id: string; kind: "task" | "plan" }) =>
    invokeTauri<ReasoningRefDto | null>("reasoning_ref_get", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      id: args.id,
      kind: args.kind,
    }),

  stepsList: (args: { storage_dir: string; workspace: string; task_id: string; limit: number }) =>
    invokeTauri<StepListDto[]>("steps_list", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      taskId: args.task_id,
      limit: args.limit,
    }),

  stepsDetail: (args: { storage_dir: string; workspace: string; task_id: string; selector: StepDetailInput }) =>
    invokeTauri<StepDetailDto>("steps_detail", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      taskId: args.task_id,
      selector: args.selector,
    }),

  taskStepsSummary: (args: { storage_dir: string; workspace: string; task_id: string }) =>
    invokeTauri<TaskStepsSummaryDto>("task_steps_summary", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      taskId: args.task_id,
    }),

  docsEntriesSince: (args: { storage_dir: string; workspace: string; input: DocEntriesSinceInput }) =>
    invokeTauri<DocEntriesSinceDto>("docs_entries_since", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      input: args.input,
    }),

  docsShowTail: (args: {
    storage_dir: string;
    workspace: string;
    branch: string;
    doc: string;
    cursor?: number;
    limit: number;
  }) =>
    invokeTauri<DocSliceDto>("docs_show_tail", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      branch: args.branch,
      doc: args.doc,
      cursor: args.cursor,
      limit: args.limit,
    }),

  branchesList: (args: { storage_dir: string; workspace: string; limit: number }) =>
    invokeTauri<BranchDto[]>("branches_list", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      limit: args.limit,
    }),

  graphQuery: (args: { storage_dir: string; workspace: string; branch: string; doc: string; input: GraphQueryInput }) =>
    invokeTauri<GraphSliceDto>("graph_query", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      branch: args.branch,
      doc: args.doc,
      input: args.input,
    }),

  graphDiff: (args: {
    storage_dir: string;
    workspace: string;
    from_branch: string;
    to_branch: string;
    doc: string;
    cursor?: number;
    limit: number;
  }) =>
    invokeTauri<GraphDiffSliceDto>("graph_diff", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      fromBranch: args.from_branch,
      toBranch: args.to_branch,
      doc: args.doc,
      cursor: args.cursor,
      limit: args.limit,
    }),

  tasksSearch: (args: { storage_dir: string; workspace: string; text: string; limit: number }) =>
    invokeTauri<TasksSearchDto>("tasks_search", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      text: args.text,
      limit: args.limit,
    }),

  knowledgeSearch: (args: { storage_dir: string; workspace: string; text: string; limit: number }) =>
    invokeTauri<KnowledgeSearchDto>("knowledge_search", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      text: args.text,
      limit: args.limit,
    }),

  knowledgeCardGet: (args: { storage_dir: string; workspace: string; card_id: string }) =>
    invokeTauri<GraphNodeDto | null>("knowledge_card_get", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      cardId: args.card_id,
    }),

  anchorsList: (args: {
    storage_dir: string;
    workspace: string;
    text?: string;
    kind?: string;
    status?: string;
    limit: number;
  }) =>
    invokeTauri<AnchorsListDto>("anchors_list", {
      storageDir: args.storage_dir,
      workspace: args.workspace,
      text: args.text,
      kind: args.kind,
      status: args.status,
      limit: args.limit,
    }),
};
