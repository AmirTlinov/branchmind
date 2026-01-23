type TaskCounts = {
  total: number;
  active: number;
  backlog: number;
  parked: number;
  done: number;
};

type PlanItem = {
  id: string;
  title: string;
  description?: string | null;
  context?: string | null;
  status: string;
  priority: string;
  updated_at_ms: number;
  task_counts: TaskCounts;
};

type TaskItem = {
  id: string;
  plan_id: string;
  title: string;
  description?: string | null;
  context?: string | null;
  status: string;
  priority: string;
  blocked: boolean;
  updated_at_ms: number;
  parked_until_ts_ms?: number | null;
};

type FocusItem = {
  kind: "plan" | "task" | "none";
  id: string | null;
  title: string | null;
  plan_id: string | null;
};

type PlanChecklist = {
  plan_id: string;
  current: number;
  steps: string[];
};

type RunnerJobs = {
  running: number;
  queued: number;
};

type RunnerAutostart = {
  enabled: boolean;
  dry_run: boolean;
  active: boolean;
  last_attempt_ms?: number | null;
  last_attempt_ok?: boolean | null;
};

type RunnerInfo = {
  status: string;
  base_status: string;
  runner_id?: string | null;
  active_job_id?: string | null;
  lease_expires_at_ms?: number | null;
  live_count: number;
  idle_count: number;
  offline_count: number;
  jobs: RunnerJobs;
  autostart: RunnerAutostart;
};

type Snapshot = {
  workspace: string;
  workspace_exists: boolean;
  project_guard?: {
    expected?: string | null;
    stored?: string | null;
    status?: string | null;
  };
  generated_at: string;
  generated_at_ms: number;
  runner: RunnerInfo;
  focus: FocusItem;
  primary_plan_id: string | null;
  plans: PlanItem[];
  plan_checklist: PlanChecklist | null;
  plan_checklists?: Record<string, PlanChecklist> | null;
  tasks: TaskItem[];
  truncated: { plans: boolean; tasks: boolean };
};

type SnapshotError = {
  error: { code: string; message: string; recovery?: string | null };
};

type AboutPayload = {
  project_guard?: string | null;
  workspace_default?: string | null;
  workspace_recommended?: string | null;
};

type WorkspaceListItem = {
  workspace: string;
  created_at_ms: number;
  project_guard?: string | null;
};

type WorkspacesPayload = {
  project_guard?: string | null;
  workspace_default?: string | null;
  workspace_recommended?: string | null;
  workspaces: WorkspaceListItem[];
};

type ProjectListItem = {
  project_guard: string;
  label: string;
  storage_dir?: string | null;
  workspace_default?: string | null;
  workspace_recommended?: string | null;
  updated_at_ms: number;
  stale: boolean;
  store_present?: boolean;
  is_temp?: boolean;
};

type ProjectsPayload = {
  generated_at: string;
  generated_at_ms: number;
  current_project_guard?: string | null;
  current_label?: string | null;
  current_storage_dir?: string | null;
  projects: ProjectListItem[];
};

type DocTailEntry = {
  seq: number;
  ts_ms: number;
  kind: "note" | "event";
  title?: string | null;
  format?: string | null;
  content?: string | null;
  event_type?: string | null;
  task_id?: string | null;
  path?: string | null;
  source?: "trace" | "notes";
};

type DocTail = {
  branch: string;
  doc: string;
  entries: DocTailEntry[];
  has_more: boolean;
  next_cursor?: number | null;
};

type TaskStep = {
  path: string;
  title: string;
  completed: boolean;
  created_at_ms: number;
  updated_at_ms: number;
  completed_at_ms?: number | null;
  criteria_confirmed: boolean;
  tests_confirmed: boolean;
  security_confirmed: boolean;
  perf_confirmed: boolean;
  docs_confirmed: boolean;
  blocked: boolean;
  block_reason?: string | null;
};

type TaskDetailPayload = {
  workspace: string;
  generated_at: string;
  generated_at_ms: number;
  task: TaskItem;
  steps: { items: TaskStep[]; truncated: boolean };
  trace_tail: DocTail;
  notes_tail: DocTail;
};

type PlanDetailPayload = {
  workspace: string;
  generated_at: string;
  generated_at_ms: number;
  plan: PlanItem;
  trace_tail: DocTail;
  notes_tail: DocTail;
};

type ActivityState = {
  entries: DocTailEntry[];
  seen: Set<string>;
  trace_cursor: number | null;
  notes_cursor: number | null;
  trace_has_more: boolean;
  notes_has_more: boolean;
  loading_more: boolean;
};

type DetailSelection = { kind: "plan" | "task"; id: string; token: number; activity: ActivityState };

type GraphNode = {
  id: string;
  kind: "plan" | "task";
  plan_id?: string;
  status?: string;
  label: string;
  x: number;
  y: number;
  tx?: number;
  ty?: number;
  vx: number;
  vy: number;
  radius: number;
};

type GraphEdge = {
  from: string;
  to: string;
  type?: "hierarchy" | "similar";
  weight?: number;
};

type GraphModel = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  width: number;
  height: number;
};

type GraphView = {
  offsetX: number;
  offsetY: number;
  scale: number;
  dragging: boolean;
  draggingNodeId: string | null;
  dragNodeOffsetX: number;
  dragNodeOffsetY: number;
  lastX: number;
  lastY: number;
  moved: boolean;
};

const state: {
  snapshot: Snapshot | null;
  selectedPlanId: string | null;
  detailToken: number;
  detailSelection: DetailSelection | null;
  projectOverride: string | null;
  currentProjectGuard: string | null;
  currentProjectLabel: string | null;
  includeTempProjects: boolean;
  projects: ProjectListItem[];
  workspaceOverride: string | null;
  workspaceRecommended: string | null;
  workspaces: WorkspaceListItem[];
  workspacesDefault: string | null;
  workspacesExpectedGuard: string | null;
} = {
  snapshot: null,
  selectedPlanId: null,
  detailToken: 0,
  detailSelection: null,
  projectOverride: null,
  currentProjectGuard: null,
  currentProjectLabel: null,
  includeTempProjects: queryFlag("temp") || queryFlag("show_temp"),
  projects: [],
  workspaceOverride: null,
  workspaceRecommended: null,
  workspaces: [],
  workspacesDefault: null,
  workspacesExpectedGuard: null,
};

const graphState: {
  model: GraphModel | null;
  view: GraphView | null;
  hoverId: string | null;
  snapshotKey: number;
  handlersReady: boolean;
  pixelRatio: number;
} = {
  model: null,
  view: null,
  hoverId: null,
  snapshotKey: 0,
  handlersReady: false,
  pixelRatio: 1,
};

const autostartMutation: { pending: boolean } = { pending: false };
const workspaceMutation: { recoveredGuardMismatch: boolean } = { recoveredGuardMismatch: false };
const projectsMutation: { pending: boolean } = { pending: false };

const PROJECT_STORAGE_KEY = "bm_viewer_project";
const WORKSPACE_STORAGE_KEY = "bm_viewer_workspace";
const WORKSPACE_STORAGE_PREFIX = "bm_viewer_workspace:";

function queryFlag(name: string): boolean {
  try {
    const params = new URLSearchParams(window.location.search || "");
    const raw = (params.get(name) || "").trim().toLowerCase();
    return raw === "1" || raw === "true" || raw === "yes" || raw === "on";
  } catch {
    return false;
  }
}

function workspaceUrl(path: string): string {
  const parts: string[] = [];
  const project = (state.projectOverride || "").trim();
  if (project) {
    parts.push(`project=${encodeURIComponent(project)}`);
  }
  const workspace = (state.workspaceOverride || "").trim();
  if (workspace) {
    parts.push(`workspace=${encodeURIComponent(workspace)}`);
  }
  if (parts.length === 0) return path;
  const join = path.includes("?") ? "&" : "?";
  return `${path}${join}${parts.join("&")}`;
}

