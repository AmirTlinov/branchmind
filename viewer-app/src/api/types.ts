/* ── API Type Definitions (mirrors Rust viewer JSON contracts) ── */

export interface ProjectInfo {
  label: string;
  project_guard: string;
  storage_dir: string;
  store_present: boolean;
  is_temp: boolean;
  stale: boolean;
  updated_at_ms: number;
  workspace_default: string;
  workspace_recommended: string;
}

export interface ProjectsResponse {
  current_label: string;
  current_project_guard: string;
  current_storage_dir: string;
  generated_at: string;
  generated_at_ms: number;
  projects: ProjectInfo[];
}

export interface WorkspaceInfo {
  workspace: string;
  project_guard: string;
  created_at_ms: number;
}

export interface WorkspacesResponse {
  generated_at: string;
  generated_at_ms: number;
  project_guard: string;
  workspace_default: string;
  workspace_recommended: string;
  workspaces: WorkspaceInfo[];
}

export interface AboutInfo {
  fingerprint: string;
  project_guard: string;
  workspace_default: string;
  workspace_recommended: string;
}

export interface ProjectGuard {
  expected: string | null;
  stored: string | null;
  status: string;
}

export interface RunnerJobs {
  running: number;
  queued: number;
}

export interface RunnerAutostart {
  enabled: boolean;
  dry_run: boolean;
  active: boolean;
  last_attempt_ms: number | null;
  last_attempt_ok: boolean | null;
}

export interface RunnerInfo {
  status: string;
  base_status: string;
  runner_id: string | null;
  active_job_id: string | null;
  lease_expires_at_ms: number | null;
  live_count: number;
  idle_count: number;
  offline_count: number;
  jobs: RunnerJobs;
  autostart: RunnerAutostart;
}

export interface FocusInfo {
  kind: string;
  id: string | null;
  title: string | null;
  plan_id: string | null;
}

export interface TaskCounts {
  total: number;
  active: number;
  backlog: number;
  parked: number;
  done: number;
}

export interface PlanSummary {
  id: string;
  title: string;
  description: string | null;
  context: string | null;
  status: string;
  priority: string | null;
  updated_at_ms: number;
  task_counts: TaskCounts;
}

export interface TaskSummary {
  id: string;
  plan_id: string | null;
  title: string;
  description: string | null;
  context: string | null;
  status: string;
  priority: string | null;
  blocked: boolean;
  updated_at_ms: number;
  parked_until_ts_ms: number | null;
}

export interface ChecklistStep {
  path: string;
  title: string;
  completed: boolean;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface PlanChecklist {
  plan_id: string;
  current: string | null;
  steps: ChecklistStep[];
}

export interface Snapshot {
  lens: string;
  workspace: string;
  workspace_exists: boolean;
  project_guard: ProjectGuard;
  generated_at: string;
  generated_at_ms: number;
  runner: RunnerInfo;
  focus: FocusInfo;
  primary_plan_id: string | null;
  plans_total: number;
  tasks_total: number;
  plans: PlanSummary[];
  plan_checklist: PlanChecklist | null;
  plan_checklists: Record<string, PlanChecklist>;
  tasks: TaskSummary[];
  truncated: { plans: boolean; tasks: boolean };
}

/* ── Detail views ── */

export interface TaskStep {
  path: string;
  title: string;
  completed: boolean;
  created_at_ms: number;
  updated_at_ms: number;
  completed_at_ms: number | null;
  criteria_confirmed: boolean;
  tests_confirmed: boolean;
  security_confirmed: boolean;
  perf_confirmed: boolean;
  docs_confirmed: boolean;
  blocked: boolean;
  block_reason: string | null;
}

export interface DocEntry {
  seq: number;
  content: string;
  ts_ms: number;
}

export interface DocTail {
  branch: string;
  doc: string;
  entries: DocEntry[];
  has_more: boolean;
  cursor: number | null;
}

export interface TaskDetail {
  workspace: string;
  project_guard: ProjectGuard;
  generated_at: string;
  generated_at_ms: number;
  task: TaskSummary;
  steps: { items: TaskStep[]; truncated: boolean };
  trace_tail: DocTail | null;
  notes_tail: DocTail | null;
}

export interface PlanDetail {
  workspace: string;
  project_guard: ProjectGuard;
  generated_at: string;
  generated_at_ms: number;
  plan: PlanSummary;
  trace_tail: DocTail | null;
  notes_tail: DocTail | null;
}

export interface KnowledgeCard {
  id: string;
  type: string;
  title: string | null;
  text: string | null;
  tags: string[];
  status: string;
  meta_json: string | null;
  deleted: boolean;
  last_seq: number;
  last_ts_ms: number;
}

export interface KnowledgeDetail {
  workspace: string;
  project_guard: ProjectGuard;
  generated_at: string;
  generated_at_ms: number;
  card: KnowledgeCard;
  supports: string[];
  blocks: string[];
  truncated: boolean;
}

/* ── Knowledge snapshot ── */

export interface KnowledgeAnchor {
  anchor: string;
  key_count: number;
}

export interface KnowledgeKey {
  anchor: string;
  key: string;
  card_id: string;
  title: string | null;
  tags: string[];
  status: string;
  last_ts_ms: number;
}

export interface KnowledgeSnapshot {
  lens: string;
  workspace: string;
  workspace_exists: boolean;
  project_guard: ProjectGuard;
  generated_at: string;
  generated_at_ms: number;
  runner: RunnerInfo;
  anchors: KnowledgeAnchor[];
  anchors_total: number;
  keys: KnowledgeKey[];
  keys_total: number;
  truncated: { anchors: boolean; keys: boolean };
}

/* ── Search ── */

export interface SearchItem {
  kind: string;
  id: string;
  title: string;
  score: number;
  snippet: string | null;
}

export interface SearchResult {
  items: SearchItem[];
  total: number;
  query: string;
}

/* ── Settings ── */

export interface ViewerSettings {
  runner_autostart_enabled: boolean;
}

/* ── SSE Events ── */

export interface SSEPayload {
  type: string;
  plan?: PlanSummary;
  tasks?: TaskSummary[];
  lens?: string;
}
