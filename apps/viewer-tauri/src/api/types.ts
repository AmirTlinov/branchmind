// DTOs returned by the Tauri backend (Rust).
//
// Convention: we keep `snake_case` everywhere to match Rust serde defaults.

export interface WorkspaceDto {
  workspace: string;
  created_at_ms: number;
  project_guard?: string | null;
}

export interface ProjectDto {
  project_id: string;
  display_name: string;
  storage_dir: string;
  db_path: string;
  repo_root?: string | null;
  workspaces: WorkspaceDto[];
}

export interface TaskSummaryDto {
  id: string;
  parent_plan_id: string;
  title: string;
  status: string;
  priority: string;
  blocked: boolean;
  reasoning_mode: string;
  updated_at_ms: number;
}

export interface TaskDto {
  id: string;
  revision: number;
  parent_plan_id: string;
  title: string;
  description?: string | null;
  context?: string | null;
  status: string;
  status_manual: boolean;
  priority: string;
  blocked: boolean;
  assignee?: string | null;
  domain?: string | null;
  phase?: string | null;
  component?: string | null;
  reasoning_mode: string;
  criteria_confirmed: boolean;
  tests_confirmed: boolean;
  security_confirmed: boolean;
  perf_confirmed: boolean;
  docs_confirmed: boolean;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface PlanDto {
  id: string;
  revision: number;
  title: string;
  description?: string | null;
  context?: string | null;
  status: string;
  status_manual: boolean;
  priority: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface ReasoningRefDto {
  branch: string;
  notes_doc: string;
  graph_doc: string;
  trace_doc: string;
}

export interface StepListDto {
  step_id: string;
  path: string;
  title: string;
  completed: boolean;
  criteria_confirmed: boolean;
  tests_confirmed: boolean;
  security_confirmed: boolean;
  perf_confirmed: boolean;
  docs_confirmed: boolean;
  blocked: boolean;
  block_reason?: string | null;
  updated_at_ms: number;
}

export interface StepDetailInput {
  step_id?: string;
  path?: string;
}

export interface StepDetailDto {
  step_id: string;
  path: string;
  title: string;
  next_action?: string | null;
  stop_criteria?: string | null;
  success_criteria: string[];
  tests: string[];
  blockers: string[];
  criteria_confirmed: boolean;
  tests_confirmed: boolean;
  security_confirmed: boolean;
  perf_confirmed: boolean;
  docs_confirmed: boolean;
  completed: boolean;
  blocked: boolean;
  block_reason?: string | null;
  proof_tests_mode: string;
  proof_security_mode: string;
  proof_perf_mode: string;
  proof_docs_mode: string;
}

export interface TaskStepsSummaryDto {
  total_steps: number;
  completed_steps: number;
  open_steps: number;
  missing_criteria: number;
  missing_tests: number;
  missing_security: number;
  missing_perf: number;
  missing_docs: number;
  first_open?: StepDetailDto | null;
}

export interface DocEntriesSinceInput {
  branch: string;
  doc: string;
  since_seq: number;
  limit: number;
  kind?: string;
}

export interface DocEntryDto {
  seq: number;
  ts_ms: number;
  branch: string;
  doc: string;
  kind: string;
  title?: string | null;
  format?: string | null;
  meta_json?: string | null;
  content?: string | null;
  source_event_id?: string | null;
  event_type?: string | null;
  task_id?: string | null;
  path?: string | null;
  payload_json?: string | null;
}

export interface DocEntriesSinceDto {
  entries: DocEntryDto[];
  total: number;
}

export interface DocSliceDto {
  entries: DocEntryDto[];
  next_cursor?: number | null;
  has_more: boolean;
}

export interface BranchDto {
  name: string;
  base_branch?: string | null;
  base_seq?: number | null;
  created_at_ms?: number | null;
}

export interface GraphNodeDto {
  id: string;
  node_type: string;
  title?: string | null;
  text?: string | null;
  tags: string[];
  status?: string | null;
  meta_json?: string | null;
  deleted: boolean;
  last_seq: number;
  last_ts_ms: number;
}

export interface GraphEdgeDto {
  from: string;
  rel: string;
  to: string;
  meta_json?: string | null;
  deleted: boolean;
  last_seq: number;
  last_ts_ms: number;
}

export interface GraphSliceDto {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  next_cursor?: number | null;
  has_more: boolean;
}

export interface GraphQueryInput {
  ids?: string[];
  types?: string[];
  status?: string;
  tags_any?: string[];
  tags_all?: string[];
  text?: string;
  cursor?: number;
  limit?: number;
  include_edges?: boolean;
  edges_limit?: number;
}

export interface GraphDiffSliceDto {
  changes: Array<
    | { kind: "node"; to: GraphNodeDto }
    | { kind: "edge"; to: GraphEdgeDto }
  >;
  next_cursor?: number | null;
  has_more: boolean;
}

export interface TaskSearchHitDto {
  id: string;
  plan_id: string;
  title: string;
  updated_at_ms: number;
}

export interface TasksSearchDto {
  tasks: TaskSearchHitDto[];
  has_more: boolean;
}

export interface KnowledgeKeyDto {
  anchor_id: string;
  key: string;
  card_id: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface KnowledgeSearchDto {
  items: KnowledgeKeyDto[];
  has_more: boolean;
}

export interface AnchorDto {
  id: string;
  title: string;
  kind: string;
  description?: string | null;
  status?: string | null;
  parent_id?: string | null;
  refs: string[];
  depends_on: string[];
  aliases: string[];
  created_at_ms: number;
  updated_at_ms: number;
}

export interface AnchorsListDto {
  anchors: AnchorDto[];
  has_more: boolean;
}