function workspaceUrlWithParams(path: string, params: Record<string, unknown>): string {
  const base = workspaceUrl(path);
  const parts: string[] = [];
  Object.entries(params || {}).forEach(([key, value]) => {
    if (value === null || value === undefined) return;
    parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`);
  });
  if (parts.length === 0) return base;
  const join = base.includes("?") ? "&" : "?";
  return `${base}${join}${parts.join("&")}`;
}

function activeProjectKey(): string {
  const key = (state.projectOverride || state.currentProjectGuard || "current").trim();
  return key || "current";
}

function workspaceStorageKey(): string {
  return `${WORKSPACE_STORAGE_PREFIX}${activeProjectKey()}`;
}

function setProjectOverride(value: string | null) {
  const next = (value || "").trim();
  state.projectOverride = next || null;
  try {
    if (state.projectOverride) {
      localStorage.setItem(PROJECT_STORAGE_KEY, state.projectOverride);
    } else {
      localStorage.removeItem(PROJECT_STORAGE_KEY);
    }
  } catch {
    // ignore storage failures (private mode, etc.)
  }
}

function setWorkspaceOverride(value: string | null) {
  const next = (value || "").trim();
  state.workspaceOverride = next || null;
  try {
    if (state.workspaceOverride) {
      localStorage.setItem(workspaceStorageKey(), state.workspaceOverride);
    } else {
      localStorage.removeItem(workspaceStorageKey());
    }
  } catch {
    // ignore storage failures (private mode, etc.)
  }
}

function loadWorkspaceOverrideFromStorage() {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(workspaceStorageKey());
    if (!stored) {
      // Backward-compat: migrate old single-project key into the current project slot.
      stored = localStorage.getItem(WORKSPACE_STORAGE_KEY);
      if (stored) {
        localStorage.setItem(workspaceStorageKey(), stored);
        localStorage.removeItem(WORKSPACE_STORAGE_KEY);
      }
    }
  } catch {
    stored = null;
  }
  state.workspaceOverride = stored ? stored.trim() || null : null;
}

function currentProjectLabel(): string {
  const override = state.projectOverride;
  if (override) {
    const match = state.projects.find((p) => p.project_guard === override);
    return match?.label || "external";
  }
  return state.currentProjectLabel || "current";
}

async function loadProjects() {
  try {
    const payload = await fetchJson<ProjectsPayload>("/api/projects");
    const currentGuard = (payload.current_project_guard || "").trim() || null;
    const currentLabel = (payload.current_label || "").trim() || null;
    state.currentProjectGuard = currentGuard;
    state.currentProjectLabel = currentLabel;
    state.projects = Array.isArray(payload.projects) ? payload.projects : [];

    if (!state.includeTempProjects) {
      const others = state.projects
        .filter((project) => project?.project_guard)
        .filter((project) => project.store_present !== false)
        .filter((project) => (currentGuard ? project.project_guard !== currentGuard : true));
      const hasNonTemp = others.some((project) => !project.is_temp);
      const hasTemp = others.some((project) => !!project.is_temp);
      // If hiding temp projects would leave us with nothing to switch to, show them by default.
      if (!hasNonTemp && hasTemp) {
        state.includeTempProjects = true;
      }
    }

    let stored: string | null = null;
    try {
      stored = localStorage.getItem(PROJECT_STORAGE_KEY);
    } catch {
      stored = null;
    }

    const candidate = (stored || "").trim() || null;
    const match = candidate ? state.projects.find((project) => project.project_guard === candidate) : undefined;
    const isKnown = !!(
      match &&
      match.store_present !== false &&
      (state.includeTempProjects || !match.is_temp) &&
      // Do not auto-restore stale external stores; users typically want the current shared store.
      !match.stale
    );
    if (!candidate || !isKnown || (currentGuard && candidate === currentGuard)) {
      setProjectOverride(null);
    } else {
      setProjectOverride(candidate);
    }
  } catch {
    state.currentProjectGuard = null;
    state.currentProjectLabel = null;
    state.projects = [];
    setProjectOverride(null);
  } finally {
    loadWorkspaceOverrideFromStorage();
  }
}

async function loadAbout() {
  try {
    const payload = await fetchJson<AboutPayload>(workspaceUrl("/api/about"));
    const recommended = (payload.workspace_recommended || "").trim();
    state.workspaceRecommended = recommended ? recommended : null;
  } catch {
    state.workspaceRecommended = null;
  }
}

async function loadWorkspaces() {
  try {
    const payload = await fetchJson<WorkspacesPayload>(workspaceUrl("/api/workspaces"));
    state.workspaces = Array.isArray(payload.workspaces) ? payload.workspaces : [];
    state.workspacesDefault = (payload.workspace_default || "").trim() || null;
    state.workspacesExpectedGuard = (payload.project_guard || "").trim() || null;

    if (!state.workspaceOverride) {
      const candidates = state.workspaces
        .map((entry) => (entry?.workspace || "").trim())
        .filter((value) => value);
      const recommended = (payload.workspace_recommended || state.workspaceRecommended || "").trim();
      const desired =
        (recommended && candidates.includes(recommended) && recommended) ||
        (state.workspacesDefault && candidates.includes(state.workspacesDefault) && state.workspacesDefault) ||
        candidates[0] ||
        null;
      if (desired) {
        setWorkspaceOverride(desired);
      }
    }
  } catch {
    state.workspaces = [];
    state.workspacesDefault = null;
    state.workspacesExpectedGuard = null;
  }
}

const nodes = {
  project: document.getElementById("project") as HTMLSelectElement,
  workspace: document.getElementById("workspace") as HTMLSelectElement,
  updated: document.getElementById("updated") as HTMLElement,
  runnerStatus: document.getElementById("runner-status") as HTMLElement,
  runnerJobs: document.getElementById("runner-jobs") as HTMLElement,
  runnerAutostart: document.getElementById("runner-autostart") as HTMLInputElement,
  focus: document.getElementById("focus") as HTMLElement,
  focusSub: document.getElementById("focus-sub") as HTMLElement,
  planCount: document.getElementById("plan-count") as HTMLElement,
  planBreakdown: document.getElementById("plan-breakdown") as HTMLElement,
  taskCount: document.getElementById("task-count") as HTMLElement,
  taskBreakdown: document.getElementById("task-breakdown") as HTMLElement,
  goalList: document.getElementById("goal-list") as HTMLElement,
  planChecklist: document.getElementById("plan-checklist") as HTMLElement,
  taskList: document.getElementById("task-list") as HTMLElement,
  graph: document.getElementById("graph") as HTMLCanvasElement,
  detailPanel: document.getElementById("detail-panel") as HTMLElement,
  detailKicker: document.getElementById("detail-kicker") as HTMLElement,
  detailTitle: document.getElementById("detail-title") as HTMLElement,
  detailMeta: document.getElementById("detail-meta") as HTMLElement,
  detailBody: document.getElementById("detail-body") as HTMLElement,
  detailClose: document.getElementById("detail-close") as HTMLButtonElement,
};

function renderProjectSelect() {
  const select = nodes.project;
  if (!select) return;
  clear(select);

  const currentLabel = state.currentProjectLabel || "current";
  const current = document.createElement("option");
  current.value = "";
  current.textContent = `${currentLabel} — current`;
  select.append(current);

  const currentGuard = state.currentProjectGuard;
  const includeTemp = state.includeTempProjects;
  const others = (state.projects || [])
    .filter((project) => project?.project_guard)
    .filter((project) => project.store_present !== false)
    .filter((project) => includeTemp || !project.is_temp)
    .filter((project) => (currentGuard ? project.project_guard !== currentGuard : true))
    .sort((a, b) => {
      const aStale = !!a.stale;
      const bStale = !!b.stale;
      if (aStale !== bStale) return aStale ? 1 : -1;
      const aTemp = !!a.is_temp;
      const bTemp = !!b.is_temp;
      if (aTemp !== bTemp) return aTemp ? 1 : -1;
      return (a.label || "").localeCompare(b.label || "");
    });

  const labelCounts = new Map<string, number>();
  others.forEach((project) => {
    const base = ((project.label || project.project_guard || "") + "").trim() || "project";
    labelCounts.set(base, (labelCounts.get(base) || 0) + 1);
  });

  others.forEach((project) => {
    const opt = document.createElement("option");
    opt.value = project.project_guard;
    opt.title = project.storage_dir || project.project_guard;
    const stale = project.stale ? " — stale" : "";
    const temp = includeTemp && project.is_temp ? " — temp" : "";
    const base = ((project.label || project.project_guard || "") + "").trim() || "project";
    const needsDisambiguation = (labelCounts.get(base) || 0) > 1;
    const guard = ((project.project_guard || "") + "").trim();
    const shortGuard = guard.startsWith("repo:") ? guard.slice(4, 12) : guard.slice(0, 8);
    const disambiguator = needsDisambiguation && shortGuard ? ` — ${shortGuard}` : "";
    opt.textContent = `${base}${disambiguator}${stale}${temp}`;
    select.append(opt);
  });

  select.value = state.projectOverride || "";
  select.disabled = false;
  const title = state.projectOverride || state.currentProjectGuard || "";
  if (title) select.title = title;
}

function renderWorkspaceSelect(selectedWorkspace: string | null) {
  const select = nodes.workspace;
  if (!select) return;
  clear(select);

  const selected = (selectedWorkspace || "").trim();
  const expected = (state.workspacesExpectedGuard || "").trim();

  const autoLabel = selected || state.workspacesDefault || state.workspaceRecommended || "auto";
  const auto = document.createElement("option");
  auto.value = "";
  auto.textContent = `${autoLabel} — auto`;
  select.append(auto);

  const pinnedOrder: string[] = [];
  const pinnedRank = new Map<string, number>();
  const pushPinned = (value: string | null | undefined) => {
    const key = (value || "").trim();
    if (!key) return;
    if (pinnedRank.has(key)) return;
    pinnedRank.set(key, pinnedOrder.length);
    pinnedOrder.push(key);
  };

  // Keep current/selected first so it stays easy to find.
  pushPinned(selected);
  pushPinned(state.workspacesDefault);
  pushPinned(state.workspaceRecommended);
  // Seed with repo-recommended workspace names so "real projects" float to the top.
  (state.projects || []).forEach((project) => {
    pushPinned((project.workspace_recommended || "").trim());
    pushPinned((project.workspace_default || "").trim());
  });

  const entries = (state.workspaces || [])
    .filter((entry) => entry?.workspace)
    .slice()
    .sort((a, b) => {
      const aw = (a.workspace || "").trim();
      const bw = (b.workspace || "").trim();
      const ar = pinnedRank.get(aw);
      const br = pinnedRank.get(bw);
      if (ar !== undefined || br !== undefined) {
        if (ar === undefined) return 1;
        if (br === undefined) return -1;
        if (ar !== br) return ar - br;
      }
      return aw.localeCompare(bw, undefined, { sensitivity: "base" });
    });

  entries.forEach((entry) => {
      const workspace = (entry.workspace || "").trim();
      if (!workspace) return;
      const opt = document.createElement("option");
      opt.value = workspace;
      const guard = (entry.project_guard || "").trim();
      let suffix = "";
      if (!guard) {
        suffix = " — uninitialized";
      } else if (expected && guard !== expected) {
        suffix = " — guard mismatch";
      }
      opt.textContent = `${workspace}${suffix}`;
      select.append(opt);
    });

  select.value = state.workspaceOverride || "";
  const title = state.workspaceOverride || selected || "";
  if (title) select.title = title;
  select.disabled = false;
}

function clear(element: HTMLElement) {
  while (element.firstChild) element.removeChild(element.firstChild);
}

function formatStatus(status: string) {
  return status.replace(/_/g, " ").toLowerCase();
}

function formatCount(label: string, value: number) {
  return `${label} ${value}`;
}

function formatRunnerStatus(status: string) {
  const normalized = (status || "").toLowerCase();
  switch (normalized) {
    case "starting":
      return "starting…";
    case "offline":
      return "offline";
    case "idle":
      return "idle";
    case "live":
      return "live";
    default:
      return normalized || "-";
  }
}

function formatDate(valueMs: number) {
  if (!valueMs) return "-";
  const date = new Date(valueMs);
  return date.toISOString().replace("T", " ").replace("Z", " UTC");
}

function formatApiError(error: unknown) {
  if (!error) return "Unable to load details.";
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Unable to load details.";
}

function startDetailLoad(kind: DetailSelection["kind"], id: string) {
  state.detailToken += 1;
  state.detailSelection = {
    kind,
    id,
    token: state.detailToken,
    activity: {
      entries: [],
      seen: new Set(),
      trace_cursor: null,
      notes_cursor: null,
      trace_has_more: false,
      notes_has_more: false,
      loading_more: false,
    },
  };
  return state.detailToken;
}

function isCurrentDetail(token: number) {
  return state.detailSelection?.token === token;
}

async function fetchJson<T>(path: string): Promise<T> {
  const response = await fetchWithTimeout(path, { cache: "no-store" }, 7_000);
  const text = await response.text();
  let payload: unknown = null;
  try {
    payload = JSON.parse(text);
  } catch (_err) {
    throw new Error(`Invalid JSON response for ${path}`);
  }
  if (!response.ok) {
    const message =
      (payload as SnapshotError)?.error?.message || `Request failed (${response.status})`;
    const recovery = (payload as SnapshotError)?.error?.recovery
      ? ` ${(payload as SnapshotError).error.recovery}`
      : "";
    throw new Error(`${message}${recovery}`.trim());
  }
  return payload as T;
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetchWithTimeout(
    path,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      cache: "no-store",
    },
    7_000
  );
  const text = await response.text();
  let payload: unknown = null;
  try {
    payload = JSON.parse(text);
  } catch (_err) {
    throw new Error(`Invalid JSON response for ${path}`);
  }
  if (!response.ok) {
    const message =
      (payload as SnapshotError)?.error?.message || `Request failed (${response.status})`;
    const recovery = (payload as SnapshotError)?.error?.recovery
      ? ` ${(payload as SnapshotError).error.recovery}`
      : "";
    throw new Error(`${message}${recovery}`.trim());
  }
  return payload as T;
}

async function fetchWithTimeout(
  path: string,
  options: RequestInit,
  timeoutMs: number
): Promise<Response> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(path, { ...options, signal: controller.signal });
  } finally {
    window.clearTimeout(timeout);
  }
}

function setDetailVisible(open: boolean) {
  if (!nodes.detailPanel) return;
  nodes.detailPanel.classList.toggle("is-open", open);
  nodes.detailPanel.setAttribute("aria-hidden", open ? "false" : "true");
}

function renderDetailMeta(lines: string[]) {
  clear(nodes.detailMeta);
  lines.forEach((line) => {
    const row = document.createElement("div");
    row.textContent = line;
    nodes.detailMeta.append(row);
  });
}

function renderDetailSection(title: string, content: HTMLElement[]) {
  const section = document.createElement("div");
  const header = document.createElement("div");
  header.className = "detail-section-title";
  header.textContent = title;
  section.append(header, ...content);
  return section;
}

function renderDetailText(value: string | null | undefined, emptyMessage: string) {
  const block = document.createElement("div");
  block.className = "detail-text";
  const text = (value ?? "").trim();
  if (!text) {
    block.classList.add("muted");
    block.textContent = emptyMessage;
    return block;
  }
  block.textContent = text;
  return block;
}

function chooseDerivedPlanText(snapshot: Snapshot, plan: PlanItem) {
  const planText = (plan.description ?? "").trim();
  if (planText) {
    return { text: planText, fromTask: null as TaskItem | null };
  }
  const checklist =
    snapshot.plan_checklists?.[plan.id] ||
    (snapshot.plan_checklist?.plan_id === plan.id ? snapshot.plan_checklist : null);
  if (checklist && Array.isArray(checklist.steps) && checklist.steps.length > 0) {
    const max = Math.min(24, checklist.steps.length);
    const lines = checklist.steps.slice(0, max).map((step, index) => `${index + 1}. ${step}`);
    const suffix = checklist.steps.length > max ? `\n… +${checklist.steps.length - max} more` : "";
    return { text: `${lines.join("\n")}${suffix}`, fromTask: null as TaskItem | null };
  }
  const directTaskId = plan.id.startsWith("PLAN-") ? plan.id.replace("PLAN-", "TASK-") : null;
  if (directTaskId) {
    const direct = snapshot.tasks.find(
      (task) => task.id === directTaskId && (task.description ?? "").trim().length > 0
    );
    if (direct) {
      return { text: (direct.description ?? "").trim(), fromTask: direct };
    }
  }
  const candidates = snapshot.tasks
    .filter((task) => task.plan_id === plan.id)
    .filter((task) => (task.description ?? "").trim().length > 0);
  if (candidates.length === 0) {
    return { text: "", fromTask: null as TaskItem | null };
  }
  const statusRank = (status: string) => {
    switch (status) {
      case "ACTIVE":
        return 0;
      case "TODO":
        return 1;
      case "PARKED":
        return 2;
      case "DONE":
        return 3;
      default:
        return 4;
    }
  };
  candidates.sort((a, b) => {
    const diff = statusRank(a.status) - statusRank(b.status);
    if (diff !== 0) return diff;
    return a.id.localeCompare(b.id);
  });
  const fromTask = candidates[0];
  return { text: (fromTask.description ?? "").trim(), fromTask };
}

function appendDocTail(activity: ActivityState, tail: DocTail | null | undefined, source: "trace" | "notes") {
  if (!tail?.entries?.length) return;
  tail.entries.forEach((entry) => {
    const seq = entry?.seq ?? null;
    if (seq === null || seq === undefined) return;
    const key = `${source}:${seq}`;
    if (activity.seen.has(key)) return;
    activity.seen.add(key);
    activity.entries.push({ ...entry, source });
  });
}

function updateActivityState(activity: ActivityState, payload: { trace_tail?: DocTail; notes_tail?: DocTail }) {
  const trace = payload.trace_tail;
  const notes = payload.notes_tail;
  appendDocTail(activity, trace, "trace");
  appendDocTail(activity, notes, "notes");
  activity.trace_has_more = !!trace?.has_more;
  activity.notes_has_more = !!notes?.has_more;
  activity.trace_cursor = activity.trace_has_more ? (trace?.next_cursor ?? null) : 0;
  activity.notes_cursor = activity.notes_has_more ? (notes?.next_cursor ?? null) : 0;
  activity.entries.sort((a, b) => {
    const ts = (b.ts_ms || 0) - (a.ts_ms || 0);
    if (ts !== 0) return ts;
    return (b.seq || 0) - (a.seq || 0);
  });
}

function renderLoadMoreButton(label: string, onClick: () => void, disabled: boolean) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "list-item";
  btn.style.transform = "none";
  btn.textContent = label;
  btn.disabled = !!disabled;
  btn.addEventListener("click", onClick);
  return btn;
}

function renderDocEntry(entry: DocTailEntry) {
  const row = document.createElement("div");
  row.className = "list-item detail-entry";

  const header = document.createElement("div");
  header.className = "detail-entry-head";

  const title = document.createElement("div");
  title.className = "detail-entry-title";
  title.textContent =
    (entry.title ?? "").trim() ||
    (entry.event_type ?? "").trim() ||
    (entry.kind === "event" ? "Event" : "Note");

  const meta = document.createElement("div");
  meta.className = "detail-entry-meta";
  const source = document.createElement("span");
  source.className = "badge dim";
  source.textContent = entry.source === "notes" ? "notes" : "trace";
  const kind = document.createElement("span");
  kind.className = "badge accent";
  kind.textContent = entry.kind === "event" ? "event" : "note";
  const ts = document.createElement("span");
  ts.textContent = formatDate(entry.ts_ms);
  meta.append(source, kind, ts);

  header.append(title, meta);

  const body = document.createElement("div");
  body.className = "detail-entry-body";
  body.textContent = (entry.content ?? "").trim() || "No content.";

  row.append(header, body);
  return row;
}

function renderDocEntries(entries: DocTailEntry[]) {
  if (!entries || entries.length === 0) {
    return [renderDetailText(null, "No recent notes yet.")];
  }
  return entries.map(renderDocEntry);
}

function renderStepEntry(step: TaskStep) {
  const row = document.createElement("div");
  row.className = "list-item step-item";
  row.style.transform = "none";

  const depth = Math.max(0, (step.path ?? "").split(".").length - 1);
  const content = document.createElement("div");
  content.className = "step-content";
  content.style.paddingLeft = `${depth * 14}px`;

  const title = document.createElement("div");
  title.className = "item-title";
  title.textContent = step.title || step.path || "Step";

  const meta = document.createElement("div");
  meta.className = "item-meta";

  const path = document.createElement("span");
  path.className = "badge dim";
  path.textContent = step.path;

  const status = document.createElement("span");
  status.className = `badge ${step.completed ? "accent" : "dim"}`;
  status.textContent = step.completed ? "done" : "todo";

  const gate = document.createElement("span");
  gate.className = "step-gate";
  const bits = [
    step.criteria_confirmed ? "C" : "c",
    step.tests_confirmed ? "T" : "t",
    step.security_confirmed ? "S" : "s",
    step.perf_confirmed ? "P" : "p",
    step.docs_confirmed ? "D" : "d",
  ];
  gate.textContent = `gate ${bits.join("")}`;

  meta.append(path, status, gate);

  if (step.blocked) {
    const blocked = document.createElement("span");
    blocked.className = "badge";
    blocked.textContent = "blocked";
    meta.append(blocked);
  }

  content.append(title, meta);

  if ((step.block_reason ?? "").trim()) {
    const snippet = document.createElement("div");
    snippet.className = "detail-snippet";
    snippet.textContent = (step.block_reason ?? "").trim();
    content.append(snippet);
  }

  row.append(content);
  return row;
}

function renderStepsBlock(steps: TaskDetailPayload["steps"] | null | undefined) {
  const items = steps?.items ?? [];
  if (!Array.isArray(items) || items.length === 0) {
    return [renderDetailText(null, "No steps yet.")];
  }
  const nodes = items.map(renderStepEntry);
  if (steps?.truncated) {
    const hint = document.createElement("div");
    hint.className = "detail-caption";
    hint.textContent = "Steps list truncated.";
    nodes.push(hint);
  }
  return nodes;
}

function renderPlanDetail(snapshot: Snapshot, plan: PlanItem) {
  const token = startDetailLoad("plan", plan.id);
  nodes.detailKicker.textContent = "Plan";
  nodes.detailTitle.textContent = plan.title || plan.id;
  renderDetailMeta([
    `Status: ${formatStatus(plan.status)}`,
    `Priority: ${formatStatus(plan.priority)}`,
    `Updated: ${formatDate(plan.updated_at_ms)}`,
    `Tasks: ${plan.task_counts.done}/${plan.task_counts.total} done`,
  ]);
  clear(nodes.detailBody);

  const sections: HTMLElement[] = [];
  const derived = chooseDerivedPlanText(snapshot, plan);
  const planTextBlock = renderDetailText(
    derived.text || null,
    "No plan text yet."
  );
  const planExtras: HTMLElement[] = [planTextBlock];
  if (derived.fromTask) {
    const caption = document.createElement("div");
    caption.className = "detail-caption";
    caption.textContent = "Derived from ";
    const link = document.createElement("button");
    link.type = "button";
    link.className = "detail-link";
    link.textContent = derived.fromTask.id;
    link.addEventListener("click", () => renderTaskDetail(snapshot, derived.fromTask!));
    caption.append(link);
    planExtras.push(caption);
  }
  sections.push(renderDetailSection("Plan", planExtras));
  if ((plan.context ?? "").trim()) {
    sections.push(renderDetailSection("Context", [renderDetailText(plan.context, "No context.")])); 
  }
  const checklist =
    snapshot.plan_checklists?.[plan.id] ||
    (snapshot.plan_checklist?.plan_id === plan.id ? snapshot.plan_checklist : null);
  if (checklist) {
    const steps = checklist.steps ?? [];
    const items = steps.length
      ? steps.map((step, index) => {
          const row = document.createElement("div");
          row.className = "list-item";
          const title = document.createElement("div");
          title.className = "item-title";
          title.textContent = `${index + 1}. ${step}`;
          row.append(title);
          if (index + 1 === checklist.current) {
            row.classList.add("active");
          }
          return row;
        })
      : [
          (() => {
            const row = document.createElement("div");
            row.className = "detail-text muted";
            row.textContent = "No checklist steps yet.";
            return row;
          })(),
        ];
    sections.push(renderDetailSection("Checklist", items));
  }

  const relatedTasks = snapshot.tasks.filter((task) => task.plan_id === plan.id).slice(0, 8);
  if (relatedTasks.length) {
    const items = relatedTasks.map((task) => {
      const row = document.createElement("div");
      row.className = "list-item";
      const title = document.createElement("div");
      title.className = "item-title";
      title.textContent = task.title || task.id;
      const snippetText = (task.description ?? "").trim();
      const snippet = document.createElement("div");
      snippet.className = "detail-snippet";
      snippet.textContent = snippetText
        ? snippetText.length > 220
          ? `${snippetText.slice(0, 220)}…`
          : snippetText
        : "No task description.";
      const meta = document.createElement("div");
      meta.className = "item-meta";
      const status = document.createElement("span");
      status.className = "badge accent";
      status.textContent = formatStatus(task.status);
      meta.append(status);
      row.append(title, snippet, meta);
      row.addEventListener("click", () => renderTaskDetail(snapshot, task));
      return row;
    });
    sections.push(renderDetailSection("Tasks", items));
  }

  const activityHost = document.createElement("div");
  activityHost.className = "detail-list";
  activityHost.append(renderDetailText(null, "Loading recent notes…"));
  sections.push(renderDetailSection("Recent", [activityHost]));

  if (!sections.length) {
    const empty = document.createElement("div");
    empty.className = "list-item";
    empty.textContent = "No extra details for this plan yet.";
    nodes.detailBody.append(empty);
  } else {
    sections.forEach((section) => nodes.detailBody.append(section));
  }
  setDetailVisible(true);
  loadPlanExtras(plan.id, token, activityHost);
}

function renderTaskDetail(snapshot: Snapshot, task: TaskItem) {
  const token = startDetailLoad("task", task.id);
  nodes.detailKicker.textContent = "Task";
  nodes.detailTitle.textContent = task.title || task.id;
  const plan = snapshot.plans.find((p) => p.id === task.plan_id);
  renderDetailMeta([
    `Status: ${formatStatus(task.status)}`,
    `Priority: ${formatStatus(task.priority)}`,
    `Blocked: ${task.blocked ? "yes" : "no"}`,
    `Plan: ${plan?.title || task.plan_id}`,
    `Updated: ${formatDate(task.updated_at_ms)}`,
  ]);
  clear(nodes.detailBody);

  nodes.detailBody.append(
    renderDetailSection("Task", [
      renderDetailText(task.description, "No task description yet."),
    ])
  );
  if ((task.context ?? "").trim()) {
    nodes.detailBody.append(
      renderDetailSection("Context", [
        renderDetailText(task.context, "No context."),
      ])
    );
  }

  const stepsHost = document.createElement("div");
  stepsHost.className = "detail-list";
  stepsHost.append(renderDetailText(null, "Loading steps…"));
  nodes.detailBody.append(renderDetailSection("Steps", [stepsHost]));

  const activityHost = document.createElement("div");
  activityHost.className = "detail-list";
  activityHost.append(renderDetailText(null, "Loading recent notes…"));
  nodes.detailBody.append(renderDetailSection("Recent", [activityHost]));

  const siblings = snapshot.tasks
    .filter((entry) => entry.plan_id === task.plan_id && entry.id !== task.id)
    .slice(0, 6);
  if (siblings.length) {
    const items = siblings.map((sibling) => {
      const row = document.createElement("div");
      row.className = "list-item";
      const title = document.createElement("div");
      title.className = "item-title";
      title.textContent = sibling.title || sibling.id;
      const meta = document.createElement("div");
      meta.className = "item-meta";
      const status = document.createElement("span");
      status.className = "badge accent";
      status.textContent = formatStatus(sibling.status);
      meta.append(status);
      row.append(title, meta);
      row.addEventListener("click", () => renderTaskDetail(snapshot, sibling));
      return row;
    });
    nodes.detailBody.append(renderDetailSection("Related tasks", items));
  } else {
    const empty = document.createElement("div");
    empty.className = "list-item";
    empty.textContent = "No related tasks to show.";
    nodes.detailBody.append(empty);
  }
  setDetailVisible(true);
  loadTaskExtras(task.id, token, stepsHost, activityHost);
}

async function loadPlanExtras(planId: string, token: number, activityHost: HTMLElement) {
  try {
    const payload = await fetchJson<PlanDetailPayload>(workspaceUrl(`/api/plan/${planId}`));
    if (!isCurrentDetail(token)) return;
    const selection = state.detailSelection;
    if (!selection?.activity) return;
    selection.activity.entries = [];
    selection.activity.seen = new Set();
    updateActivityState(selection.activity, payload);
    renderActivityHost(activityHost, planId, token, "plan");
  } catch (err) {
    if (!isCurrentDetail(token)) return;
    clear(activityHost);
    activityHost.append(renderDetailText(null, formatApiError(err)));
  }
}

async function loadTaskExtras(
  taskId: string,
  token: number,
  stepsHost: HTMLElement,
  activityHost: HTMLElement
) {
  try {
    const payload = await fetchJson<TaskDetailPayload>(workspaceUrl(`/api/task/${taskId}`));
    if (!isCurrentDetail(token)) return;

    clear(stepsHost);
    renderStepsBlock(payload.steps).forEach((node) => stepsHost.append(node));

    const selection = state.detailSelection;
    if (!selection?.activity) return;
    selection.activity.entries = [];
    selection.activity.seen = new Set();
    updateActivityState(selection.activity, payload);
    renderActivityHost(activityHost, taskId, token, "task");
  } catch (err) {
    if (!isCurrentDetail(token)) return;
    clear(stepsHost);
    stepsHost.append(renderDetailText(null, "Unable to load steps."));
    clear(activityHost);
    activityHost.append(renderDetailText(null, formatApiError(err)));
  }
}

function renderActivityHost(activityHost: HTMLElement, entityId: string, token: number, kind: DetailSelection["kind"]) {
  const selection = state.detailSelection;
  const activity = selection?.activity;
  if (!activity) return;
  clear(activityHost);
  renderDocEntries(activity.entries).forEach((node) => activityHost.append(node));

  const hasMore = activity.trace_has_more || activity.notes_has_more;
  if (!hasMore) return;

  const label = activity.loading_more ? "Loading…" : "Load older notes";
  const btn = renderLoadMoreButton(
    label,
    async () => {
      if (!isCurrentDetail(token)) return;
      const current = state.detailSelection;
      const activityNow = current?.activity;
      if (!activityNow || activityNow.loading_more) return;
      if (!activityNow.trace_has_more && !activityNow.notes_has_more) return;

      activityNow.loading_more = true;
      renderActivityHost(activityHost, entityId, token, kind);

      const params = {
        trace_cursor: activityNow.trace_has_more ? activityNow.trace_cursor ?? 0 : 0,
        notes_cursor: activityNow.notes_has_more ? activityNow.notes_cursor ?? 0 : 0,
      };
      const url =
        kind === "task"
          ? workspaceUrlWithParams(`/api/task/${entityId}`, params)
          : workspaceUrlWithParams(`/api/plan/${entityId}`, params);

      try {
        const payload =
          kind === "task"
            ? await fetchJson<TaskDetailPayload>(url)
            : await fetchJson<PlanDetailPayload>(url);
        if (!isCurrentDetail(token)) return;
        updateActivityState(activityNow, payload);
      } catch {
        // keep existing entries; just stop loading indicator
      } finally {
        if (isCurrentDetail(token)) {
          activityNow.loading_more = false;
          renderActivityHost(activityHost, entityId, token, kind);
        }
      }
    },
    activity.loading_more
  );
  activityHost.append(btn);
}

function openDetailFromNode(snapshot: Snapshot, node: GraphNode) {
  if (node.kind === "plan") {
    const plan = snapshot.plans.find((item) => item.id === node.id);
    if (plan) renderPlanDetail(snapshot, plan);
    return;
  }
  const task = snapshot.tasks.find((item) => item.id === node.id);
  if (task) renderTaskDetail(snapshot, task);
}

function sumTaskCounts(plans: PlanItem[]) {
  return plans.reduce(
    (acc, plan) => {
      acc.total += plan.task_counts.total;
      acc.active += plan.task_counts.active;
      acc.backlog += plan.task_counts.backlog;
      acc.parked += plan.task_counts.parked;
      acc.done += plan.task_counts.done;
      return acc;
    },
    { total: 0, active: 0, backlog: 0, parked: 0, done: 0 }
  );
}

function renderSummary(snapshot: Snapshot) {
  renderWorkspaceSelect(snapshot.workspace);
  nodes.updated.textContent = snapshot.generated_at.replace("T", " ").replace("Z", " UTC");

  const runner = snapshot.runner;
  if (runner && nodes.runnerStatus) {
    nodes.runnerStatus.textContent = formatRunnerStatus(runner.status);
    nodes.runnerStatus.dataset.state = (runner.status || "offline").toLowerCase();
  }
  if (runner && nodes.runnerJobs) {
    const queued = runner.jobs?.queued ?? 0;
    const running = runner.jobs?.running ?? 0;
    const bits: string[] = [];
    if (queued > 0) bits.push(`queued ${queued}`);
    if (running > 0) bits.push(`running ${running}`);
    nodes.runnerJobs.textContent = bits.length ? bits.join(" | ") : "—";
  }
  if (runner && nodes.runnerAutostart) {
    if (!autostartMutation.pending) {
      nodes.runnerAutostart.checked = !!runner.autostart?.enabled;
    }
    const locked = autostartMutation.pending || !!state.projectOverride;
    nodes.runnerAutostart.disabled = locked;
    const host = nodes.runnerAutostart.closest(".toggle") as HTMLElement | null;
    if (host) {
      host.title = state.projectOverride
        ? "Autostart is only configurable for the current project."
        : "Autostart runner locally (runtime only)";
    }
  }

  const focus = snapshot.focus;
  if (focus.kind === "none") {
    nodes.focus.textContent = "No focus";
    nodes.focusSub.textContent = "Set focus to highlight a goal.";
  } else {
    nodes.focus.textContent = focus.title || focus.id || "Focused";
    nodes.focusSub.textContent =
      focus.kind === "task" ? `Task / ${focus.id}` : `Plan / ${focus.id}`;
  }

  const guardStatus = snapshot.project_guard?.status;
  if (guardStatus === "uninitialized") {
    nodes.focusSub.textContent = `${nodes.focusSub.textContent} • guard uninitialized`;
  }

  nodes.planCount.textContent = snapshot.plans.length.toString();
  const activePlans = snapshot.plans.filter((plan) => plan.status !== "DONE").length;
  nodes.planBreakdown.textContent = `${activePlans} active`;

  const counts = sumTaskCounts(snapshot.plans);
  nodes.taskCount.textContent = counts.total.toString();
  nodes.taskBreakdown.textContent = [
    formatCount("active", counts.active),
    formatCount("backlog", counts.backlog),
    formatCount("parked", counts.parked),
    formatCount("done", counts.done),
  ].join(" | ");
}

function renderGoals(snapshot: Snapshot) {
  clear(nodes.goalList);
  snapshot.plans.forEach((plan) => {
    const item = document.createElement("button");
    item.className = "list-item";
    item.type = "button";
    if (plan.id === state.selectedPlanId) {
      item.classList.add("active");
    }

    const title = document.createElement("div");
    title.className = "item-title";
    title.textContent = plan.title || plan.id;

    const meta = document.createElement("div");
    meta.className = "item-meta";
    const status = document.createElement("span");
    status.className = "badge accent";
    status.textContent = formatStatus(plan.status);
    const count = document.createElement("span");
    count.className = "badge dim";
    count.textContent = `${plan.task_counts.done}/${plan.task_counts.total} done`;
    meta.append(status, count);

    item.append(title, meta);
    item.addEventListener("click", () => {
      state.selectedPlanId = plan.id;
      render(snapshot);
      renderPlanDetail(snapshot, plan);
    });

    nodes.goalList.append(item);
  });
}

function renderChecklist(snapshot: Snapshot) {
  clear(nodes.planChecklist);
  const planId = state.selectedPlanId ?? snapshot.primary_plan_id;
  const checklist =
    (planId && snapshot.plan_checklists?.[planId]) ||
    (snapshot.plan_checklist?.plan_id === planId ? snapshot.plan_checklist : null);
  if (!checklist) {
    const empty = document.createElement("div");
    empty.className = "list-item";
    empty.textContent = "No checklist for this plan.";
    nodes.planChecklist.append(empty);
    return;
  }
  if (!checklist.steps || checklist.steps.length === 0) {
    const empty = document.createElement("div");
    empty.className = "list-item";
    empty.textContent = "No checklist for this plan.";
    nodes.planChecklist.append(empty);
    return;
  }
  checklist.steps.forEach((step, index) => {
    const item = document.createElement("div");
    item.className = "list-item";
    if (index + 1 === checklist.current) {
      item.classList.add("active");
    }
    const title = document.createElement("div");
    title.className = "item-title";
    title.textContent = `${index + 1}. ${step}`;
    item.append(title);
    nodes.planChecklist.append(item);
  });
}

function renderTasks(snapshot: Snapshot) {
  clear(nodes.taskList);
  const tasks = snapshot.tasks.filter((task) =>
    state.selectedPlanId ? task.plan_id === state.selectedPlanId : true
  );
  if (tasks.length === 0) {
    const empty = document.createElement("div");
    empty.className = "list-item";
    empty.textContent = "No tasks for this plan.";
    nodes.taskList.append(empty);
    return;
  }
  tasks.forEach((task) => {
    const item = document.createElement("div");
    item.className = "list-item";
    const title = document.createElement("div");
    title.className = "item-title";
    title.textContent = task.title || task.id;

    const meta = document.createElement("div");
    meta.className = "item-meta";
    const status = document.createElement("span");
    status.className = "badge accent";
    status.textContent = formatStatus(task.status);
    meta.append(status);
    if (task.blocked) {
      const blocked = document.createElement("span");
      blocked.className = "badge dim";
      blocked.textContent = "blocked";
      meta.append(blocked);
    }
    const priority = document.createElement("span");
    priority.className = "badge dim";
    priority.textContent = formatStatus(task.priority);
    meta.append(priority);

    item.append(title, meta);
    item.addEventListener("click", () => {
      renderTaskDetail(snapshot, task);
    });
    nodes.taskList.append(item);
  });
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function hashToUnit(input: string) {
  let hash = 2166136261;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) / 4294967296;
}

function hashMix(hash: number, input: string) {
  let next = hash;
  for (let i = 0; i < input.length; i += 1) {
    next ^= input.charCodeAt(i);
    next = Math.imul(next, 16777619);
  }
  return next >>> 0;
}

function graphDataKey(snapshot: Snapshot) {
  let hash = 2166136261;
  snapshot.plans.forEach((plan) => {
    hash = hashMix(hash, plan.id);
    hash = hashMix(hash, plan.status || "");
    hash = hashMix(hash, String(plan.updated_at_ms || 0));
  });
  snapshot.tasks.forEach((task) => {
    hash = hashMix(hash, task.id);
    hash = hashMix(hash, task.plan_id || "");
    hash = hashMix(hash, task.status || "");
    hash = hashMix(hash, String(task.updated_at_ms || 0));
  });
  return hash >>> 0;
}

const STOP_WORDS = new Set([
  "the",
  "and",
  "for",
  "with",
  "to",
  "of",
  "in",
  "on",
  "a",
  "an",
  "or",
  "via",
  "as",
  "is",
  "are",
  "be",
  "by",
  "from",
  "at",
  "it",
  "this",
  "that",
  "these",
  "those",
  "plan",
  "task",
  "steps",
  "step",
  "mcp",
  "dx",
  "v1",
  "v2",
  "bm",
  "ii",
  "iii",
  "и",
  "для",
  "что",
  "это",
  "на",
  "в",
  "по",
  "из",
  "к",
  "с",
  "без",
  "или",
  "а",
]);

function tokenizeText(text: string) {
  const raw = (text || "").toLowerCase();
  const cleaned = raw.replace(/[^\p{L}\p{N}]+/gu, " ");
  return cleaned
    .split(" ")
    .map((t) => t.trim())
    .filter((t) => t.length >= 3)
    .filter((t) => !STOP_WORDS.has(t));
}

function tokenSet(text: string) {
  return new Set(tokenizeText(text));
}

function jaccardSimilarity(a: Set<string>, b: Set<string>) {
  if (!a.size || !b.size) return 0;
  let intersection = 0;
  const [small, large] = a.size <= b.size ? [a, b] : [b, a];
  small.forEach((value) => {
    if (large.has(value)) intersection += 1;
  });
  const union = a.size + b.size - intersection;
  return union > 0 ? intersection / union : 0;
}

function semanticVector(tokens: Set<string> | string[], fallbackId: string) {
  let x = 0;
  let y = 0;
  const list = Array.isArray(tokens) ? tokens : Array.from(tokens || []);
  for (let i = 0; i < list.length; i += 1) {
    const token = list[i];
    x += hashToUnit(`${token}|x`) * 2 - 1;
    y += hashToUnit(`${token}|y`) * 2 - 1;
  }
  const scale = Math.sqrt(Math.max(1, list.length));
  x = Math.tanh(x / scale);
  y = Math.tanh(y / scale);
  const len = Math.hypot(x, y);
  if (len < 1e-3) {
    const seed = hashToUnit(fallbackId || "");
    const angle = seed * Math.PI * 2;
    x = Math.cos(angle) * 0.35;
    y = Math.sin(angle) * 0.35;
  }
  return { x, y };
}

function pushTopK(list: { id: string; score: number }[], candidate: { id: string; score: number }, k: number) {
  list.push(candidate);
  list.sort((a, b) => b.score - a.score);
  if (list.length > k) list.length = k;
}

function buildKnnEdges(
  items: { id: string; group?: string; tokens: Set<string> }[],
  k: number,
  threshold: number,
  sameGroupBonus: number
) {
  const best = new Map<string, { id: string; score: number }[]>();
  for (let i = 0; i < items.length; i += 1) {
    best.set(items[i].id, []);
  }

  for (let i = 0; i < items.length; i += 1) {
    for (let j = i + 1; j < items.length; j += 1) {
      const a = items[i];
      const b = items[j];
      let score = jaccardSimilarity(a.tokens, b.tokens);
      if (score <= 0) continue;
      if (a.group && b.group && a.group === b.group) {
        score += sameGroupBonus;
      }
      if (score < threshold) continue;
      pushTopK(best.get(a.id)!, { id: b.id, score }, k);
      pushTopK(best.get(b.id)!, { id: a.id, score }, k);
    }
  }

  const dedupe = new Set<string>();
  const edges: GraphEdge[] = [];
  best.forEach((neighbors, id) => {
    neighbors.forEach((neighbor) => {
      const a = id;
      const b = neighbor.id;
      const key = a < b ? `${a}|${b}` : `${b}|${a}`;
      if (dedupe.has(key)) return;
      dedupe.add(key);
      edges.push({ from: a, to: b, type: "similar", weight: neighbor.score });
    });
  });
  return edges;
}

function buildGraphModel(snapshot: Snapshot, width: number, height: number): GraphModel {
  const nowMs = snapshot.generated_at_ms || Date.now();
  const statusRank = (status: string) => {
    switch (status) {
      case "ACTIVE":
        return 0;
      case "TODO":
        return 1;
      case "PARKED":
        return 2;
      case "DONE":
        return 3;
      default:
        return 4;
    }
  };

  const MAX_TASK_NODES = 320;
  const MAX_PLAN_NODES = 200;

  const tasksSorted = snapshot.tasks.slice().sort((a, b) => {
    const diff = statusRank(a.status) - statusRank(b.status);
    if (diff !== 0) return diff;
    const time = (b.updated_at_ms || 0) - (a.updated_at_ms || 0);
    if (time !== 0) return time;
    return a.id.localeCompare(b.id);
  });
  const tasks = tasksSorted.slice(0, Math.min(MAX_TASK_NODES, tasksSorted.length));

  const planById = new Map(snapshot.plans.map((plan) => [plan.id, plan] as const));
  const neededPlanIds = new Set<string>(tasks.map((task) => task.plan_id));
  const focus = snapshot.focus;
  if (focus?.kind === "plan" && focus.id) neededPlanIds.add(focus.id);
  if (focus?.kind === "task" && focus.plan_id) neededPlanIds.add(focus.plan_id);
  if (state.selectedPlanId) neededPlanIds.add(state.selectedPlanId);

  const plansSorted = snapshot.plans.slice().sort((a, b) => {
    const diff = statusRank(a.status) - statusRank(b.status);
    if (diff !== 0) return diff;
    const time = (b.updated_at_ms || 0) - (a.updated_at_ms || 0);
    if (time !== 0) return time;
    return a.id.localeCompare(b.id);
  });

  const plansPicked: PlanItem[] = [];
  const pickedIds = new Set<string>();
  neededPlanIds.forEach((id) => {
    const plan = planById.get(id);
    if (plan && !pickedIds.has(plan.id)) {
      plansPicked.push(plan);
      pickedIds.add(plan.id);
    }
  });
  for (let i = 0; i < plansSorted.length && plansPicked.length < MAX_PLAN_NODES; i += 1) {
    const plan = plansSorted[i];
    if (pickedIds.has(plan.id)) continue;
    plansPicked.push(plan);
    pickedIds.add(plan.id);
  }
  const plans = plansPicked;

  const nodesList: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const planMap = new Map<string, GraphNode>();

  const maxX = width * 0.48;
  const maxY = height * 0.48;
  const semanticX = maxX * 0.9;
  const semanticY = maxY * 0.9;

  const planTokenCache = new Map<string, Set<string>>();
  const taskTokenCache = new Map<string, Set<string>>();

  const computeHeat = (updatedAtMs?: number) => {
    const ageDays = Math.max(0, nowMs - (updatedAtMs || nowMs)) / (1000 * 60 * 60 * 24);
    return 1 / (1 + ageDays / 7);
  };

  plans.forEach((plan) => {
    const tokens = tokenSet(`${plan.title || plan.id} ${plan.description || ""}`);
    planTokenCache.set(plan.id, tokens);
    const vec = semanticVector(tokens, plan.id);
    const heat = computeHeat(plan.updated_at_ms);
    const radiusScale = clamp(0.55 + (1 - heat) * 0.35, 0.55, 0.9);
    const tx = vec.x * semanticX * radiusScale;
    const ty = vec.y * semanticY * radiusScale * 0.82;
    const jitter = (hashToUnit(`${plan.id}-j`) - 0.5) * 18;
    const x = tx + jitter;
    const y = ty - jitter * 0.6;
    const node: GraphNode = {
      id: plan.id,
      kind: "plan",
      plan_id: plan.id,
      status: plan.status,
      label: plan.title || plan.id,
      x,
      y,
      tx,
      ty,
      vx: 0,
      vy: 0,
      radius: 8,
    };
    nodesList.push(node);
    planMap.set(plan.id, node);
  });

  tasks.forEach((task, index) => {
    const anchor =
      planMap.get(task.plan_id) ?? nodesList[index % Math.max(1, nodesList.length)];
    const tokens = tokenSet(`${task.title || task.id} ${task.description || ""}`);
    taskTokenCache.set(task.id, tokens);
    const vec = semanticVector(tokens, task.id);
    const heat = computeHeat(task.updated_at_ms);
    const statusBoost = task.status === "DONE" ? 38 : task.status === "PARKED" ? 16 : 0;
    const radius = 62 + statusBoost + (1 - heat) * 26;
    const tx = (anchor?.x ?? 0) + vec.x * radius;
    const ty = (anchor?.y ?? 0) + vec.y * radius;
    const jitter = (hashToUnit(`${task.id}-j`) - 0.5) * 10;
    const x = tx + jitter;
    const y = ty - jitter * 0.6;
    const node: GraphNode = {
      id: task.id,
      kind: "task",
      plan_id: task.plan_id,
      status: task.status,
      label: task.title || task.id,
      x,
      y,
      tx,
      ty,
      vx: 0,
      vy: 0,
      radius: 3,
    };
    nodesList.push(node);
    if (anchor) {
      edges.push({ from: task.id, to: anchor.id, type: "hierarchy", weight: 1 });
    }
  });

  const nodeIndex = new Map(nodesList.map((node) => [node.id, node] as const));

  const planItems = plans.map((plan) => ({
    id: plan.id,
    group: "plan",
    tokens: planTokenCache.get(plan.id) || tokenSet(`${plan.title || plan.id} ${plan.description || ""}`),
  }));
  const taskItems = tasks.map((task) => ({
    id: task.id,
    group: task.plan_id,
    tokens: taskTokenCache.get(task.id) || tokenSet(`${task.title || task.id} ${task.description || ""}`),
  }));

  buildKnnEdges(planItems, 2, 0.26, 0.0).forEach((edge) => edges.push(edge));
  buildKnnEdges(taskItems, 3, 0.28, 0.06).forEach((edge) => edges.push(edge));

  const steps = clamp(160 + Math.floor(nodesList.length * 0.25), 160, 260);
  const repulsion = 4200;
  const damp = 0.82;
  const repulsionCutoff = clamp(Math.min(maxX, maxY) * 0.75, 220, 520);
  for (let step = 0; step < steps; step += 1) {
    for (let i = 0; i < nodesList.length; i += 1) {
      for (let j = i + 1; j < nodesList.length; j += 1) {
        const a = nodesList[i];
        const b = nodesList[j];
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        const dist2 = dx * dx + dy * dy + 0.01;
        if (dist2 > repulsionCutoff * repulsionCutoff) continue;
        const minDist = a.radius + b.radius + 9;
        const force = repulsion / dist2 + (dist2 < minDist * minDist ? 18 : 0);
        const dist = Math.sqrt(dist2);
        dx /= dist;
        dy /= dist;
        a.vx += dx * force;
        a.vy += dy * force;
        b.vx -= dx * force;
        b.vy -= dy * force;
      }
    }

    edges.forEach((edge) => {
      const a = nodeIndex.get(edge.from);
      const b = nodeIndex.get(edge.to);
      if (!a || !b) return;
      let dx = a.x - b.x;
      let dy = a.y - b.y;
      const dist = Math.sqrt(dx * dx + dy * dy) + 0.01;

      const isSimilar = edge.type === "similar";
      const weight = typeof edge.weight === "number" ? edge.weight : 0;
      let target = 82;
      let strength = 0.012;
      if (isSimilar) {
        target = clamp(132 - weight * 110, 58, 132);
        strength = 0.007;
      } else {
        target = a.status === "DONE" ? 102 : a.status === "PARKED" ? 90 : 78;
        strength = 0.014;
      }

      const diff = dist - target;
      const force = diff * strength;
      dx /= dist;
      dy /= dist;
      a.vx -= dx * force;
      a.vy -= dy * force;
      b.vx += dx * force;
      b.vy += dy * force;
    });

    nodesList.forEach((node) => {
      const pull = node.kind === "plan" ? 0.02 : 0.014;
      const tx = node.tx ?? 0;
      const ty = node.ty ?? 0;
      node.vx += (tx - node.x) * pull;
      node.vy += (ty - node.y) * pull;
      node.vx += -node.x * 0.0015;
      node.vy += -node.y * 0.0015;
      node.vx *= damp;
      node.vy *= damp;
      node.x = clamp(node.x + node.vx, -maxX, maxX);
      node.y = clamp(node.y + node.vy, -maxY, maxY);
    });
  }

  return { nodes: nodesList, edges, width, height };
}

function ensureGraphView(width: number, height: number) {
  if (!graphState.view) {
    graphState.view = {
      offsetX: width * 0.5,
      offsetY: height * 0.5,
      scale: 1,
      dragging: false,
      draggingNodeId: null,
      dragNodeOffsetX: 0,
      dragNodeOffsetY: 0,
      lastX: 0,
      lastY: 0,
      moved: false,
    };
  }
}

function screenToWorld(view: GraphView, x: number, y: number) {
  return {
    x: (x - view.offsetX) / view.scale,
    y: (y - view.offsetY) / view.scale,
  };
}

function hitTestNode(model: GraphModel, view: GraphView, x: number, y: number) {
  const world = screenToWorld(view, x, y);
  for (let i = model.nodes.length - 1; i >= 0; i -= 1) {
    const node = model.nodes[i];
    const dx = world.x - node.x;
    const dy = world.y - node.y;
    const radius = node.radius + 6 / view.scale;
    if (dx * dx + dy * dy <= radius * radius) {
      return node;
    }
  }
  return null;
}

function drawGraph(snapshot: Snapshot) {
  const canvas = nodes.graph;
  const model = graphState.model;
  const view = graphState.view;
  if (!canvas || !model || !view) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const ratio = graphState.pixelRatio || 1;

  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.setTransform(
    view.scale * ratio,
    0,
    0,
    view.scale * ratio,
    view.offsetX * ratio,
    view.offsetY * ratio
  );

  const selectedPlanId = state.selectedPlanId ?? snapshot.primary_plan_id;
  const focus = snapshot.focus;
  const focusPlanId =
    focus.kind === "plan" ? focus.id : focus.kind === "task" ? focus.plan_id : null;
  const selectedId = state.detailSelection ? state.detailSelection.id : null;
  const hoverId = graphState.hoverId;
  const nodeById = new Map(model.nodes.map((node) => [node.id, node] as const));
  const highlight = new Set<string>();
  if (selectedId) highlight.add(selectedId);
  if (hoverId) highlight.add(hoverId);
  if (focus?.id) highlight.add(focus.id);

  ctx.globalAlpha = 1;
  model.edges.forEach((edge) => {
    const from = nodeById.get(edge.from);
    const to = nodeById.get(edge.to);
    if (!from || !to) return;

    const isSimilar = edge.type === "similar";
    const weight = typeof edge.weight === "number" ? edge.weight : 0;

    const connected =
      (selectedId && (edge.from === selectedId || edge.to === selectedId)) ||
      (hoverId && (edge.from === hoverId || edge.to === hoverId)) ||
      (focus?.id && (edge.from === focus.id || edge.to === focus.id));
    if (connected) {
      highlight.add(from.id);
      highlight.add(to.id);
    }

    const inSelected =
      from.id === selectedPlanId ||
      to.id === selectedPlanId ||
      from.plan_id === selectedPlanId ||
      to.plan_id === selectedPlanId;
    const inFocus =
      (focusPlanId &&
        (from.id === focusPlanId ||
          to.id === focusPlanId ||
          from.plan_id === focusPlanId ||
          to.plan_id === focusPlanId)) ||
      false;
    const cluster = inSelected || inFocus;

    if (isSimilar && !connected) {
      if (view.scale < 0.95) return;
      if (weight < 0.34 && !cluster) return;
    }

    const alpha = connected
      ? isSimilar
        ? 0.32
        : 0.58
      : cluster
        ? isSimilar
          ? 0.1 + clamp(weight, 0, 1) * 0.12
          : 0.18
        : isSimilar
          ? 0.05 + clamp(weight, 0, 1) * 0.08
          : 0.1;

    const color = isSimilar ? "132, 169, 255" : "125, 211, 199";
    ctx.strokeStyle = `rgba(${color}, ${alpha})`;
    ctx.lineWidth = connected ? (isSimilar ? 1.2 : 1.6) : isSimilar ? 1 : 1.1;
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
  });

  model.nodes
    .filter((node) => node.kind === "task")
    .forEach((node) => {
      const belongs =
        highlight.has(node.id) ||
        node.plan_id === selectedPlanId ||
        node.plan_id === focusPlanId;
      const alpha = highlight.has(node.id) ? 0.95 : belongs ? 0.7 : 0.22;
      const tint =
        node.status === "DONE"
          ? "rgba(125, 211, 199, 0.55)"
          : "rgba(255, 255, 255, 0.9)";
      ctx.fillStyle = tint.replace(/0\.9\)/, `${alpha})`).replace(/0\.55\)/, `${alpha})`);
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fill();
    });

  model.nodes
    .filter((node) => node.kind === "plan")
    .forEach((node) => {
      const isActive =
        highlight.has(node.id) || node.id === selectedPlanId || node.id === focusPlanId;
      ctx.fillStyle = isActive
        ? "rgba(125, 211, 199, 0.92)"
        : "rgba(125, 211, 199, 0.35)";
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fill();
    });

  if (hoverId) {
    const node = model.nodes.find((n) => n.id === hoverId);
    if (node) {
      ctx.strokeStyle = "rgba(132, 169, 255, 0.8)";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius + 4, 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  if (focus && focus.id) {
    const node = model.nodes.find((n) => n.id === focus.id);
    if (node) {
      ctx.strokeStyle = "rgba(132, 169, 255, 0.85)";
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius + 7, 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.font = `${12 * ratio}px "IBM Plex Sans", "SF Pro Text", "Segoe UI", sans-serif`;
  ctx.fillStyle = "rgba(238, 242, 246, 0.85)";
  model.nodes.forEach((node) => {
    const shouldLabel =
      (node.kind === "plan" &&
        (node.id === selectedPlanId || node.id === focusPlanId || node.id === hoverId)) ||
      (node.kind === "task" && (node.id === selectedId || node.id === hoverId));
    if (!shouldLabel) return;
    const sx = (node.x * view.scale + view.offsetX) * ratio;
    const sy = (node.y * view.scale + view.offsetY) * ratio;
    ctx.fillText(node.label, sx + 10, sy - 8);
  });
}

function ensureGraphHandlers() {
  if (graphState.handlersReady) return;
  const canvas = nodes.graph;
  if (!canvas) return;
  graphState.handlersReady = true;
  canvas.style.cursor = "grab";

  canvas.addEventListener("pointerdown", (event) => {
    const model = graphState.model;
    const view = graphState.view;
    if (!view) return;
    const rect = canvas.getBoundingClientRect();
    view.moved = false;
    const localX = event.clientX - rect.left;
    const localY = event.clientY - rect.top;

    if (model) {
      const hit = hitTestNode(model, view, localX, localY);
      if (hit) {
        const world = screenToWorld(view, localX, localY);
        view.draggingNodeId = hit.id;
        view.dragNodeOffsetX = world.x - hit.x;
        view.dragNodeOffsetY = world.y - hit.y;
        hit.vx = 0;
        hit.vy = 0;
        canvas.setPointerCapture(event.pointerId);
        canvas.style.cursor = "grabbing";
        return;
      }
    }

    view.dragging = true;
    view.draggingNodeId = null;
    view.lastX = localX;
    view.lastY = localY;
    canvas.setPointerCapture(event.pointerId);
    canvas.style.cursor = "grabbing";
  });

  canvas.addEventListener("pointermove", (event) => {
    const model = graphState.model;
    const view = graphState.view;
    if (!model || !view) return;
    const rect = canvas.getBoundingClientRect();
    const localX = event.clientX - rect.left;
    const localY = event.clientY - rect.top;
    if (view.draggingNodeId) {
      const node = model.nodes.find((n) => n.id === view.draggingNodeId);
      if (node) {
        const world = screenToWorld(view, localX, localY);
        node.x = world.x - view.dragNodeOffsetX;
        node.y = world.y - view.dragNodeOffsetY;
        node.tx = node.x;
        node.ty = node.y;
        node.vx = 0;
        node.vy = 0;
      }
      view.moved = true;
      if (state.snapshot) {
        drawGraph(state.snapshot);
      }
      return;
    }
    if (view.dragging) {
      const dx = localX - view.lastX;
      const dy = localY - view.lastY;
      view.offsetX += dx;
      view.offsetY += dy;
      view.lastX = localX;
      view.lastY = localY;
      view.moved = true;
      if (state.snapshot) {
        drawGraph(state.snapshot);
      }
      return;
    }
    const hit = hitTestNode(model, view, localX, localY);
    const nextHover = hit ? hit.id : null;
    if (nextHover !== graphState.hoverId) {
      graphState.hoverId = nextHover;
      if (state.snapshot) {
        drawGraph(state.snapshot);
      }
    }
    canvas.style.cursor = hit ? "pointer" : "grab";
  });

  canvas.addEventListener("pointerup", (event) => {
    const model = graphState.model;
    const view = graphState.view;
    if (!model || !view) return;
    if (!view.moved) {
      const rect = canvas.getBoundingClientRect();
      const hit = hitTestNode(
        model,
        view,
        event.clientX - rect.left,
        event.clientY - rect.top
      );
      if (hit) {
        if (hit.kind === "plan") {
          state.selectedPlanId = hit.id;
        } else if (hit.plan_id) {
          state.selectedPlanId = hit.plan_id;
        }
        if (state.snapshot) {
          render(state.snapshot);
          openDetailFromNode(state.snapshot, hit);
        }
      }
    }
    view.dragging = false;
    view.draggingNodeId = null;
    canvas.style.cursor = "grab";
  });

  canvas.addEventListener("pointerleave", () => {
    if (!graphState.view?.dragging && !graphState.view?.draggingNodeId) {
      graphState.hoverId = null;
    }
    if (state.snapshot) {
      drawGraph(state.snapshot);
    }
  });

  canvas.addEventListener(
    "wheel",
    (event) => {
      const view = graphState.view;
      if (!view) return;
      const rect = canvas.getBoundingClientRect();
      event.preventDefault();
      const zoom = event.deltaY < 0 ? 1.08 : 0.92;
      const mouseX = event.clientX - rect.left;
      const mouseY = event.clientY - rect.top;
      const world = screenToWorld(view, mouseX, mouseY);
      view.scale = clamp(view.scale * zoom, 0.6, 2.2);
      view.offsetX = mouseX - world.x * view.scale;
      view.offsetY = mouseY - world.y * view.scale;
      if (state.snapshot) {
        drawGraph(state.snapshot);
      }
    },
    { passive: false },
  );

  canvas.addEventListener("dblclick", () => {
    if (!graphState.view || !graphState.model) return;
    graphState.view.offsetX = graphState.model.width * 0.5;
    graphState.view.offsetY = graphState.model.height * 0.5;
    graphState.view.scale = 1;
    if (state.snapshot) {
      drawGraph(state.snapshot);
    }
  });
}

function renderGraph(snapshot: Snapshot) {
  const canvas = nodes.graph;
  if (!canvas) return;
  const rect = canvas.getBoundingClientRect();
  const width = Math.max(1, rect.width);
  const height = Math.max(1, rect.height);
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(width * ratio));
  canvas.height = Math.max(1, Math.floor(height * ratio));
  graphState.pixelRatio = ratio;
  ensureGraphView(width, height);
  const key = graphDataKey(snapshot);
  if (!graphState.model || graphState.snapshotKey !== key) {
    graphState.model = buildGraphModel(snapshot, width, height);
    graphState.snapshotKey = key;
  } else if (graphState.model.width !== width || graphState.model.height !== height) {
    graphState.model = buildGraphModel(snapshot, width, height);
  }
  ensureGraphHandlers();
  drawGraph(snapshot);
}

function render(snapshot: Snapshot) {
  state.snapshot = snapshot;
  if (!state.selectedPlanId) {
    state.selectedPlanId = snapshot.primary_plan_id || snapshot.plans[0]?.id || null;
  }
  renderSummary(snapshot);
  renderGoals(snapshot);
  renderChecklist(snapshot);
  renderTasks(snapshot);
  renderGraph(snapshot);
}

function renderError(payload: SnapshotError) {
  const message = payload.error.message || "Unable to load snapshot.";
  nodes.focus.textContent = "Viewer error";
  nodes.focusSub.textContent = payload.error.code;
  nodes.planBreakdown.textContent = payload.error.recovery || "Check server settings.";
  nodes.taskBreakdown.textContent = "";
  if (nodes.runnerStatus) {
    nodes.runnerStatus.textContent = "offline";
    nodes.runnerStatus.dataset.state = "offline";
  }
  if (nodes.runnerJobs) {
    nodes.runnerJobs.textContent = "—";
  }
  if (nodes.runnerAutostart) {
    nodes.runnerAutostart.disabled = true;
  }
  clear(nodes.goalList);
  const item = document.createElement("div");
  item.className = "list-item";
  item.textContent = message;
  nodes.goalList.append(item);
}

async function loadSnapshot() {
  try {
    const response = await fetchWithTimeout(
      workspaceUrl("/api/snapshot"),
      { cache: "no-store" },
      7_000
    );
    const payload = (await response.json()) as Snapshot | SnapshotError;
    if ("error" in payload) {
      if (
        payload.error.code === "PROJECT_GUARD_MISMATCH" &&
        !workspaceMutation.recoveredGuardMismatch
      ) {
        await loadAbout();
        if (state.workspaceRecommended && state.workspaceRecommended !== state.workspaceOverride) {
          workspaceMutation.recoveredGuardMismatch = true;
          setWorkspaceOverride(state.workspaceRecommended);
          await loadSnapshot();
          return;
        }
      }
      renderError(payload);
      return;
    }
    render(payload);
  } catch (err) {
    renderError({
      error: { code: "NETWORK_ERROR", message: "Snapshot unavailable." },
    });
  }
}

window.addEventListener("resize", () => {
  if (state.snapshot) {
    renderGraph(state.snapshot);
  }
});

if (nodes.detailClose) {
  nodes.detailClose.addEventListener("click", () => setDetailVisible(false));
}

if (nodes.project) {
  nodes.project.addEventListener("change", async () => {
    const next = (nodes.project.value || "").trim();
    if (!next || (state.currentProjectGuard && next === state.currentProjectGuard)) {
      setProjectOverride(null);
    } else {
      setProjectOverride(next);
    }
    workspaceMutation.recoveredGuardMismatch = false;
    loadWorkspaceOverrideFromStorage();
    state.selectedPlanId = null;
    state.detailSelection = null;
    setDetailVisible(false);
    renderProjectSelect();
    await loadWorkspaces();
    renderWorkspaceSelect(null);
    void loadAbout();
    void loadSnapshot();
  });
}

if (nodes.workspace) {
  nodes.workspace.addEventListener("change", () => {
    const next = (nodes.workspace.value || "").trim();
    if (!next) {
      setWorkspaceOverride(null);
    } else {
      setWorkspaceOverride(next);
    }
    workspaceMutation.recoveredGuardMismatch = false;
    state.selectedPlanId = null;
    state.detailSelection = null;
    setDetailVisible(false);
    void loadAbout();
    void loadSnapshot();
  });
}

if (nodes.runnerAutostart) {
  nodes.runnerAutostart.addEventListener("change", async () => {
    if (autostartMutation.pending) return;
    autostartMutation.pending = true;
    const desired = nodes.runnerAutostart.checked;
    nodes.runnerAutostart.disabled = true;
    try {
      await postJson<{ ok: boolean }>(`/api/settings/runner_autostart`, { enabled: desired });
    } catch (err) {
      // Revert to the last known snapshot state (if available).
      if (state.snapshot) {
        nodes.runnerAutostart.checked = !!state.snapshot.runner?.autostart?.enabled;
      }
    } finally {
      autostartMutation.pending = false;
      nodes.runnerAutostart.disabled = false;
      void loadSnapshot();
    }
  });
}

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    setDetailVisible(false);
  }
});

async function boot() {
  await loadProjects();
  renderProjectSelect();
  await loadWorkspaces();
  renderWorkspaceSelect(null);
  void loadAbout();
  void loadSnapshot();
}

void boot();

async function refreshProjects() {
  if (projectsMutation.pending) return;
  projectsMutation.pending = true;
  const before = state.projectOverride;
  try {
    await loadProjects();
    renderProjectSelect();
    if (before !== state.projectOverride) {
      await loadWorkspaces();
      renderWorkspaceSelect(state.snapshot?.workspace || null);
    }
  } finally {
    projectsMutation.pending = false;
  }
}

window.setInterval(() => {
  if (document.visibilityState !== "visible") return;
  void loadSnapshot();
}, 3000);

window.setInterval(() => {
  if (document.visibilityState !== "visible") return;
  void refreshProjects();
}, 10_000);

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    void refreshProjects();
    void loadSnapshot();
  }
});
