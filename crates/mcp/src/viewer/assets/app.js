// Source of truth for the viewer UI (no TS build step).
const state = {
  snapshot: null,
  lens: "work",
  selectedPlanId: null,
  detailToken: 0,
  detailSelection: null,
  navStack: [],
  navIndex: -1,
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

const graphState = {
  model: null,
  view: null,
  lod: "overview",
  hoverId: null,
  snapshotKey: 0,
  displayKey: null,
  handlersReady: false,
  pixelRatio: 1,
  animating: false,
  animationFrame: 0,
  settle: null,
  cameraAnim: null,
  lodDebounceTimer: 0,
  local: {
    centerId: null,
    hops: 2,
    ids: new Set(),
    key: null,
  },
  lastCanvasWidth: 0,
  lastCanvasHeight: 0,
};

const layoutCache = {
  nodesById: new Map(),
  fade: new Map(),
  visibleIds: new Set(),
  renderIds: new Set(),
  saveTimer: 0,
  lastViewKey: null,
  layoutSaveTimer: 0,
  lastLayoutKey: null,
  storedPositions: new Map(),
};

const GRAPH_LIMITS = {
  maxTasksInPlan: 600,
  maxPlans: 220,
};

const GRAPH_CONST = {
  worldPlanRadius: 900,
  worldTaskRadiusBase: 140,
  tile: 0.45,
  lodClustersAt: 0.9,
  lodClustersOutAt: 0.84,
  lodTasksAt: 1.35,
  lodTasksOutAt: 1.25,
  lodDebounceMs: 120,
  showTaskLabelsAt: 1.75,
  showTaskSimilarEdgesAt: 1.55,
  fadeMs: 260,
  settleMs: 400,
};

const autostartMutation = { pending: false };
const workspaceMutation = { recoveredGuardMismatch: false };
const projectsMutation = { pending: false };
const navMutation = { applying: false };

const windowUi = {
  zCounter: 80,
  explorer: { open: false, pinned: false, anchor: "left", x: 32, y: 32, z: 60 },
  detail: { open: false, pinned: false, anchor: "right", x: 32, y: 32, z: 70 },
  dragging: null,
  lastExplorerKey: null,
  lastDetailKey: null,
};
const snapshotMutation = { pending: false, queued: false, timer: 0, lastRequestedAt: 0 };
const liveMutation = { source: null, open: false, lastEventId: null, lastMessageAt: 0, failures: 0 };

const graphOverlay = {
  contextKey: null,
  plans: new Map(),
  tasks: new Map(),
  maxPlans: 800,
  maxTasks: 4500,
};

function graphOverlayContextKey(lensOverride) {
  const project = ((state.projectOverride || state.currentProjectGuard || "current") + "").trim() || "current";
  const workspace = ((state.workspaceOverride || "auto") + "").trim() || "auto";
  const lens = normalizeLens(lensOverride || state.lens || "work");
  return `${project}::${workspace}::${lens}`;
}

function clearGraphOverlay() {
  graphOverlay.contextKey = null;
  graphOverlay.plans.clear();
  graphOverlay.tasks.clear();
}

function ensureGraphOverlayContext(lensOverride) {
  const key = graphOverlayContextKey(lensOverride);
  if (graphOverlay.contextKey && graphOverlay.contextKey === key) return;
  clearGraphOverlay();
  graphOverlay.contextKey = key;
}

function overlayTouch(map, id, value) {
  map.delete(id);
  map.set(id, value);
}

function pruneOverlayMap(map, maxSize) {
  while (map.size > maxSize) {
    const first = map.keys().next();
    if (first.done) break;
    map.delete(first.value);
  }
}

function ingestGraphOverlay(payload) {
  if (!payload || typeof payload !== "object") return;
  ensureGraphOverlayContext(payload?.lens);

  const plan = payload.plan;
  if (plan && plan.id) {
    overlayTouch(graphOverlay.plans, plan.id, plan);
  }

  const tasks = payload.tasks;
  if (Array.isArray(tasks)) {
    tasks.forEach((task) => {
      if (!task || !task.id) return;
      overlayTouch(graphOverlay.tasks, task.id, task);
    });
  }

  pruneOverlayMap(graphOverlay.plans, graphOverlay.maxPlans);
  pruneOverlayMap(graphOverlay.tasks, graphOverlay.maxTasks);
}

function mergeGraphOverlay(snapshot) {
  if (!snapshot || typeof snapshot !== "object") return snapshot;
  ensureGraphOverlayContext(snapshot?.lens);

  const basePlans = Array.isArray(snapshot.plans) ? snapshot.plans : [];
  const baseTasks = Array.isArray(snapshot.tasks) ? snapshot.tasks : [];

  const planOverlay = graphOverlay.plans;
  const taskOverlay = graphOverlay.tasks;

  const seenPlans = new Set();
  const plans = [];
  basePlans.forEach((plan) => {
    if (!plan || !plan.id) return;
    seenPlans.add(plan.id);
    plans.push(planOverlay.get(plan.id) || plan);
  });
  Array.from(planOverlay.keys())
    .filter((id) => !seenPlans.has(id))
    .sort((a, b) => a.localeCompare(b))
    .forEach((id) => {
      const plan = planOverlay.get(id);
      if (plan) plans.push(plan);
    });

  const seenTasks = new Set();
  const tasks = [];
  baseTasks.forEach((task) => {
    if (!task || !task.id) return;
    seenTasks.add(task.id);
    tasks.push(taskOverlay.get(task.id) || task);
  });
  Array.from(taskOverlay.keys())
    .filter((id) => !seenTasks.has(id))
    .sort((a, b) => a.localeCompare(b))
    .forEach((id) => {
      const task = taskOverlay.get(id);
      if (task) tasks.push(task);
    });

  return { ...snapshot, plans, tasks };
}

const PROJECT_STORAGE_KEY = "bm_viewer_project";
const WORKSPACE_STORAGE_KEY = "bm_viewer_workspace";
const WORKSPACE_STORAGE_PREFIX = "bm_viewer_workspace:";
const LENS_STORAGE_KEY = "bm_viewer_lens";
const SIDEBAR_OPEN_STORAGE_PREFIX = "bm_viewer_sidebar_open:";
const DETAIL_WINDOW_STORAGE_PREFIX = "bm_viewer_detail_window:";

function queryFlag(name) {
  try {
    const params = new URLSearchParams(window.location.search || "");
    const raw = (params.get(name) || "").trim().toLowerCase();
    return raw === "1" || raw === "true" || raw === "yes" || raw === "on";
  } catch {
    return false;
  }
}

function queryParam(name) {
  try {
    const params = new URLSearchParams(window.location.search || "");
    const raw = (params.get(name) || "").trim();
    return raw || null;
  } catch {
    return null;
  }
}

function normalizeLens(value) {
  const raw = ((value || "") + "").trim().toLowerCase();
  if (raw === "knowledge") return "knowledge";
  return "work";
}

// UI mode: default = flagship. Use `?ui=legacy` to keep the previous graph renderer.
const UI_MODE = (queryParam("ui") || "").trim().toLowerCase() === "legacy" ? "legacy" : "flagship";
try {
  document.documentElement.dataset.ui = UI_MODE;
} catch {
  // ignore DOM failures
}

function workspaceUrl(path) {
  const parts = [];
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

function workspaceUrlWithParams(path, params) {
  const base = workspaceUrl(path);
  const parts = [];
  if (params && typeof params === "object") {
    Object.entries(params).forEach(([key, value]) => {
      if (value === null || value === undefined) return;
      parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`);
    });
  }
  if (parts.length === 0) return base;
  const join = base.includes("?") ? "&" : "?";
  return `${base}${join}${parts.join("&")}`;
}

function activeProjectKey() {
  const key = (state.projectOverride || state.currentProjectGuard || "current").trim();
  return key || "current";
}

function workspaceStorageKey() {
  return `${WORKSPACE_STORAGE_PREFIX}${activeProjectKey()}`;
}

function activeWorkspaceKey() {
  const ws = ((state.workspaceOverride || state.snapshot?.workspace || "auto") + "").trim();
  return ws || "auto";
}

function activeLensKey() {
  return normalizeLens(state.lens || state.snapshot?.lens || "work");
}

function windowScopeKey() {
  return `${activeProjectKey()}:${activeWorkspaceKey()}:${activeLensKey()}`;
}

function legacySidebarOpenStorageKey() {
  return `${SIDEBAR_OPEN_STORAGE_PREFIX}${activeProjectKey()}`;
}

function sidebarOpenStorageKey() {
  return `${SIDEBAR_OPEN_STORAGE_PREFIX}${windowScopeKey()}`;
}

function detailWindowStorageKey() {
  return `${DETAIL_WINDOW_STORAGE_PREFIX}${windowScopeKey()}`;
}

function setProjectOverride(value) {
  const next = (value || "").trim();
  state.projectOverride = next || null;
  try {
    if (state.projectOverride) {
      localStorage.setItem(PROJECT_STORAGE_KEY, state.projectOverride);
    } else {
      localStorage.removeItem(PROJECT_STORAGE_KEY);
    }
  } catch {
    // ignore storage failures
  }
}

function setWorkspaceOverride(value) {
  const next = (value || "").trim();
  state.workspaceOverride = next || null;
  try {
    if (state.workspaceOverride) {
      localStorage.setItem(workspaceStorageKey(), state.workspaceOverride);
    } else {
      localStorage.removeItem(workspaceStorageKey());
    }
  } catch {
    // ignore storage failures
  }
}

function loadWorkspaceOverrideFromStorage() {
  let stored = null;
  try {
    stored = localStorage.getItem(workspaceStorageKey());
    if (!stored) {
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

function setLens(value) {
  const next = normalizeLens(value);
  state.lens = next;
  try {
    localStorage.setItem(LENS_STORAGE_KEY, next);
  } catch {
    // ignore storage failures
  }
  if (nodes.lens) {
    nodes.lens.value = next;
  }
  applyLensCopy();
  applyExplorerWindowPreference({ defaultOpen: true });
  applyDetailWindowPreference({ defaultOpen: false });
}

function loadLensFromStorage() {
  let stored = null;
  try {
    stored = localStorage.getItem(LENS_STORAGE_KEY);
  } catch {
    stored = null;
  }
  const fromQuery = queryParam("lens");
  setLens(fromQuery || stored || state.lens || "work");
}

function parseStoredBool(value) {
  const raw = ((value || "") + "").trim().toLowerCase();
  if (!raw) return null;
  if (raw === "1" || raw === "true" || raw === "yes" || raw === "on") return true;
  if (raw === "0" || raw === "false" || raw === "no" || raw === "off") return false;
  return null;
}

function parseStoredWindowState(value) {
  const raw = ((value || "") + "").trim();
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object") return parsed;
  } catch {
    // ignore
  }
  const open = parseStoredBool(raw);
  if (open === null) return null;
  return { open };
}

function loadExplorerWindowStateFromStorage() {
  let stored = null;
  try {
    stored = localStorage.getItem(sidebarOpenStorageKey());
    if (!stored) {
      const legacy = localStorage.getItem(legacySidebarOpenStorageKey());
      if (legacy) {
        stored = legacy;
        localStorage.setItem(sidebarOpenStorageKey(), legacy);
      }
    }
  } catch {
    stored = null;
  }
  const parsed = parseStoredWindowState(stored) || {};
  const open = typeof parsed.open === "boolean" ? parsed.open : null;
  const pinned = typeof parsed.pinned === "boolean" ? parsed.pinned : false;
  const anchor = parsed.anchor === "right" ? "right" : "left";
  const x = typeof parsed.x === "number" ? parsed.x : windowUi.explorer.x;
  const y = typeof parsed.y === "number" ? parsed.y : windowUi.explorer.y;
  const z = typeof parsed.z === "number" ? parsed.z : windowUi.explorer.z;
  return { open, pinned, anchor, x, y, z };
}

function loadDetailWindowStateFromStorage() {
  let stored = null;
  try {
    stored = localStorage.getItem(detailWindowStorageKey());
  } catch {
    stored = null;
  }
  const parsed = parseStoredWindowState(stored) || {};
  const open = typeof parsed.open === "boolean" ? parsed.open : null;
  const pinned = typeof parsed.pinned === "boolean" ? parsed.pinned : false;
  const anchor = parsed.anchor === "left" ? "left" : "right";
  const x = typeof parsed.x === "number" ? parsed.x : windowUi.detail.x;
  const y = typeof parsed.y === "number" ? parsed.y : windowUi.detail.y;
  const z = typeof parsed.z === "number" ? parsed.z : windowUi.detail.z;
  return { open, pinned, anchor, x, y, z };
}

function applyExplorerWindowPreference({ defaultOpen } = {}) {
  const key = sidebarOpenStorageKey();
  if (windowUi.lastExplorerKey === key) return;
  windowUi.lastExplorerKey = key;
  const stored = loadExplorerWindowStateFromStorage();
  windowUi.explorer.pinned = stored.pinned;
  windowUi.explorer.anchor = stored.anchor;
  windowUi.explorer.x = stored.x;
  windowUi.explorer.y = stored.y;
  windowUi.explorer.z = stored.z;

  const open = stored.pinned ? true : stored.open === null ? defaultOpen !== false : stored.open;
  setSidebarPinned(stored.pinned, { persist: false });
  setSidebarVisible(open, { persist: false, focus: false, clamp: true });
}

function applyDetailWindowPreference({ defaultOpen } = {}) {
  const key = detailWindowStorageKey();
  if (windowUi.lastDetailKey === key) return;
  windowUi.lastDetailKey = key;
  const stored = loadDetailWindowStateFromStorage();
  windowUi.detail.pinned = stored.pinned;
  windowUi.detail.anchor = stored.anchor;
  windowUi.detail.x = stored.x;
  windowUi.detail.y = stored.y;
  windowUi.detail.z = stored.z;

  const open = stored.pinned ? true : stored.open === null ? defaultOpen === true : stored.open;
  setDetailPinned(stored.pinned, { persist: false });
  setDetailVisible(open, { persist: false, focus: false, clamp: true });
}

async function loadProjects() {
  try {
    const payload = await fetchJson("/api/projects");
    const currentGuard = (payload.current_project_guard || "").trim() || null;
    const currentLabel = (payload.current_label || "").trim() || null;
    state.currentProjectGuard = currentGuard;
    state.currentProjectLabel = currentLabel;
    state.projects = Array.isArray(payload.projects) ? payload.projects : [];

    if (!state.includeTempProjects) {
      const others = state.projects
        .filter((project) => project && project.project_guard)
        .filter((project) => project.store_present !== false)
        .filter((project) => (currentGuard ? project.project_guard !== currentGuard : true));
      const hasNonTemp = others.some((project) => !project.is_temp);
      const hasTemp = others.some((project) => !!project.is_temp);
      if (!hasNonTemp && hasTemp) {
        state.includeTempProjects = true;
      }
    }

    let stored = null;
    try {
      stored = localStorage.getItem(PROJECT_STORAGE_KEY);
    } catch {
      stored = null;
    }

    const candidate = (stored || "").trim() || null;
    const match = candidate
      ? state.projects.find((project) => project && project.project_guard === candidate)
      : undefined;
    const isKnown = !!(
      match &&
      match.store_present !== false &&
      (state.includeTempProjects || !match.is_temp) &&
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
    const payload = await fetchJson(workspaceUrl("/api/about"));
    const recommended = (payload.workspace_recommended || "").trim();
    state.workspaceRecommended = recommended ? recommended : null;
  } catch {
    state.workspaceRecommended = null;
  }
}

async function loadWorkspaces() {
  try {
    const payload = await fetchJson(workspaceUrl("/api/workspaces"));
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
        (state.workspacesDefault &&
          candidates.includes(state.workspacesDefault) &&
          state.workspacesDefault) ||
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
  shell: document.querySelector(".shell"),
  project: document.getElementById("project"),
  lens: document.getElementById("lens"),
  workspace: document.getElementById("workspace"),
  updated: document.getElementById("updated"),
  runnerStatus: document.getElementById("runner-status"),
  runnerJobs: document.getElementById("runner-jobs"),
  runnerAutostart: document.getElementById("runner-autostart"),
  focus: document.getElementById("focus"),
  focusSub: document.getElementById("focus-sub"),
  summaryPlanLabel: document.getElementById("summary-plan-label"),
  planCount: document.getElementById("plan-count"),
  planBreakdown: document.getElementById("plan-breakdown"),
  summaryTaskLabel: document.getElementById("summary-task-label"),
  taskCount: document.getElementById("task-count"),
  taskBreakdown: document.getElementById("task-breakdown"),
  goalsHeading: document.getElementById("goals-heading"),
  goalsSubheading: document.getElementById("goals-subheading"),
  planHeading: document.getElementById("plan-heading"),
  planSubheading: document.getElementById("plan-subheading"),
  tasksHeading: document.getElementById("tasks-heading"),
  tasksSubheading: document.getElementById("tasks-subheading"),
  goalList: document.getElementById("goal-list"),
  planChecklist: document.getElementById("plan-checklist"),
  taskList: document.getElementById("task-list"),
  graph: document.getElementById("graph"),
  minimap: document.getElementById("minimap"),
  hud: document.getElementById("hud"),
  hudWhere: document.getElementById("hud-where"),
  hudLod: document.getElementById("hud-lod"),
  hudFocus: document.getElementById("hud-focus"),
  hudSelected: document.getElementById("hud-selected"),
  hudLegend: document.getElementById("hud-legend"),
  hudWarning: document.getElementById("hud-warning"),
  graphControls: document.getElementById("graph-controls"),
  graphSearch: document.getElementById("graph-search"),
  btnBack: document.getElementById("btn-back"),
  btnForward: document.getElementById("btn-forward"),
  btnHome: document.getElementById("btn-home"),
  btnFit: document.getElementById("btn-fit"),
  btnFocus: document.getElementById("btn-focus"),
  btnZoomIn: document.getElementById("btn-zoom-in"),
  btnZoomOut: document.getElementById("btn-zoom-out"),
  btnRefresh: document.getElementById("btn-refresh"),
  btnExplorer: document.getElementById("btn-explorer"),
  sidebarPanel: document.getElementById("sidebar-panel"),
  sidebarClose: document.getElementById("sidebar-close"),
  sidebarPin: document.getElementById("sidebar-pin"),
  detailPanel: document.getElementById("detail-panel"),
  detailKicker: document.getElementById("detail-kicker"),
  detailTitle: document.getElementById("detail-title"),
  detailMeta: document.getElementById("detail-meta"),
  detailBody: document.getElementById("detail-body"),
  detailClose: document.getElementById("detail-close"),
  detailPin: document.getElementById("detail-pin"),
  palette: document.getElementById("palette"),
  palettePanel: document.querySelector(".palette-panel"),
  paletteInput: document.getElementById("palette-input"),
  paletteResults: document.getElementById("palette-results"),
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
    .filter((project) => project && project.project_guard)
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

  const labelCounts = new Map();
  others.forEach((project) => {
    const label = ((project.label || project.project_guard || "") + "").trim() || "project";
    labelCounts.set(label, (labelCounts.get(label) || 0) + 1);
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

function renderWorkspaceSelect(selectedWorkspace) {
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

  const pinnedOrder = [];
  const pinnedRank = new Map();
  const pushPinned = (value) => {
    const key = ((value || "") + "").trim();
    if (!key) return;
    if (pinnedRank.has(key)) return;
    pinnedRank.set(key, pinnedOrder.length);
    pinnedOrder.push(key);
  };

  pushPinned(selected);
  pushPinned(state.workspacesDefault);
  pushPinned(state.workspaceRecommended);
  (state.projects || []).forEach((project) => {
    pushPinned(((project && project.workspace_recommended) || "").trim());
    pushPinned(((project && project.workspace_default) || "").trim());
  });

  const entries = (state.workspaces || [])
    .filter((entry) => entry && entry.workspace)
    .slice()
    .sort((a, b) => {
      const aw = ((a.workspace || "") + "").trim();
      const bw = ((b.workspace || "") + "").trim();
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

function clear(element) {
  while (element.firstChild) element.removeChild(element.firstChild);
}

function formatStatus(status) {
  const normalized = ((status || "") + "").trim().toUpperCase();
  switch (normalized) {
    case "ACTIVE":
      return "в работе";
    case "TODO":
      return "в очереди";
    case "PARKED":
      return "отложено";
    case "DONE":
      return "сделано";
    case "BLOCKED":
      return "заблокировано";
    case "LOW":
      return "низкий";
    case "MEDIUM":
      return "средний";
    case "HIGH":
      return "высокий";
    default:
      return normalized ? normalized.replace(/_/g, " ").toLowerCase() : "-";
  }
}

function formatCount(label, value) {
  return `${label} ${value}`;
}

function formatRunnerStatus(status) {
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

function formatDate(valueMs) {
  if (!valueMs) return "-";
  const date = new Date(valueMs);
  return date.toISOString().replace("T", " ").replace("Z", " UTC");
}

function formatApiError(error) {
  if (!error) return "Unable to load details.";
  if (typeof error === "string") return error;
  if (error.message) return error.message;
  return "Unable to load details.";
}

function startDetailLoad(kind, id) {
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

function isCurrentDetail(token) {
  return state.detailSelection && state.detailSelection.token === token;
}

async function fetchJson(path) {
  const response = await fetchWithTimeout(path, { cache: "no-store" }, 7000);
  const text = await response.text();
  let payload = null;
  try {
    payload = JSON.parse(text);
  } catch (err) {
    throw new Error(`Invalid JSON response for ${path}`);
  }
  if (!response.ok) {
    const message = payload?.error?.message || `Request failed (${response.status})`;
    const recovery = payload?.error?.recovery ? ` ${payload.error.recovery}` : "";
    throw new Error(`${message}${recovery}`.trim());
  }
  return payload;
}

function toUserError(err) {
  try {
    const message =
      err && typeof err === "object" && "message" in err ? err.message : err;
    const text = ((message || "Request failed") + "").trim() || "Request failed";
    return text.length > 160 ? `${text.slice(0, 157)}…` : text;
  } catch {
    return "Request failed";
  }
}

async function fetchGraphPlan(planId, opts) {
  const id = (planId || "").trim();
  if (!id) throw new Error("plan_id: missing");
  const params = { lens: "work" };
  if (opts && typeof opts === "object") {
    if (opts.cursor !== null && opts.cursor !== undefined) params.cursor = opts.cursor;
    if (typeof opts.limit === "number") params.limit = opts.limit;
  }
  const url = workspaceUrlWithParams(`/api/graph/plan/${id}`, params);
  return await fetchJson(url);
}

async function fetchGraphCluster(clusterId, opts) {
  const id = (clusterId || "").trim();
  if (!id) throw new Error("cluster_id: missing");
  const params = { lens: "work" };
  if (opts && typeof opts === "object") {
    if (opts.cursor !== null && opts.cursor !== undefined) params.cursor = opts.cursor;
    if (typeof opts.limit === "number") params.limit = opts.limit;
  }
  const url = workspaceUrlWithParams(`/api/graph/cluster/${id}`, params);
  return await fetchJson(url);
}

async function fetchGraphLocal(nodeId, opts) {
  const id = (nodeId || "").trim();
  if (!id) throw new Error("node_id: missing");
  const params = { lens: "work" };
  if (opts && typeof opts === "object") {
    if (opts.cursor !== null && opts.cursor !== undefined) params.cursor = opts.cursor;
    if (typeof opts.limit === "number") params.limit = opts.limit;
    if (typeof opts.hops === "number") params.hops = clamp(opts.hops, 1, 2);
  }
  const url = workspaceUrlWithParams(`/api/graph/local/${id}`, params);
  return await fetchJson(url);
}

async function postJson(path, body) {
  const response = await fetchWithTimeout(
    path,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      cache: "no-store",
    },
    7000
  );
  const text = await response.text();
  let payload = null;
  try {
    payload = JSON.parse(text);
  } catch (err) {
    throw new Error(`Invalid JSON response for ${path}`);
  }
  if (!response.ok) {
    const message = payload?.error?.message || `Request failed (${response.status})`;
    const recovery = payload?.error?.recovery ? ` ${payload.error.recovery}` : "";
    throw new Error(`${message}${recovery}`.trim());
  }
  return payload;
}

async function fetchWithTimeout(path, options, timeoutMs) {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(path, { ...options, signal: controller.signal });
  } finally {
    window.clearTimeout(timeout);
  }
}

function setDetailVisible(open, opts) {
  if (!nodes.detailPanel) return;
  const desired = !!open;
  windowUi.detail.open = desired;
  if (desired) {
    windowUi.detail.z = bumpWindowZ();
  }
  syncDetailWindowDom({ persist: !(opts && opts.persist === false), clamp: !(opts && opts.clamp === false) });
  const focus = !(opts && opts.focus === false);
  if (focus) {
    window.setTimeout(() => {
      if (desired) {
        nodes.detailClose?.focus?.();
      } else {
        nodes.btnFocus?.focus?.();
      }
    }, 0);
  }
}

function sidebarIsOpen() {
  return !!nodes.sidebarPanel?.classList.contains("is-open");
}

function setSidebarVisible(open, opts) {
  const desired = !!open;
  windowUi.explorer.open = desired;
  if (desired) {
    windowUi.explorer.z = bumpWindowZ();
  }
  syncExplorerWindowDom({ persist: !(opts && opts.persist === false), clamp: !(opts && opts.clamp === false) });

  const focus = !(opts && typeof opts === "object" && opts.focus === false);
  if (focus) {
    window.setTimeout(() => {
      if (desired) {
        nodes.sidebarClose?.focus?.();
      } else {
        nodes.btnExplorer?.focus?.();
      }
    }, 0);
  }
}

function detailIsOpen() {
  return !!nodes.detailPanel?.classList.contains("is-open");
}

function explorerIsPinned() {
  return !!nodes.sidebarPanel?.classList.contains("is-pinned");
}

function detailIsPinned() {
  return !!nodes.detailPanel?.classList.contains("is-pinned");
}

function setSidebarPinned(pinned, opts) {
  windowUi.explorer.pinned = !!pinned;
  if (windowUi.explorer.pinned) {
    windowUi.explorer.open = true;
  }
  syncExplorerWindowDom({ persist: !(opts && opts.persist === false), clamp: true });
}

function setDetailPinned(pinned, opts) {
  windowUi.detail.pinned = !!pinned;
  if (windowUi.detail.pinned) {
    windowUi.detail.open = true;
  }
  syncDetailWindowDom({ persist: !(opts && opts.persist === false), clamp: true });
}

function closeExplorerWindow(opts) {
  setSidebarPinned(false, { persist: false });
  setSidebarVisible(false, opts);
}

function closeDetailWindow(opts) {
  setDetailPinned(false, { persist: false });
  setDetailVisible(false, opts);
}

function persistExplorerWindowState() {
  try {
    localStorage.setItem(
      sidebarOpenStorageKey(),
      JSON.stringify({
        v: 1,
        open: !!windowUi.explorer.open,
        pinned: !!windowUi.explorer.pinned,
        anchor: windowUi.explorer.anchor,
        x: Math.round(windowUi.explorer.x || 0),
        y: Math.round(windowUi.explorer.y || 0),
        z: Math.round(windowUi.explorer.z || 0),
      })
    );
  } catch {
    // ignore storage failures
  }
}

function persistDetailWindowState() {
  try {
    localStorage.setItem(
      detailWindowStorageKey(),
      JSON.stringify({
        v: 1,
        open: !!windowUi.detail.open,
        pinned: !!windowUi.detail.pinned,
        anchor: windowUi.detail.anchor,
        x: Math.round(windowUi.detail.x || 0),
        y: Math.round(windowUi.detail.y || 0),
        z: Math.round(windowUi.detail.z || 0),
      })
    );
  } catch {
    // ignore storage failures
  }
}

function clampWindowToViewport(el, win) {
  if (!el) return win;
  const rect = el.getBoundingClientRect();
  const margin = 12;

  const maxY = Math.max(margin, window.innerHeight - rect.height - margin);
  const nextY = clamp(win.y, margin, maxY);

  if (win.anchor === "right") {
    const maxX = Math.max(margin, window.innerWidth - rect.width - margin);
    const nextX = clamp(win.x, margin, maxX);
    return { ...win, x: nextX, y: nextY };
  }
  const maxX = Math.max(margin, window.innerWidth - rect.width - margin);
  const nextX = clamp(win.x, margin, maxX);
  return { ...win, x: nextX, y: nextY };
}

function applyWindowGeometry(el, win) {
  if (!el) return;
  const x = typeof win.x === "number" ? win.x : 0;
  const y = typeof win.y === "number" ? win.y : 0;
  el.style.top = `${Math.round(y)}px`;
  if (win.anchor === "right") {
    el.style.right = `${Math.round(x)}px`;
    el.style.left = "auto";
  } else {
    el.style.left = `${Math.round(x)}px`;
    el.style.right = "auto";
  }
  if (typeof win.z === "number") {
    el.style.zIndex = `${Math.round(win.z)}`;
  }
}

function syncExplorerWindowDom(opts) {
  if (!nodes.sidebarPanel) return;
  if (opts && opts.clamp) {
    windowUi.explorer = clampWindowToViewport(nodes.sidebarPanel, windowUi.explorer);
  }
  nodes.sidebarPanel.classList.toggle("is-open", !!windowUi.explorer.open);
  nodes.sidebarPanel.classList.toggle("is-pinned", !!windowUi.explorer.pinned);
  nodes.sidebarPanel.setAttribute("aria-hidden", windowUi.explorer.open ? "false" : "true");
  applyWindowGeometry(nodes.sidebarPanel, windowUi.explorer);

  if (nodes.sidebarPin) {
    const label = windowUi.explorer.pinned ? "Unpin" : "Pin";
    nodes.sidebarPin.textContent = label;
    nodes.sidebarPin.setAttribute("aria-pressed", windowUi.explorer.pinned ? "true" : "false");
    nodes.sidebarPin.setAttribute("aria-label", label);
    nodes.sidebarPin.setAttribute("title", label);
  }

  if (opts && opts.persist) {
    persistExplorerWindowState();
  }
}

function syncDetailWindowDom(opts) {
  if (!nodes.detailPanel) return;
  if (opts && opts.clamp) {
    windowUi.detail = clampWindowToViewport(nodes.detailPanel, windowUi.detail);
  }
  nodes.detailPanel.classList.toggle("is-open", !!windowUi.detail.open);
  nodes.detailPanel.classList.toggle("is-pinned", !!windowUi.detail.pinned);
  const ariaHidden = paletteIsOpen() ? "true" : windowUi.detail.open ? "false" : "true";
  nodes.detailPanel.setAttribute("aria-hidden", ariaHidden);
  applyWindowGeometry(nodes.detailPanel, windowUi.detail);

  if (nodes.detailPin) {
    const label = windowUi.detail.pinned ? "Unpin" : "Pin";
    nodes.detailPin.textContent = label;
    nodes.detailPin.setAttribute("aria-pressed", windowUi.detail.pinned ? "true" : "false");
    nodes.detailPin.setAttribute("aria-label", label);
    nodes.detailPin.setAttribute("title", label);
  }

  if (opts && opts.persist) {
    persistDetailWindowState();
  }
}

function bumpWindowZ() {
  windowUi.zCounter += 1;
  if (windowUi.zCounter > 900) {
    windowUi.zCounter = 80;
    windowUi.explorer.z = 60;
    windowUi.detail.z = 70;
  }
  return windowUi.zCounter;
}

function bringWindowToFront(kind) {
  if (kind === "explorer") {
    windowUi.explorer.z = bumpWindowZ();
    syncExplorerWindowDom({ persist: false, clamp: false });
    return;
  }
  if (kind === "detail") {
    windowUi.detail.z = bumpWindowZ();
    syncDetailWindowDom({ persist: false, clamp: false });
  }
}

function isInteractiveElement(target) {
  if (!target || typeof target.closest !== "function") return false;
  return !!target.closest("button, input, select, textarea, a, [role='button']");
}

function startWindowDrag(kind, el, event) {
  if (!el || !event) return;
  if (event.button !== 0) return;
  const target = event.target;
  if (isInteractiveElement(target)) return;

  const rect = el.getBoundingClientRect();
  const win = kind === "explorer" ? windowUi.explorer : windowUi.detail;

  windowUi.dragging = {
    kind,
    pointerId: event.pointerId,
    startClientX: event.clientX,
    startClientY: event.clientY,
    startAnchor: win.anchor,
    startX: win.x,
    startY: win.y,
    width: rect.width,
    height: rect.height,
  };

  try {
    el.setPointerCapture(event.pointerId);
  } catch {
    // ignore
  }
  event.preventDefault();
}

function handleWindowDragMove(event) {
  const drag = windowUi.dragging;
  if (!drag || !event) return;
  if (drag.pointerId !== event.pointerId) return;

  const dx = event.clientX - drag.startClientX;
  const dy = event.clientY - drag.startClientY;
  const margin = 12;

  if (drag.kind === "explorer") {
    const maxX = Math.max(margin, window.innerWidth - drag.width - margin);
    const maxY = Math.max(margin, window.innerHeight - drag.height - margin);
    windowUi.explorer.anchor = "left";
    windowUi.explorer.x = clamp(drag.startX + dx, margin, maxX);
    windowUi.explorer.y = clamp(drag.startY + dy, margin, maxY);
    syncExplorerWindowDom({ persist: false, clamp: false });
    return;
  }

  const maxY = Math.max(margin, window.innerHeight - drag.height - margin);
  windowUi.detail.y = clamp(drag.startY + dy, margin, maxY);

  if (drag.startAnchor === "right") {
    const maxX = Math.max(margin, window.innerWidth - drag.width - margin);
    windowUi.detail.anchor = "right";
    windowUi.detail.x = clamp(drag.startX - dx, margin, maxX);
  } else {
    const maxX = Math.max(margin, window.innerWidth - drag.width - margin);
    windowUi.detail.anchor = "left";
    windowUi.detail.x = clamp(drag.startX + dx, margin, maxX);
  }

  syncDetailWindowDom({ persist: false, clamp: false });
}

function handleWindowDragEnd(event) {
  const drag = windowUi.dragging;
  if (!drag || !event) return;
  if (drag.pointerId !== event.pointerId) return;
  windowUi.dragging = null;

  if (drag.kind === "explorer") {
    syncExplorerWindowDom({ persist: true, clamp: true });
    return;
  }
  syncDetailWindowDom({ persist: true, clamp: true });
}

function renderDetailMeta(lines) {
  clear(nodes.detailMeta);
  lines.forEach((line) => {
    const row = document.createElement("div");
    row.textContent = line;
    nodes.detailMeta.append(row);
  });
}

function renderDetailSection(title, content) {
  const section = document.createElement("div");
  const header = document.createElement("div");
  header.className = "detail-section-title";
  header.textContent = title;
  section.append(header, ...content);
  return section;
}

function renderDetailText(value, emptyMessage) {
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

function chooseDerivedPlanText(snapshot, plan) {
  const planText = (plan.description || "").trim();
  if (planText) {
    return { text: planText, fromTask: null };
  }
  const checklist =
    snapshot.plan_checklists?.[plan.id] ||
    (snapshot.plan_checklist?.plan_id === plan.id ? snapshot.plan_checklist : null);
  if (checklist && Array.isArray(checklist.steps) && checklist.steps.length > 0) {
    const max = Math.min(24, checklist.steps.length);
    const lines = checklist.steps.slice(0, max).map((step, index) => `${index + 1}. ${step}`);
    const suffix = checklist.steps.length > max ? `\n… +${checklist.steps.length - max} more` : "";
    return { text: `${lines.join("\n")}${suffix}`, fromTask: null };
  }
  const directTaskId = plan.id.startsWith("PLAN-") ? plan.id.replace("PLAN-", "TASK-") : null;
  if (directTaskId) {
    const direct = snapshot.tasks.find(
      (task) => task.id === directTaskId && (task.description || "").trim().length > 0
    );
    if (direct) {
      return { text: (direct.description || "").trim(), fromTask: direct };
    }
  }
  const candidates = snapshot.tasks
    .filter((task) => task.plan_id === plan.id)
    .filter((task) => (task.description || "").trim().length > 0);
  if (candidates.length === 0) {
    return { text: "", fromTask: null };
  }
  const statusRank = (status) => {
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
  return { text: (fromTask.description || "").trim(), fromTask };
}

function appendDocTail(activity, tail, source) {
  if (!activity || !tail || !Array.isArray(tail.entries)) return;
  tail.entries.forEach((entry) => {
    const seq = entry?.seq ?? null;
    if (seq === null || seq === undefined) return;
    const key = `${source}:${seq}`;
    if (activity.seen.has(key)) return;
    activity.seen.add(key);
    activity.entries.push({ ...entry, source });
  });
}

function updateActivityState(activity, payload) {
  if (!activity || !payload) return;
  const trace = payload.trace_tail || null;
  const notes = payload.notes_tail || null;
  appendDocTail(activity, trace, "trace");
  appendDocTail(activity, notes, "notes");

  activity.trace_has_more = !!trace?.has_more;
  activity.notes_has_more = !!notes?.has_more;
  activity.trace_cursor = activity.trace_has_more ? trace?.next_cursor ?? null : 0;
  activity.notes_cursor = activity.notes_has_more ? notes?.next_cursor ?? null : 0;

  activity.entries.sort((a, b) => {
    const ts = (b.ts_ms || 0) - (a.ts_ms || 0);
    if (ts !== 0) return ts;
    return (b.seq || 0) - (a.seq || 0);
  });
}

function renderLoadMoreButton(label, onClick, disabled) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "list-item";
  btn.style.transform = "none";
  btn.textContent = label;
  btn.disabled = !!disabled;
  btn.addEventListener("click", onClick);
  return btn;
}

function renderTaskListButton(task, sourceSnapshot, opts) {
  const options = opts && typeof opts === "object" ? opts : {};
  const includeSnippet = !!options.includeSnippet;

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "list-item";
  btn.style.transform = "none";

  const title = document.createElement("div");
  title.className = "item-title";
  title.textContent = task.title || task.id;

  const nodes = [title];

  if (includeSnippet) {
    const snippetText = (task.description || "").trim();
    const snippet = document.createElement("div");
    snippet.className = "detail-snippet";
    snippet.textContent = snippetText
      ? snippetText.length > 220
        ? `${snippetText.slice(0, 220)}…`
        : snippetText
      : "No task description.";
    nodes.push(snippet);
  }

  const meta = document.createElement("div");
  meta.className = "item-meta";

  const id = document.createElement("span");
  id.className = "badge dim";
  id.textContent = task.id;

  const status = document.createElement("span");
  status.className = "badge accent";
  status.textContent = formatStatus(task.status);
  meta.append(id, status);

  nodes.push(meta);

  btn.append(...nodes);
  btn.addEventListener("click", () => renderTaskDetail(state.snapshot || sourceSnapshot, task));
  return btn;
}

function renderDocEntry(entry) {
  const row = document.createElement("div");
  row.className = "list-item detail-entry";

  const header = document.createElement("div");
  header.className = "detail-entry-head";

  const title = document.createElement("div");
  title.className = "detail-entry-title";
  title.textContent =
    (entry.title || "").trim() ||
    (entry.event_type || "").trim() ||
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

  const bodyText = (entry.content || "").trim();
  const body = document.createElement("div");
  body.className = "detail-entry-body";
  body.textContent = bodyText || "No content.";

  row.append(header, body);
  return row;
}

function renderDocEntries(entries) {
  if (!entries || entries.length === 0) {
    return [renderDetailText(null, "No recent notes yet.")];
  }
  return entries.map(renderDocEntry);
}

function renderStepEntry(step) {
  const row = document.createElement("div");
  row.className = "list-item step-item";
  row.style.transform = "none";

  const depth = Math.max(0, (step.path || "").split(".").length - 1);
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

  const reason = (step.block_reason || "").trim();
  if (reason) {
    const snippet = document.createElement("div");
    snippet.className = "detail-snippet";
    snippet.textContent = reason;
    content.append(snippet);
  }

  row.append(content);
  return row;
}

function renderStepsBlock(steps) {
  const items = steps?.items || [];
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

function renderAnchorDetail(snapshot, anchor) {
  const token = startDetailLoad("plan", anchor.id);
  void token;
  nodes.detailKicker.textContent = "Anchor";
  nodes.detailTitle.textContent = anchor.title || anchor.id;

  const total = typeof anchor?.task_counts?.total === "number" ? anchor.task_counts.total : 0;
  renderDetailMeta([
    `ID: ${anchor.id}`,
    `Kind: ${(anchor.kind || anchor.priority || "anchor").toString()}`,
    `Status: ${(anchor.status || "").toString() || "—"}`,
    `Updated: ${formatDate(anchor.updated_at_ms)}`,
    `Keys: ${total}`,
  ]);

  clear(nodes.detailBody);
  nodes.detailBody.append(
    renderDetailSection("Anchor", [
      renderDetailText(anchor.description, "No anchor description yet."),
    ])
  );

  const depends = Array.isArray(anchor.depends_on) ? anchor.depends_on : [];
  if (depends.length) {
    const items = depends.slice(0, 32).map((dep) => {
      const id = (dep || "").trim();
      const row = document.createElement("div");
      row.className = "list-item";
      row.textContent = id || "(invalid)";
      row.addEventListener("click", () => {
        if (!id) return;
        state.selectedPlanId = id;
        render(snapshot);
        const next = (snapshot.plans || []).find((p) => p && p.id === id);
        if (next) {
          renderAnchorDetail(snapshot, next);
        }
      });
      return row;
    });
    nodes.detailBody.append(renderDetailSection("Depends on", items));
  }

  setDetailVisible(true);
}

function renderPlanDetail(snapshot, plan) {
  if (normalizeLens(snapshot?.lens || state.lens) === "knowledge") {
    renderAnchorDetail(snapshot, plan);
    return;
  }

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

  const sections = [];
  const derived = chooseDerivedPlanText(snapshot, plan);
  const planTextBlock = renderDetailText(derived.text || null, "No plan text yet.");
  const planExtras = [planTextBlock];
  if (derived.fromTask) {
    const caption = document.createElement("div");
    caption.className = "detail-caption";
    caption.textContent = "Derived from ";
    const link = document.createElement("button");
    link.type = "button";
    link.className = "detail-link";
    link.textContent = derived.fromTask.id;
    link.addEventListener("click", () => renderTaskDetail(snapshot, derived.fromTask));
    caption.append(link);
    planExtras.push(caption);
  }
  sections.push(renderDetailSection("Plan", planExtras));
  const ctx = (plan.context || "").trim();
  if (ctx) {
    sections.push(renderDetailSection("Context", [renderDetailText(ctx, "No context.")]));
  }
  const checklist =
    snapshot.plan_checklists?.[plan.id] ||
    (snapshot.plan_checklist?.plan_id === plan.id ? snapshot.plan_checklist : null);
  if (checklist) {
    const steps = checklist.steps || [];
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

  const tasksInSnapshot = (snapshot.tasks || []).filter((task) => task.plan_id === plan.id);
  const tasksTotal = typeof plan?.task_counts?.total === "number" ? plan.task_counts.total : 0;
  const tasksCap = Math.min(Math.max(tasksTotal, tasksInSnapshot.length), GRAPH_LIMITS.maxTasksInPlan);

  if (tasksTotal > 0 || tasksInSnapshot.length > 0) {
    const selection = state.detailSelection;
    if (selection) {
      selection.graphTasksPager = {
        kind: "plan",
        id: plan.id,
        cursor: null,
        has_more: true,
        loading: false,
        last_error: null,
        started: false,
        limit: 200,
      };
    }

    const caption = document.createElement("div");
    caption.className = "detail-caption";

    const listHost = document.createElement("div");
    listHost.className = "detail-list";

    const loadBtn = renderLoadMoreButton(
      "Load more tasks",
      () => {
        void loadMore(false);
      },
      false
    );

    function loadedCount(snapshotNow) {
      return (snapshotNow.tasks || []).filter((task) => task.plan_id === plan.id).length;
    }

    function renderList() {
      const snapshotNow = state.snapshot || snapshot;
      const current = state.detailSelection;
      const pager = current?.graphTasksPager;
      const started = !!pager?.started;

      const tasks = started
        ? (snapshotNow.tasks || [])
            .filter((task) => task.plan_id === plan.id)
            .slice()
            .sort((a, b) => (a.id || "").localeCompare(b.id || ""))
            .slice(0, tasksCap || GRAPH_LIMITS.maxTasksInPlan)
        : (snapshotNow.tasks || [])
            .filter((task) => task.plan_id === plan.id)
            .slice()
            .sort(
              (a, b) =>
                statusRank(a.status) - statusRank(b.status) ||
                (b.updated_at_ms || 0) - (a.updated_at_ms || 0) ||
                (a.id || "").localeCompare(b.id || "")
            )
            .slice(0, 10);

      clear(listHost);
      if (!tasks.length) {
        listHost.append(
          renderDetailText(null, started ? "No tasks loaded yet." : "No tasks in this snapshot.")
        );
      } else {
        tasks.forEach((task) =>
          listHost.append(renderTaskListButton(task, snapshotNow, { includeSnippet: true }))
        );
      }
      listHost.append(loadBtn);
    }

    function updateControls(snapshotNow) {
      const current = state.detailSelection;
      const pager = current?.graphTasksPager;
      if (!pager || pager.kind !== "plan" || pager.id !== plan.id) return;

      const loaded = loadedCount(snapshotNow);
      const capNow = tasksCap;
      const baseCaption = capNow ? `Loaded: ${loaded}/${capNow}` : "No tasks.";
      caption.textContent = pager.last_error ? `${baseCaption} — ${pager.last_error}` : baseCaption;

      const capReached = capNow > 0 && loaded >= capNow;
      const canLoadMore = !capReached && pager.has_more;
      loadBtn.style.display = canLoadMore ? "" : "none";
      loadBtn.disabled = !!pager.loading;
      loadBtn.textContent = pager.loading
        ? "Loading…"
        : pager.last_error
          ? "Retry loading tasks"
          : "Load more tasks";
    }

    async function loadMore(prefetch) {
      if (!isCurrentDetail(token)) return;
      const current = state.detailSelection;
      const pager = current?.graphTasksPager;
      if (!pager || pager.kind !== "plan" || pager.id !== plan.id) return;
      if (pager.loading) return;

      const snapshotNow = state.snapshot || snapshot;
      const loadedNow = loadedCount(snapshotNow);
      if (tasksCap === 0 || loadedNow >= tasksCap) {
        pager.has_more = false;
        pager.started = true;
        renderList();
        updateControls(snapshotNow);
        return;
      }

      if (!pager.has_more && !prefetch) return;

      pager.loading = true;
      pager.last_error = null;
      updateControls(snapshotNow);
      try {
        const payload = await fetchGraphPlan(plan.id, { cursor: pager.cursor, limit: pager.limit });
        if (!isCurrentDetail(token)) return;
        ingestGraphOverlay(payload);
        const merged = mergeGraphOverlay(state.snapshot || snapshotNow);
        render(merged);
        pager.started = true;

        const pagination = payload?.pagination || {};
        const hasMore = !!pagination.has_more;
        const nextCursor = pagination.next_cursor ?? null;
        if (hasMore && (nextCursor === null || nextCursor === "")) {
          pager.last_error = "Paging stalled: server returned has_more without next_cursor.";
          pager.cursor = null;
          pager.has_more = false;
          return;
        }
        pager.cursor = nextCursor;
        pager.has_more = hasMore;
      } catch (err) {
        if (isCurrentDetail(token)) {
          pager.last_error = toUserError(err);
        }
      } finally {
        if (isCurrentDetail(token)) {
          const latest = state.detailSelection?.graphTasksPager;
          if (latest && latest.kind === "plan" && latest.id === plan.id) {
            latest.loading = false;
          }
        }
      }

      if (!isCurrentDetail(token)) return;
      const snapshotFinal = state.snapshot || snapshot;
      renderList();
      updateControls(snapshotFinal);
    }

    renderList();
    updateControls(snapshot);
    sections.push(renderDetailSection("Tasks", [caption, listHost]));

    if (tasksInSnapshot.length === 0 && tasksCap > 0) {
      void loadMore(true);
    }
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

function renderKnowledgeKeyDetail(snapshot, task) {
  const token = startDetailLoad("task", task.id);
  void token;
  nodes.detailKicker.textContent = "Knowledge";
  nodes.detailTitle.textContent = task.title || task.key || task.id;

  const anchor = (snapshot.plans || []).find((p) => p && p.id === task.plan_id) || null;
  const cardId = (task.card_id || task.context || "").toString().trim() || null;

  renderDetailMeta([
    `Key: ${(task.key || task.title || "").toString() || "—"}`,
    `Anchor: ${anchor?.title || task.plan_id || "—"}`,
    `card_id: ${cardId || "—"}`,
    `Updated: ${formatDate(task.updated_at_ms)}`,
  ]);

  clear(nodes.detailBody);
  const hints = [];
  if (cardId) {
    hints.push(
      renderDetailText(
        `This key maps to card_id=${cardId} (open via BranchMind tools).`,
        ""
      )
    );
  } else {
    hints.push(renderDetailText(null, "No card id for this key."));
  }
  nodes.detailBody.append(renderDetailSection("TL;DR", hints));
  setDetailVisible(true);
}

function renderTaskDetail(snapshot, task) {
  if (normalizeLens(snapshot?.lens || state.lens) === "knowledge") {
    renderKnowledgeKeyDetail(snapshot, task);
    return;
  }

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
    renderDetailSection("Task", [renderDetailText(task.description, "No task description yet.")])
  );
  const ctx = (task.context || "").trim();
  if (ctx) {
    nodes.detailBody.append(
      renderDetailSection("Context", [renderDetailText(ctx, "No context.")])
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

async function loadPlanExtras(planId, token, activityHost) {
  try {
    const payload = await fetchJson(workspaceUrl(`/api/plan/${planId}`));
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
    activityHost.append(renderDetailText(null, formatApiError(err?.message)));
  }
}

async function loadTaskExtras(taskId, token, stepsHost, activityHost) {
  try {
    const payload = await fetchJson(workspaceUrl(`/api/task/${taskId}`));
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
    activityHost.append(renderDetailText(null, formatApiError(err?.message)));
  }
}

function renderActivityHost(activityHost, entityId, token, kind) {
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
        trace_cursor: activityNow.trace_has_more ? activityNow.trace_cursor || 0 : 0,
        notes_cursor: activityNow.notes_has_more ? activityNow.notes_cursor || 0 : 0,
      };
      const url =
        kind === "task"
          ? workspaceUrlWithParams(`/api/task/${entityId}`, params)
          : workspaceUrlWithParams(`/api/plan/${entityId}`, params);

      try {
        const payload = await fetchJson(url);
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

function openDetailFromNode(snapshot, node) {
  if (node.kind === "plan") {
    const plan = snapshot.plans.find((item) => item.id === node.id);
    if (plan) renderPlanDetail(snapshot, plan);
    return;
  }
  const task = snapshot.tasks.find((item) => item.id === node.id);
  if (task) renderTaskDetail(snapshot, task);
}

function sumTaskCounts(plans) {
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

function applyLensCopy() {
  const lens = normalizeLens(state.lens);
  const isKnowledge = lens === "knowledge";

  if (nodes.summaryPlanLabel) {
    nodes.summaryPlanLabel.textContent = isKnowledge ? "Anchors" : "Plans";
  }
  if (nodes.summaryTaskLabel) {
    nodes.summaryTaskLabel.textContent = isKnowledge ? "Knowledge" : "Tasks";
  }
  if (nodes.goalsHeading) {
    nodes.goalsHeading.textContent = isKnowledge ? "Anchors" : "Goals";
  }
  if (nodes.goalsSubheading) {
    nodes.goalsSubheading.textContent = isKnowledge
      ? "Meaning map anchors (a:...)."
      : "Active plans at a glance.";
  }
  if (nodes.planHeading) {
    nodes.planHeading.textContent = isKnowledge ? "Anchor" : "Plan";
  }
  if (nodes.planSubheading) {
    nodes.planSubheading.textContent = isKnowledge ? "Selected anchor details." : "Selected goal checklist.";
  }
  if (nodes.tasksHeading) {
    nodes.tasksHeading.textContent = isKnowledge ? "Knowledge" : "Tasks";
  }
  if (nodes.tasksSubheading) {
    nodes.tasksSubheading.textContent = isKnowledge
      ? "Knowledge keys for the selected anchor."
      : "Workstream for the selected goal.";
  }
  if (nodes.hudLegend) {
    nodes.hudLegend.textContent = isKnowledge
      ? "Якорь=материк, ключ=точка."
      : "Цель=материк, кластер=город, задача=точка.";
  }
  if (nodes.graphSearch) {
    nodes.graphSearch.placeholder = isKnowledge
      ? "Поиск: a:viewer, ключ, текст…"
      : "Поиск: PLAN-123, TASK-456, текст…";
  }
  if (nodes.paletteInput) {
    nodes.paletteInput.placeholder = isKnowledge
      ? "Ctrl+K: прыгнуть к… (a:viewer, ключ, текст)"
      : "Ctrl+K: прыгнуть к… (PLAN-123, TASK-456, текст)";
  }
}

function renderSummary(snapshot) {
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
    const bits = [];
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
    const host = nodes.runnerAutostart.closest(".toggle");
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

  const plansShown = snapshot.plans.length;
  const plansTotal = Number.isFinite(snapshot.plans_total) ? snapshot.plans_total : plansShown;
  nodes.planCount.textContent = plansTotal.toString();
  const activePlans = snapshot.plans.filter((plan) => plan.status !== "DONE").length;
  nodes.planBreakdown.textContent = snapshot.truncated?.plans && plansTotal > plansShown
    ? `${activePlans} active (shown)`
    : `${activePlans} active`;

  const counts = sumTaskCounts(snapshot.plans);
  const tasksTotal = Number.isFinite(snapshot.tasks_total) ? snapshot.tasks_total : counts.total;
  nodes.taskCount.textContent = tasksTotal.toString();
  if (snapshot.truncated?.plans && tasksTotal > counts.total) {
    nodes.taskBreakdown.textContent = "Partial map (plans truncated)";
  } else {
    nodes.taskBreakdown.textContent = [
      formatCount("active", counts.active),
      formatCount("backlog", counts.backlog),
      formatCount("parked", counts.parked),
      formatCount("done", counts.done),
    ].join(" | ");
  }
}

function renderGoals(snapshot) {
  clear(nodes.goalList);
  const isKnowledge = normalizeLens(snapshot?.lens || state.lens) === "knowledge";
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
    if (isKnowledge) {
      const kind = document.createElement("span");
      kind.className = "badge accent";
      kind.textContent = (plan.kind || plan.priority || "anchor").toString();
      const count = document.createElement("span");
      count.className = "badge dim";
      const total = typeof plan?.task_counts?.total === "number" ? plan.task_counts.total : 0;
      count.textContent = `${total} ключей`;
      meta.append(kind, count);
    } else {
      const status = document.createElement("span");
      status.className = "badge accent";
      status.textContent = formatStatus(plan.status);
      const count = document.createElement("span");
      count.className = "badge dim";
      count.textContent = `${plan.task_counts.done}/${plan.task_counts.total} done`;
      meta.append(status, count);
    }

    item.append(title, meta);
    item.addEventListener("click", () => {
      state.selectedPlanId = plan.id;
      render(snapshot);
      renderPlanDetail(snapshot, plan);
    });

    nodes.goalList.append(item);
  });
}

function renderChecklist(snapshot) {
  clear(nodes.planChecklist);
  const isKnowledge = normalizeLens(snapshot?.lens || state.lens) === "knowledge";
  const planId = state.selectedPlanId ?? snapshot.primary_plan_id;

  if (isKnowledge) {
    const anchor = (snapshot.plans || []).find((plan) => plan && plan.id === planId) || null;
    if (!anchor) {
      const empty = document.createElement("div");
      empty.className = "list-item";
      empty.textContent = "Select an anchor to see details.";
      nodes.planChecklist.append(empty);
      return;
    }

    const info = document.createElement("div");
    info.className = "list-item";
    info.textContent = (anchor.description || "").trim() || "No anchor description.";
    nodes.planChecklist.append(info);

    const kind = document.createElement("div");
    kind.className = "list-item";
    kind.textContent = `kind: ${(anchor.kind || anchor.priority || "anchor").toString()}`;
    nodes.planChecklist.append(kind);

    const depends = Array.isArray(anchor.depends_on) ? anchor.depends_on : [];
    if (depends.length) {
      const header = document.createElement("div");
      header.className = "list-item";
      header.textContent = "depends_on:";
      nodes.planChecklist.append(header);

      depends.slice(0, 24).forEach((dep) => {
        const id = (dep || "").trim();
        if (!id) return;
        const btn = renderListButton(
          id,
          () => {
            state.selectedPlanId = id;
            render(snapshot);
            const plan = (snapshot.plans || []).find((p) => p && p.id === id);
            if (plan) renderPlanDetail(snapshot, plan);
          },
          false
        );
        nodes.planChecklist.append(btn);
      });
    } else {
      const empty = document.createElement("div");
      empty.className = "list-item";
      empty.textContent = "No dependencies recorded.";
      nodes.planChecklist.append(empty);
    }

    return;
  }

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

function renderTasks(snapshot) {
  clear(nodes.taskList);
  const isKnowledge = normalizeLens(snapshot?.lens || state.lens) === "knowledge";
  const tasks = snapshot.tasks.filter((task) =>
    state.selectedPlanId ? task.plan_id === state.selectedPlanId : true
  );
  if (tasks.length === 0) {
    const empty = document.createElement("div");
    empty.className = "list-item";
    empty.textContent = isKnowledge ? "No knowledge keys for this anchor." : "No tasks for this plan.";
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
    if (isKnowledge) {
      const kind = document.createElement("span");
      kind.className = "badge accent";
      kind.textContent = "knowledge";
      meta.append(kind);
      const cardId = (task.card_id || task.context || "").toString().trim();
      if (cardId) {
        const card = document.createElement("span");
        card.className = "badge dim";
        card.textContent = cardId.length > 14 ? `${cardId.slice(0, 14)}…` : cardId;
        meta.append(card);
      }
    } else {
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
    }

    item.append(title, meta);
    item.addEventListener("click", () => {
      renderTaskDetail(snapshot, task);
    });
    nodes.taskList.append(item);
  });
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function hashToUnit(input) {
  let hash = 2166136261;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) / 4294967296;
}

function hashMix(hash, input) {
  let next = hash;
  for (let i = 0; i < input.length; i += 1) {
    next ^= input.charCodeAt(i);
    next = Math.imul(next, 16777619);
  }
  return next >>> 0;
}

function graphDataKey(snapshot) {
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

function tokenizeText(text) {
  const raw = (text || "").toLowerCase();
  const cleaned = raw.replace(/[^\p{L}\p{N}]+/gu, " ");
  return cleaned
    .split(" ")
    .map((t) => t.trim())
    .filter((t) => t.length >= 3)
    .filter((t) => !STOP_WORDS.has(t));
}

function tokenSet(text) {
  return new Set(tokenizeText(text));
}

function jaccardSimilarity(a, b) {
  if (!a.size || !b.size) return 0;
  let intersection = 0;
  const [small, large] = a.size <= b.size ? [a, b] : [b, a];
  small.forEach((value) => {
    if (large.has(value)) intersection += 1;
  });
  const union = a.size + b.size - intersection;
  return union > 0 ? intersection / union : 0;
}

function semanticVector(tokens, fallbackId) {
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

function pushTopK(list, candidate, k) {
  list.push(candidate);
  list.sort((a, b) => b.score - a.score);
  if (list.length > k) list.length = k;
}

function buildKnnEdges(items, k, threshold, sameGroupBonus) {
  const best = new Map();
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
      pushTopK(best.get(a.id), { id: b.id, score }, k);
      pushTopK(best.get(b.id), { id: a.id, score }, k);
    }
  }

  const dedupe = new Set();
  const edges = [];
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

function computeLod(scale) {
  const s = typeof scale === "number" ? scale : 1;
  if (s < GRAPH_CONST.lodClustersAt) return "overview";
  if (s < GRAPH_CONST.lodTasksAt) return "clusters";
  return "tasks";
}

function computeLodHysteresis(scale, prevLod) {
  const s = typeof scale === "number" ? scale : 1;
  const prev = (prevLod || "").toString();

  if (!prev || (prev !== "overview" && prev !== "clusters" && prev !== "tasks")) {
    return computeLod(s);
  }

  if (prev === "overview") {
    return computeLod(s);
  }

  if (prev === "clusters") {
    if (s < GRAPH_CONST.lodClustersOutAt) return "overview";
    if (s >= GRAPH_CONST.lodTasksAt) return "tasks";
    return "clusters";
  }

  // prev === "tasks"
  if (s < GRAPH_CONST.lodClustersOutAt) return "overview";
  if (s < GRAPH_CONST.lodTasksOutAt) return "clusters";
  return "tasks";
}

function scheduleLodDebouncedRebuild(snapshot) {
  if (graphState.lodDebounceTimer) {
    window.clearTimeout(graphState.lodDebounceTimer);
    graphState.lodDebounceTimer = 0;
  }
  const expectedKey = graphDataKey(snapshot);
  const expectedLod = graphState.lod;
  graphState.lodDebounceTimer = window.setTimeout(() => {
    graphState.lodDebounceTimer = 0;
    if (!state.snapshot) return;
    if (graphDataKey(state.snapshot) !== expectedKey) return;
    if (graphState.lod !== expectedLod) return;
    renderGraphFlagship(state.snapshot);
  }, GRAPH_CONST.lodDebounceMs);
}

function computeLocalGraphIds(edges, centerId, hops) {
  const start = (centerId || "").trim();
  if (!start) return new Set();

  const adjacency = new Map();
  const add = (a, b) => {
    const key = (a || "").trim();
    const val = (b || "").trim();
    if (!key || !val) return;
    const list = adjacency.get(key) || new Set();
    list.add(val);
    adjacency.set(key, list);
  };

  (edges || []).forEach((edge) => {
    if (!edge) return;
    add(edge.from, edge.to);
    add(edge.to, edge.from);
  });

  const depth = clamp(typeof hops === "number" ? hops : 2, 1, 3);
  const seen = new Set([start]);
  let frontier = new Set([start]);

  for (let i = 0; i < depth; i += 1) {
    const next = new Set();
    frontier.forEach((id) => {
      const neighbors = adjacency.get(id);
      if (!neighbors) return;
      neighbors.forEach((neighbor) => {
        if (seen.has(neighbor)) return;
        seen.add(neighbor);
        next.add(neighbor);
      });
    });
    if (next.size === 0) break;
    frontier = next;
  }

  return seen;
}

function refreshLocalGraph() {
  const local = graphState.local;
  const model = graphState.model;
  if (!local || !model) return;

  const key = graphState.displayKey || null;
  if (local.key && key && local.key === key) return;

  const centerId = (local.centerId || "").trim();
  if (!centerId) {
    local.ids = new Set();
    local.key = key;
    return;
  }

  const exists = (model.nodes || []).some((node) => node && node.id === centerId);
  if (!exists) {
    local.centerId = null;
    local.ids = new Set();
    local.key = key;
    return;
  }

  local.ids = computeLocalGraphIds(model.edges, centerId, local.hops);
  local.key = key;
}

function localGraphIsActive() {
  const local = graphState.local;
  if (!local) return false;
  return !!((local.centerId || "").trim());
}

function clearLocalGraph() {
  if (!graphState.local) return;
  graphState.local.centerId = null;
  graphState.local.ids = new Set();
  graphState.local.key = null;
}

function toggleLocalGraph(centerId) {
  const id = (centerId || "").trim();
  if (!id) return;
  if (graphState.local?.centerId === id) {
    clearLocalGraph();
  } else if (graphState.local) {
    graphState.local.centerId = id;
    graphState.local.key = null;
  }
}

function lodLabel(lod) {
  switch (lod) {
    case "clusters":
      return "Кластеры";
    case "tasks":
      return "Задачи";
    default:
      return "Обзор";
  }
}

function statusRank(status) {
  switch ((status || "").toUpperCase()) {
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
}

function computeHeat(nowMs, updatedAtMs) {
  const ageDays = Math.max(0, nowMs - (updatedAtMs || nowMs)) / (1000 * 60 * 60 * 24);
  return 1 / (1 + ageDays / 7);
}

function ensureSelectedPlanId(snapshot) {
  const plans = Array.isArray(snapshot?.plans) ? snapshot.plans : [];
  const ids = new Set(plans.map((plan) => plan?.id).filter((id) => id));

  const current = (state.selectedPlanId || "").trim();
  if (current && ids.has(current)) return current;

  const focus = snapshot?.focus || {};
  const focusPlanId =
    focus.kind === "plan"
      ? (focus.id || "").trim()
      : focus.kind === "task"
        ? (focus.plan_id || "").trim()
        : "";
  if (focusPlanId && ids.has(focusPlanId)) {
    state.selectedPlanId = focusPlanId;
    return focusPlanId;
  }

  const primary = (snapshot?.primary_plan_id || "").trim();
  if (primary && ids.has(primary)) {
    state.selectedPlanId = primary;
    return primary;
  }

  const first = (plans[0]?.id || "").trim();
  state.selectedPlanId = first || null;
  return state.selectedPlanId;
}

function buildPlanNodesFlagship(snapshot) {
  const nowMs = snapshot?.generated_at_ms || Date.now();
  const plans = (Array.isArray(snapshot?.plans) ? snapshot.plans : []).slice(0, GRAPH_LIMITS.maxPlans);

  const nodes = [];
  const tokensById = new Map();

  plans.forEach((plan) => {
    const id = (plan?.id || "").trim();
    if (!id) return;
    const title = (plan?.title || "").trim();
    const description = (plan?.description || "").trim();
    const tokens = tokenSet(`${title || id} ${description}`);
    tokensById.set(id, tokens);

    const vec = semanticVector(tokens, id);
    const heat = computeHeat(nowMs, plan?.updated_at_ms || nowMs);
    const radiusScale = clamp(0.55 + (1 - heat) * 0.35, 0.55, 0.9);
    const tx = vec.x * GRAPH_CONST.worldPlanRadius * radiusScale;
    const ty = vec.y * GRAPH_CONST.worldPlanRadius * radiusScale * 0.82;

    const counts = plan?.task_counts || { total: 0, done: 0, active: 0, backlog: 0, parked: 0 };
    const total = typeof counts.total === "number" ? counts.total : 0;
    const radius = 18 + clamp(Math.sqrt(Math.max(0, total)) * 1.8, 0, 22);
    const node = {
      id,
      kind: "plan",
      plan_id: id,
      status: plan?.status || "ACTIVE",
      label: title || id,
      counts,
      tx,
      ty,
      x: tx,
      y: ty,
      vx: 0,
      vy: 0,
      radius,
    };
    nodes.push(node);
  });

  nodes.sort((a, b) => a.id.localeCompare(b.id));

  return { planNodes: nodes, planTokens: tokensById };
}

function pickTasksForPlan(snapshot, planId) {
  const all = Array.isArray(snapshot?.tasks) ? snapshot.tasks : [];
  const tasks = all.filter((task) => (task?.plan_id || "").trim() === planId);
  const total = tasks.length;
  if (total <= GRAPH_LIMITS.maxTasksInPlan) {
    return { tasks, total, truncated: false };
  }
  const sorted = tasks.slice().sort((a, b) => {
    const diff = statusRank(a?.status) - statusRank(b?.status);
    if (diff !== 0) return diff;
    const time = (b?.updated_at_ms || 0) - (a?.updated_at_ms || 0);
    if (time !== 0) return time;
    return (a?.id || "").localeCompare(b?.id || "");
  });
  return {
    tasks: sorted.slice(0, GRAPH_LIMITS.maxTasksInPlan),
    total,
    truncated: true,
  };
}

function buildTaskNodesFlagship(snapshot, planNode, tasks) {
  const nowMs = snapshot?.generated_at_ms || Date.now();
  const metas = [];
  const nodes = [];

  tasks.forEach((task) => {
    const id = (task?.id || "").trim();
    if (!id) return;
    const title = (task?.title || "").trim();
    const description = (task?.description || "").trim();
    const tokens = tokenSet(`${title || id} ${description}`);
    const vec = semanticVector(tokens, id);
    const heat = computeHeat(nowMs, task?.updated_at_ms || nowMs);

    const status = task?.status || "TODO";
    const statusBoost = status === "DONE" ? 60 : status === "PARKED" ? 26 : 0;
    const recencyBoost = clamp((1 - heat) * 54, 0, 54);
    const taskRadius = GRAPH_CONST.worldTaskRadiusBase + statusBoost + recencyBoost;

    const tx = (planNode?.tx || 0) + vec.x * taskRadius;
    const ty = (planNode?.ty || 0) + vec.y * taskRadius;

    const radius = status === "DONE" ? 3.2 : status === "PARKED" ? 4 : 4.6;
    const node = {
      id,
      kind: "task",
      plan_id: task?.plan_id || planNode?.id,
      status,
      label: title || id,
      tx,
      ty,
      x: tx,
      y: ty,
      vx: 0,
      vy: 0,
      radius,
      blocked: !!task?.blocked,
    };
    nodes.push(node);
    metas.push({ id, vec, tokens, status, updated_at_ms: task?.updated_at_ms || 0, node, task });
  });

  nodes.sort((a, b) => a.id.localeCompare(b.id));
  metas.sort((a, b) => a.id.localeCompare(b.id));
  return { taskNodes: nodes, taskMetas: metas };
}

function topTokens(tokenCounts, limit) {
  const entries = Array.from(tokenCounts.entries());
  entries.sort((a, b) => {
    const diff = (b[1] || 0) - (a[1] || 0);
    if (diff !== 0) return diff;
    return a[0].localeCompare(b[0]);
  });
  return entries.slice(0, Math.max(0, limit)).map((entry) => entry[0]);
}

function buildClusterNodesFlagship(planId, planNode, taskMetas) {
  const clusters = new Map();

  taskMetas.forEach((meta) => {
    const vec = meta.vec;
    const tileX = Math.floor((vec.x + 1) / GRAPH_CONST.tile);
    const tileY = Math.floor((vec.y + 1) / GRAPH_CONST.tile);
    const id = `C:${planId}:${tileX}:${tileY}`;
    let cluster = clusters.get(id);
    if (!cluster) {
      cluster = {
        id,
        kind: "cluster",
        plan_id: planId,
        label: "кластер",
        status: "ACTIVE",
        counts: { total: 0, done: 0, active: 0, backlog: 0, parked: 0 },
        members: [],
        tokens: new Set(),
        tokenCounts: new Map(),
        sumX: 0,
        sumY: 0,
      };
      clusters.set(id, cluster);
    }

    cluster.members.push(meta.id);
    cluster.counts.total += 1;
    switch ((meta.status || "").toUpperCase()) {
      case "DONE":
        cluster.counts.done += 1;
        break;
      case "PARKED":
        cluster.counts.parked += 1;
        break;
      case "TODO":
        cluster.counts.backlog += 1;
        break;
      default:
        cluster.counts.active += 1;
    }

    cluster.sumX += meta.node.tx;
    cluster.sumY += meta.node.ty;
    meta.tokens.forEach((token) => {
      cluster.tokens.add(token);
      cluster.tokenCounts.set(token, (cluster.tokenCounts.get(token) || 0) + 1);
    });
  });

  const nodes = Array.from(clusters.values()).map((cluster) => {
    const count = cluster.counts.total || 1;
    const tx = count > 0 ? cluster.sumX / count : planNode?.tx || 0;
    const ty = count > 0 ? cluster.sumY / count : planNode?.ty || 0;
    const tokens = topTokens(cluster.tokenCounts, 2);
    const label = tokens.length ? tokens.join(" · ") : "кластер";
    const radius = clamp(10 + Math.sqrt(Math.max(1, count)) * 3.2, 10, 34);
    return {
      id: cluster.id,
      kind: "cluster",
      plan_id: planId,
      status: cluster.counts.active > 0 ? "ACTIVE" : cluster.counts.backlog > 0 ? "TODO" : "DONE",
      label,
      counts: cluster.counts,
      members: cluster.members.slice(),
      tokens: cluster.tokens,
      tx,
      ty,
      x: tx,
      y: ty,
      vx: 0,
      vy: 0,
      radius,
    };
  });

  nodes.sort((a, b) => a.id.localeCompare(b.id));
  return nodes;
}

function buildClusterSimilarityEdges(clusters) {
  const edges = [];
  for (let i = 0; i < clusters.length; i += 1) {
    for (let j = i + 1; j < clusters.length; j += 1) {
      const a = clusters[i];
      const b = clusters[j];
      const score = jaccardSimilarity(a.tokens || new Set(), b.tokens || new Set());
      if (score <= 0.32) continue;
      edges.push({ from: a.id, to: b.id, type: "similar", weight: score });
    }
  }
  edges.sort((a, b) => {
    const diff = (b.weight || 0) - (a.weight || 0);
    if (diff !== 0) return diff;
    const keyA = `${a.from}|${a.to}`;
    const keyB = `${b.from}|${b.to}`;
    return keyA.localeCompare(keyB);
  });
  return edges.slice(0, 2);
}

function buildTaskSimilarityEdges(taskMetas, maxEdges) {
  const tileSize = 0.35;
  const buckets = new Map();
  const coords = new Map();

  taskMetas.forEach((meta) => {
    const cx = Math.floor((meta.vec.x + 1) / tileSize);
    const cy = Math.floor((meta.vec.y + 1) / tileSize);
    coords.set(meta.id, { cx, cy });
    const key = `${cx},${cy}`;
    const list = buckets.get(key) || [];
    list.push(meta);
    buckets.set(key, list);
  });

  const edges = [];
  const dedupe = new Set();
  const threshold = 0.38;

  taskMetas.forEach((meta) => {
    const coord = coords.get(meta.id);
    if (!coord) return;
    const best = [];
    for (let dx = -1; dx <= 1; dx += 1) {
      for (let dy = -1; dy <= 1; dy += 1) {
        const key = `${coord.cx + dx},${coord.cy + dy}`;
        const list = buckets.get(key);
        if (!list) continue;
        for (let i = 0; i < list.length; i += 1) {
          const other = list[i];
          if (other.id === meta.id) continue;
          const score = jaccardSimilarity(meta.tokens, other.tokens);
          if (score < threshold) continue;
          pushTopK(best, { id: other.id, score }, 2);
        }
      }
    }
    best.forEach((neighbor) => {
      const a = meta.id;
      const b = neighbor.id;
      const key = a < b ? `${a}|${b}` : `${b}|${a}`;
      if (dedupe.has(key)) return;
      dedupe.add(key);
      edges.push({ from: a, to: b, type: "similar", weight: neighbor.score });
    });
  });

  edges.sort((a, b) => (b.weight || 0) - (a.weight || 0));
  return edges.slice(0, Math.max(0, maxEdges));
}

function boundsOfNodes(nodes) {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  nodes.forEach((node) => {
    minX = Math.min(minX, node.tx - node.radius);
    maxX = Math.max(maxX, node.tx + node.radius);
    minY = Math.min(minY, node.ty - node.radius);
    maxY = Math.max(maxY, node.ty + node.radius);
  });
  if (!Number.isFinite(minX) || !Number.isFinite(minY) || !Number.isFinite(maxX) || !Number.isFinite(maxY)) {
    return { minX: -1, minY: -1, maxX: 1, maxY: 1 };
  }
  return { minX, minY, maxX, maxY };
}

function buildDisplayModelFlagship(snapshot, view, lodOverride) {
  const isKnowledge = normalizeLens(snapshot?.lens || "work") === "knowledge";
  const { planNodes, planTokens } = buildPlanNodesFlagship(snapshot);
  const planById = new Map(planNodes.map((node) => [node.id, node]));

  const selectedPlanId = ensureSelectedPlanId(snapshot);
  const lod = (lodOverride || "").toString() || computeLod(view?.scale || 1);

  const nodes = planNodes.slice();
  const edges = [];
  const warnings = [];

  const truncated = snapshot?.truncated || null;
  if (truncated?.plans || truncated?.tasks) {
    const bits = [];
    if (truncated.plans) {
      const shown = Array.isArray(snapshot?.plans) ? snapshot.plans.length : 0;
      const total = Number.isFinite(snapshot?.plans_total) ? snapshot.plans_total : null;
      if (total && total > shown) {
        bits.push(
          isKnowledge
            ? `Карта неполная: якорей ${total}, показано ${shown}.`
            : `Карта неполная: целей ${total}, показано ${shown}.`
        );
      } else {
        bits.push(
          isKnowledge ? `Карта неполная: якорей показано ${shown}.` : `Карта неполная: целей показано ${shown}.`
        );
      }
    }
    if (truncated.tasks) {
      const shown = Array.isArray(snapshot?.tasks) ? snapshot.tasks.length : 0;
      const total = Number.isFinite(snapshot?.tasks_total)
        ? snapshot.tasks_total
        : sumTaskCounts(Array.isArray(snapshot?.plans) ? snapshot.plans : []).total;
      if (total && total > shown) {
        bits.push(
          isKnowledge
            ? `Карта неполная: ключей ${total}, показано ${shown}.`
            : `Карта неполная: задач ${total}, показано ${shown}.`
        );
      } else {
        bits.push(isKnowledge ? "Карта неполная: список ключей урезан." : "Карта неполная: список задач урезан.");
      }
    }
    bits.push("Совет: Ctrl+K — быстрый прыжок.");
    warnings.push(bits.join(" "));
  }

  const warning = () => (warnings.length ? warnings.join(" ") : null);

  if (lod === "overview") {
    const items = planNodes.map((plan) => ({
      id: plan.id,
      group: "plan",
      tokens: planTokens.get(plan.id) || new Set(),
    }));
    buildKnnEdges(items, 2, 0.34, 0.0).forEach((edge) => edges.push(edge));

    if (isKnowledge) {
      const ids = new Set(planNodes.map((node) => node.id));
      (snapshot.plans || []).forEach((anchor) => {
        const from = (anchor?.id || "").trim();
        if (!from || !ids.has(from)) return;
        const deps = Array.isArray(anchor?.depends_on) ? anchor.depends_on : [];
        deps.forEach((dep) => {
          const to = (dep || "").trim();
          if (!to || !ids.has(to)) return;
          edges.push({ from, to, type: "depends", weight: 1 });
        });
      });
    }

    return {
      nodes,
      edges,
      lod,
      selectedPlanId,
      warning: warning(),
      bounds: boundsOfNodes(planNodes),
    };
  }

  const planNode = selectedPlanId ? planById.get(selectedPlanId) : null;
  if (!planNode) {
    return {
      nodes,
      edges,
      lod,
      selectedPlanId,
      warning: warning(),
      bounds: boundsOfNodes(planNodes),
    };
  }

  const picked = pickTasksForPlan(snapshot, selectedPlanId);
  if (picked.truncated) {
    warnings.push(
      `Слишком много задач в цели (${picked.total}). Показано ${picked.tasks.length}.`
    );
  }
  const { taskNodes, taskMetas } = buildTaskNodesFlagship(snapshot, planNode, picked.tasks);

  if (lod === "clusters") {
    const clusters = buildClusterNodesFlagship(selectedPlanId, planNode, taskMetas);
    clusters.forEach((cluster) => nodes.push(cluster));
    clusters.forEach((cluster) =>
      edges.push({ from: cluster.id, to: selectedPlanId, type: "hierarchy", weight: 1 })
    );
    buildClusterSimilarityEdges(clusters).forEach((edge) => edges.push(edge));
  } else {
    taskNodes.forEach((task) => nodes.push(task));
    taskNodes.forEach((task) =>
      edges.push({ from: task.id, to: selectedPlanId, type: "hierarchy", weight: 1 })
    );
    if ((view?.scale || 1) >= GRAPH_CONST.showTaskSimilarEdgesAt) {
      buildTaskSimilarityEdges(taskMetas, 820).forEach((edge) => edges.push(edge));
    }
  }

  return {
    nodes,
    edges,
    lod,
    selectedPlanId,
    warning: warning(),
    bounds: boundsOfNodes(planNodes),
  };
}

function buildGraphModel(snapshot, width, height) {
  const nowMs = snapshot.generated_at_ms || Date.now();
  const statusRank = (status) => {
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

  const tasksSorted = snapshot.tasks
    .slice()
    .sort((a, b) => {
      const diff = statusRank(a.status) - statusRank(b.status);
      if (diff !== 0) return diff;
      const time = (b.updated_at_ms || 0) - (a.updated_at_ms || 0);
      if (time !== 0) return time;
      return a.id.localeCompare(b.id);
    });
  const tasks = tasksSorted.slice(0, Math.min(MAX_TASK_NODES, tasksSorted.length));

  const planById = new Map(snapshot.plans.map((plan) => [plan.id, plan]));
  const neededPlanIds = new Set(tasks.map((task) => task.plan_id));
  const focus = snapshot.focus;
  if (focus?.kind === "plan" && focus.id) neededPlanIds.add(focus.id);
  if (focus?.kind === "task" && focus.plan_id) neededPlanIds.add(focus.plan_id);
  if (state.selectedPlanId) neededPlanIds.add(state.selectedPlanId);

  const plansSorted = snapshot.plans
    .slice()
    .sort((a, b) => {
      const diff = statusRank(a.status) - statusRank(b.status);
      if (diff !== 0) return diff;
      const time = (b.updated_at_ms || 0) - (a.updated_at_ms || 0);
      if (time !== 0) return time;
      return a.id.localeCompare(b.id);
    });

  const plansPicked = [];
  const pickedIds = new Set();
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
  const nodesList = [];
  const edges = [];
  const planMap = new Map();

  const maxX = width * 0.48;
  const maxY = height * 0.48;
  const semanticX = maxX * 0.9;
  const semanticY = maxY * 0.9;

  const planTokenCache = new Map();
  const taskTokenCache = new Map();

  const computeHeat = (updatedAtMs) => {
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
    const node = {
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
    const anchor = planMap.get(task.plan_id) ?? nodesList[index % Math.max(1, nodesList.length)];
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
    const node = {
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

  const nodeIndex = new Map(nodesList.map((node) => [node.id, node]));

  const planItems = plans.map((plan) => ({
    id: plan.id,
    group: "plan",
    tokens:
      planTokenCache.get(plan.id) ||
      tokenSet(`${plan.title || plan.id} ${plan.description || ""}`),
  }));
  const taskItems = tasks.map((task) => ({
    id: task.id,
    group: task.plan_id,
    tokens:
      taskTokenCache.get(task.id) ||
      tokenSet(`${task.title || task.id} ${task.description || ""}`),
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
      let target = 82;
      let strength = 0.012;
      if (edge.type === "similar") {
        target = clamp(132 - (edge.weight || 0) * 110, 58, 132);
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

function ensureGraphView(width, height) {
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

function screenToWorld(view, x, y) {
  return {
    x: (x - view.offsetX) / view.scale,
    y: (y - view.offsetY) / view.scale,
  };
}

function hitTestNode(model, view, x, y) {
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

function drawGraph(snapshot) {
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
  const nodeById = new Map(model.nodes.map((node) => [node.id, node]));
  const highlight = new Set();
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

function viewStorageKey(workspace) {
  const ws = (workspace || "").trim() || "auto";
  const lens = state.lens || "work";
  return `bm_viewer_view:${activeProjectKey()}:${ws}:${lens}`;
}

function layoutStorageKey(workspace) {
  const ws = (workspace || "").trim() || "auto";
  const lens = state.lens || "work";
  return `bm_viewer_layout:${activeProjectKey()}:${ws}:${lens}`;
}

function loadStoredViewState(key) {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    const offsetX = Number(parsed?.offsetX);
    const offsetY = Number(parsed?.offsetY);
    const scale = Number(parsed?.scale);
    if (!Number.isFinite(offsetX) || !Number.isFinite(offsetY) || !Number.isFinite(scale)) {
      return null;
    }
    if (scale < 0.35 || scale > 2.6) return null;
    return { offsetX, offsetY, scale };
  } catch {
    return null;
  }
}

function loadStoredLayout(key) {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    const nodes = parsed?.nodes;
    if (!nodes || typeof nodes !== "object") return null;

    const out = new Map();
    Object.entries(nodes).forEach(([id, pos]) => {
      const key = (id || "").toString().trim();
      if (!key) return;
      const x = Number(pos?.x);
      const y = Number(pos?.y);
      if (!Number.isFinite(x) || !Number.isFinite(y)) return;
      out.set(key, { x, y });
    });
    if (out.size === 0) return null;
    return out;
  } catch {
    return null;
  }
}

function ensureLayoutRestore(snapshot) {
  const key = layoutStorageKey(snapshot?.workspace || state.workspaceOverride || "");
  if (layoutCache.lastLayoutKey === key) return;
  layoutCache.lastLayoutKey = key;
  layoutCache.storedPositions.clear();
  const stored = loadStoredLayout(key);
  if (!stored) return;
  stored.forEach((pos, id) => {
    layoutCache.storedPositions.set(id, pos);
  });
}

function scheduleSaveViewState(snapshot) {
  if (!graphState.view) return;
  const key = viewStorageKey(snapshot?.workspace || state.workspaceOverride || "");
  if (layoutCache.lastViewKey !== key) {
    layoutCache.lastViewKey = key;
  }
  if (layoutCache.saveTimer) {
    window.clearTimeout(layoutCache.saveTimer);
  }
  layoutCache.saveTimer = window.setTimeout(() => {
    if (!graphState.view) return;
    try {
      localStorage.setItem(
        key,
        JSON.stringify({
          offsetX: graphState.view.offsetX,
          offsetY: graphState.view.offsetY,
          scale: graphState.view.scale,
        })
      );
    } catch {
      // ignore storage failures
    }
  }, 320);
}

function scheduleSaveLayoutPositions(snapshot) {
  if (!snapshot) return;
  const key = layoutStorageKey(snapshot?.workspace || state.workspaceOverride || "");
  if (layoutCache.layoutSaveTimer) {
    window.clearTimeout(layoutCache.layoutSaveTimer);
  }
  layoutCache.layoutSaveTimer = window.setTimeout(() => {
    layoutCache.layoutSaveTimer = 0;
    const ids = [];
    (snapshot.plans || []).forEach((plan) => {
      if (plan && plan.id) ids.push(plan.id);
    });
    (snapshot.tasks || []).forEach((task) => {
      if (task && task.id) ids.push(task.id);
    });
    if (ids.length === 0) return;

    const MAX_NODES = 1400;
    const nodes = {};
    for (let i = 0; i < ids.length && i < MAX_NODES; i += 1) {
      const id = (ids[i] || "").trim();
      if (!id) continue;
      const node = layoutCache.nodesById.get(id);
      if (!node) continue;
      const x = Math.round((node.x || 0) * 10) / 10;
      const y = Math.round((node.y || 0) * 10) / 10;
      nodes[id] = { x, y };
    }
    try {
      localStorage.setItem(
        key,
        JSON.stringify({
          v: 1,
          savedAt: Date.now(),
          dataKey: graphDataKey(snapshot),
          nodes,
        })
      );
    } catch {
      // ignore storage failures
    }
  }, 900);
}

function ensureGraphViewFlagship(width, height, snapshot) {
  if (graphState.view) return;
  const view = {
    offsetX: width * 0.5,
    offsetY: height * 0.5,
    scale: 1,
    dragging: false,
    lastX: 0,
    lastY: 0,
    moved: false,
    restored: false,
  };
  const stored = loadStoredViewState(viewStorageKey(snapshot?.workspace || state.workspaceOverride || ""));
  if (stored) {
    view.offsetX = stored.offsetX;
    view.offsetY = stored.offsetY;
    view.scale = stored.scale;
    view.restored = true;
  }
  graphState.view = view;
  graphState.lod = computeLod(view.scale);
}

function worldToScreen(view, x, y) {
  return {
    x: x * view.scale + view.offsetX,
    y: y * view.scale + view.offsetY,
  };
}

function fitViewToBounds(view, bounds, width, height, clampToOverview) {
  const pad = 150;
  const w = Math.max(1, bounds.maxX - bounds.minX + pad * 2);
  const h = Math.max(1, bounds.maxY - bounds.minY + pad * 2);
  const fit = Math.min(width / w, height / h);
  let scale = clamp(fit, 0.45, 2.2);
  if (clampToOverview) {
    scale = Math.min(scale, 0.88);
  }
  const cx = (bounds.minX + bounds.maxX) * 0.5;
  const cy = (bounds.minY + bounds.maxY) * 0.5;
  view.scale = scale;
  view.offsetX = width * 0.5 - cx * scale;
  view.offsetY = height * 0.5 - cy * scale;
}

function easeOutCubic(t) {
  const x = clamp(t, 0, 1);
  return 1 - Math.pow(1 - x, 3);
}

function startCameraAnimation(to, durationMs) {
  if (!graphState.view) return;
  const now = performance.now();
  graphState.cameraAnim = {
    startAt: now,
    endAt: now + Math.max(120, durationMs || 420),
    from: {
      offsetX: graphState.view.offsetX,
      offsetY: graphState.view.offsetY,
      scale: graphState.view.scale,
    },
    to,
  };
  ensureGraphAnimationLoop();
}

function startSettleAnimation() {
  const now = performance.now();
  graphState.settle = { endAt: now + GRAPH_CONST.settleMs };
  ensureGraphAnimationLoop();
}

function ensureGraphAnimationLoop() {
  if (graphState.animating) return;
  graphState.animating = true;
  graphState.animationFrame = window.requestAnimationFrame(onGraphFrame);
}

function onGraphFrame(ts) {
  graphState.animationFrame = 0;
  const now = typeof ts === "number" ? ts : performance.now();
  let again = false;

  if (graphState.cameraAnim && graphState.view) {
    const anim = graphState.cameraAnim;
    const prevLod = graphState.lod;
    const t = (now - anim.startAt) / Math.max(1, anim.endAt - anim.startAt);
    const e = easeOutCubic(t);
    graphState.view.offsetX = anim.from.offsetX + (anim.to.offsetX - anim.from.offsetX) * e;
    graphState.view.offsetY = anim.from.offsetY + (anim.to.offsetY - anim.from.offsetY) * e;
    graphState.view.scale = anim.from.scale + (anim.to.scale - anim.from.scale) * e;
    graphState.lod = computeLodHysteresis(graphState.view.scale, prevLod);
    const lodChanged = prevLod && prevLod !== graphState.lod;
    if (lodChanged && state.snapshot && UI_MODE === "flagship") {
      const display = buildDisplayModelFlagship(state.snapshot, graphState.view, graphState.lod);
      mergeDisplayModelIntoCache(display);
      const snapshotKey = graphDataKey(state.snapshot);
      graphState.displayKey = `${snapshotKey}:${display.selectedPlanId || "none"}:${display.lod}`;
      graphState.snapshotKey = snapshotKey;
      startSettleAnimation();
    }
    if (t >= 1) {
      graphState.cameraAnim = null;
    } else {
      again = true;
    }
  }

  if (graphState.settle) {
    settleLayoutOnce();
    if (now >= graphState.settle.endAt) {
      graphState.settle = null;
      // Freeze velocities to avoid "forever floating".
      layoutCache.visibleIds.forEach((id) => {
        const node = layoutCache.nodesById.get(id);
        if (node) {
          node.vx = 0;
          node.vy = 0;
        }
      });
    } else {
      again = true;
    }
  }

  let fadeActive = false;
  layoutCache.fade.forEach((entry, id) => {
    const age = now - entry.startedAt;
    if (age >= GRAPH_CONST.fadeMs) {
      if (entry.mode === "out") {
        layoutCache.renderIds.delete(id);
      }
      layoutCache.fade.delete(id);
    } else {
      fadeActive = true;
    }
  });
  if (fadeActive) again = true;

  if (state.snapshot && UI_MODE === "flagship") {
    drawGraphFlagship(state.snapshot, now);
    drawMinimapFlagship(state.snapshot);
    scheduleSaveViewState(state.snapshot);
    scheduleSaveLayoutPositions(state.snapshot);
  }

  if (again && UI_MODE === "flagship") {
    graphState.animationFrame = window.requestAnimationFrame(onGraphFrame);
    return;
  }
  graphState.animating = false;
}

function fadeAlphaForId(id, now) {
  const entry = layoutCache.fade.get(id);
  if (!entry) return 1;
  const t = clamp((now - entry.startedAt) / GRAPH_CONST.fadeMs, 0, 1);
  if (entry.mode === "in") return t;
  if (entry.mode === "out") return 1 - t;
  return 1;
}

function mergeDisplayModelIntoCache(display) {
  const now = performance.now();
  const nextIds = new Set((display?.nodes || []).map((node) => node?.id).filter((id) => id));

  const prevVisible = new Set(layoutCache.visibleIds);
  layoutCache.visibleIds = nextIds;

  nextIds.forEach((id) => {
    if (!prevVisible.has(id)) {
      layoutCache.fade.set(id, { mode: "in", startedAt: now });
    }
  });

  prevVisible.forEach((id) => {
    if (!nextIds.has(id)) {
      layoutCache.fade.set(id, { mode: "out", startedAt: now });
    }
  });

  layoutCache.renderIds = new Set([...nextIds, ...Array.from(layoutCache.fade.keys())]);

  // Spawn anchors for premium semantic zoom:
  // - clusters spawn from their parent plan (“continent” → “cities”)
  // - tasks spawn from their previous cluster (“city” → “streets”), when available
  // - clusters (on collapse) spawn from the centroid of their member tasks, when available
  const taskAnchorById = new Map();
  layoutCache.nodesById.forEach((node) => {
    if (!node || node.kind !== "cluster") return;
    const members = Array.isArray(node.members) ? node.members : [];
    members.forEach((memberId) => {
      if (memberId && !taskAnchorById.has(memberId)) {
        taskAnchorById.set(memberId, { x: node.x, y: node.y });
      }
    });
  });

  const planAnchor = (planId) => {
    const id = (planId || "").trim();
    if (!id) return null;
    const plan = layoutCache.nodesById.get(id);
    if (!plan) return null;
    return { x: plan.x, y: plan.y };
  };

  const centroidAnchor = (members) => {
    const ids = Array.isArray(members) ? members : [];
    if (ids.length === 0) return null;
    let sumX = 0;
    let sumY = 0;
    let count = 0;
    for (let i = 0; i < ids.length; i += 1) {
      const id = ids[i];
      const node = id ? layoutCache.nodesById.get(id) : null;
      if (!node || node.kind !== "task") continue;
      sumX += node.x;
      sumY += node.y;
      count += 1;
    }
    if (!count) return null;
    return { x: sumX / count, y: sumY / count };
  };

  const spawnAnchorFor = (next, id) => {
    if (!next) return null;
    const kind = (next.kind || "").toString();
    if (kind === "cluster") {
      return centroidAnchor(next.members) || planAnchor(next.plan_id || next.planId || "");
    }
    if (kind === "task") {
      return taskAnchorById.get(id) || planAnchor(next.plan_id || next.planId || "");
    }
    if (kind === "plan") {
      return null;
    }
    return null;
  };

  (display?.nodes || []).forEach((next) => {
    const id = (next?.id || "").trim();
    if (!id) return;
    const existing = layoutCache.nodesById.get(id);
    if (existing) {
      existing.kind = next.kind;
      existing.plan_id = next.plan_id;
      existing.status = next.status;
      existing.label = next.label;
      existing.counts = next.counts;
      existing.members = next.members;
      existing.tokens = next.tokens;
      existing.blocked = next.blocked;
      existing.radius = next.radius;
      existing.tx = next.tx;
      existing.ty = next.ty;
      return;
    }
    const jitter = (hashToUnit(`${id}-spawn`) - 0.5) * 26;
    const anchor = spawnAnchorFor(next, id);
    const ax = anchor ? anchor.x : next.tx || 0;
    const ay = anchor ? anchor.y : next.ty || 0;
    const stored = layoutCache.storedPositions.get(id) || null;
    const node = {
      ...next,
      x: stored ? stored.x : ax + jitter,
      y: stored ? stored.y : ay - jitter * 0.7,
      vx: 0,
      vy: 0,
    };
    layoutCache.nodesById.set(id, node);
  });

  const visibleNodes = Array.from(nextIds)
    .map((id) => layoutCache.nodesById.get(id))
    .filter((node) => !!node);
  visibleNodes.sort((a, b) => a.id.localeCompare(b.id));

  graphState.model = {
    nodes: visibleNodes,
    edges: display.edges || [],
    bounds: display.bounds,
    lod: display.lod,
    selectedPlanId: display.selectedPlanId,
    warning: display.warning,
  };
  graphState.lod = display.lod;
}

function settleLayoutOnce() {
  const ids = Array.from(layoutCache.visibleIds);
  const nodesList = ids
    .map((id) => layoutCache.nodesById.get(id))
    .filter((node) => node && node.kind);
  if (nodesList.length === 0) return;

  const cellSize = 120;
  const buckets = new Map();
  nodesList.forEach((node) => {
    const cx = Math.floor(node.x / cellSize);
    const cy = Math.floor(node.y / cellSize);
    const key = `${cx},${cy}`;
    const list = buckets.get(key) || [];
    list.push(node);
    buckets.set(key, list);
  });

  const keys = Array.from(buckets.keys()).sort();
  const getBucket = (cx, cy) => buckets.get(`${cx},${cy}`);
  const applyPair = (a, b) => {
    // Mental map invariant: plans (“continents”) must not drift when we expand/collapse
    // clusters/tasks. Allow plan↔plan repulsion, but skip plan↔(cluster/task).
    const aPlan = a && a.kind === "plan";
    const bPlan = b && b.kind === "plan";
    if (aPlan !== bPlan) return;

    let dx = a.x - b.x;
    let dy = a.y - b.y;
    const dist2 = dx * dx + dy * dy + 0.01;
    const minDist = (a.radius || 6) + (b.radius || 6) + 18;
    const cutoff = minDist * 6;
    if (dist2 > cutoff * cutoff) return;
    const dist = Math.sqrt(dist2);
    dx /= dist;
    dy /= dist;
    const force = 1200 / dist2 + (dist < minDist ? 1.4 : 0);
    a.vx += dx * force;
    a.vy += dy * force;
    b.vx -= dx * force;
    b.vy -= dy * force;
  };

  keys.forEach((key) => {
    const parts = key.split(",");
    const cx = Number(parts[0]);
    const cy = Number(parts[1]);
    const bucket = buckets.get(key) || [];
    for (let i = 0; i < bucket.length; i += 1) {
      for (let j = i + 1; j < bucket.length; j += 1) {
        applyPair(bucket[i], bucket[j]);
      }
    }
    const neighborSpecs = [
      [cx + 1, cy],
      [cx, cy + 1],
      [cx + 1, cy + 1],
      [cx + 1, cy - 1],
    ];
    neighborSpecs.forEach(([nx, ny]) => {
      const other = getBucket(nx, ny);
      if (!other) return;
      for (let i = 0; i < bucket.length; i += 1) {
        for (let j = 0; j < other.length; j += 1) {
          applyPair(bucket[i], other[j]);
        }
      }
    });
  });

  const step = 0.018;
  const damp = 0.78;
  const worldMax = GRAPH_CONST.worldPlanRadius * 1.6;
  nodesList.forEach((node) => {
    const pull = node.kind === "plan" ? 0.028 : node.kind === "cluster" ? 0.02 : 0.016;
    node.vx += (node.tx - node.x) * pull;
    node.vy += (node.ty - node.y) * pull;
    node.vx *= damp;
    node.vy *= damp;
    node.x = clamp(node.x + node.vx * step, -worldMax, worldMax);
    node.y = clamp(node.y + node.vy * step, -worldMax, worldMax);
  });
}

function planFillColor(status) {
  const normalized = (status || "").toUpperCase();
  if (normalized === "DONE") return "rgba(125, 211, 199, 0.35)";
  if (normalized === "PARKED") return "rgba(251, 191, 36, 0.32)";
  if (normalized === "TODO") return "rgba(132, 169, 255, 0.34)";
  return "rgba(125, 211, 199, 0.75)";
}

function nodeInk(status) {
  const normalized = (status || "").toUpperCase();
  if (normalized === "DONE") return "rgba(125, 211, 199, 0.55)";
  if (normalized === "PARKED") return "rgba(251, 191, 36, 0.9)";
  if (normalized === "TODO") return "rgba(238, 242, 246, 0.9)";
  return "rgba(238, 242, 246, 0.95)";
}

function drawGraphFlagship(snapshot, now) {
  const canvas = nodes.graph;
  const model = graphState.model;
  const view = graphState.view;
  if (!canvas || !model || !view) return;
  refreshLocalGraph();
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

  const lod = model.lod || graphState.lod || "overview";
  const selectedPlanId = model.selectedPlanId || snapshot.primary_plan_id || null;
  const focus = snapshot.focus || { kind: "none" };
  const focusId = (focus.id || "").trim() || null;
  const focusPlanId =
    focus.kind === "plan"
      ? (focus.id || "").trim()
      : focus.kind === "task"
        ? (focus.plan_id || "").trim()
        : null;
  const selectedId = state.detailSelection ? state.detailSelection.id : null;
  const hoverId = graphState.hoverId;
  const localActive = localGraphIsActive();
  const localIds = graphState.local?.ids || new Set();

  const nodeById = new Map(model.nodes.map((node) => [node.id, node]));
  const renderIds = Array.from(layoutCache.renderIds);
  const renderNodes = renderIds
    .map((id) => nodeById.get(id) || layoutCache.nodesById.get(id))
    .filter((node) => !!node);

  // Edges (visible model only; keeps noise low).
  ctx.globalAlpha = 1;
  model.edges.forEach((edge) => {
    const from = nodeById.get(edge.from);
    const to = nodeById.get(edge.to);
    if (!from || !to) return;

    const isSimilar = edge.type === "similar";
    const connected =
      (selectedId && (edge.from === selectedId || edge.to === selectedId)) ||
      (hoverId && (edge.from === hoverId || edge.to === hoverId)) ||
      (focusId && (edge.from === focusId || edge.to === focusId));
    const localOk = !localActive || (localIds.has(edge.from) && localIds.has(edge.to));
    if (!localOk && !connected) return;

    if (isSimilar && lod === "overview") {
      // keep overview edges rare
      if ((edge.weight || 0) < 0.36 && !connected) return;
    }
    if (isSimilar && lod === "tasks" && view.scale < GRAPH_CONST.showTaskSimilarEdgesAt) {
      return;
    }

    const baseAlpha = connected ? (isSimilar ? 0.34 : 0.38) : isSimilar ? 0.12 : 0.1;
    const localFactor = localActive && !localOk ? 0.08 : 1;
    const alpha = (baseAlpha + (connected ? 0.15 : 0)) * localFactor;
    const color = isSimilar ? "132, 169, 255" : "255, 255, 255";
    ctx.strokeStyle = `rgba(${color}, ${alpha})`;
    ctx.lineWidth = (isSimilar ? 1.1 : 1) / view.scale;
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
  });

  // Nodes.
  renderNodes.forEach((node) => {
    const alphaFade = fadeAlphaForId(node.id, now);
    if (alphaFade <= 0.01) return;

    const isSelected = node.id === selectedId;
    const isHover = node.id === hoverId;
    const isFocus = focusId && node.id === focusId;
    const isPrimary =
      node.kind === "plan" && (node.id === selectedPlanId || node.id === focusPlanId);
    const isLocal = !localActive || localIds.has(node.id);

    let alpha = 0.9;
    if (node.kind === "plan") {
      alpha = lod === "overview" ? (isPrimary || isHover || isFocus ? 0.92 : 0.78) : isPrimary ? 0.92 : 0.2;
    } else if (node.kind === "cluster") {
      alpha = isHover || isSelected ? 0.92 : 0.75;
    } else {
      alpha = isHover || isSelected ? 0.9 : 0.55;
    }
    alpha *= alphaFade;
    if (!isLocal) {
      alpha *= node.kind === "plan" ? 0.18 : 0.1;
    }

    if (node.kind === "plan") {
      ctx.fillStyle = planFillColor(node.status).replace(/0\.\d+\)/, `${alpha})`);
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fill();

      // Progress ring.
      const counts = node.counts || {};
      const total = typeof counts.total === "number" ? counts.total : 0;
      const done = typeof counts.done === "number" ? counts.done : 0;
      const frac = total > 0 ? clamp(done / total, 0, 1) : 0;
      const ringR = node.radius + 8 / view.scale;
      const lw = 4 / view.scale;
      ctx.lineWidth = lw;
      ctx.strokeStyle = `rgba(255, 255, 255, ${0.12 * alphaFade})`;
      ctx.beginPath();
      ctx.arc(node.x, node.y, ringR, 0, Math.PI * 2);
      ctx.stroke();
      if (frac > 0) {
        ctx.strokeStyle = `rgba(125, 211, 199, ${0.65 * alphaFade})`;
        ctx.beginPath();
        ctx.arc(node.x, node.y, ringR, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * frac);
        ctx.stroke();
      }
    } else if (node.kind === "cluster") {
      ctx.fillStyle = `rgba(132, 169, 255, ${0.18 * alpha})`;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = `rgba(132, 169, 255, ${0.28 * alpha})`;
      ctx.lineWidth = 1.2 / view.scale;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.stroke();
    } else {
      const tint = nodeInk(node.status);
      ctx.fillStyle = tint.replace(/0\.\d+\)/, `${alpha})`);
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fill();
    }

    if (isHover || isSelected) {
      ctx.strokeStyle = `rgba(132, 169, 255, ${0.8 * alphaFade})`;
      ctx.lineWidth = 2 / view.scale;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius + 6 / view.scale, 0, Math.PI * 2);
      ctx.stroke();
    }

    if (isFocus) {
      const pulse = 0.55 + 0.45 * Math.sin((now || performance.now()) / 1200);
      ctx.strokeStyle = `rgba(132, 169, 255, ${0.25 + 0.25 * pulse})`;
      ctx.lineWidth = 1.6 / view.scale;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius + 12 / view.scale, 0, Math.PI * 2);
      ctx.stroke();
    }
  });

  // Labels (screen space).
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.font = `${12 * ratio}px "IBM Plex Sans", "SF Pro Text", "Segoe UI", sans-serif`;
  ctx.fillStyle = "rgba(238, 242, 246, 0.86)";

  const planCount = model.nodes.filter((n) => n.kind === "plan").length;
  const showPlanLabelsEverywhere = planCount <= 24 && lod !== "overview";

  renderNodes.forEach((node) => {
    const alphaFade = fadeAlphaForId(node.id, now);
    if (alphaFade <= 0.01) return;

    const isHover = node.id === hoverId;
    const isSelected = node.id === selectedId;
    const isFocus = focusId && node.id === focusId;
    const isPrimary = node.kind === "plan" && (node.id === selectedPlanId || node.id === focusPlanId);
    const isLocal = !localActive || localIds.has(node.id);
    if (!isLocal) return;

    let shouldLabel = false;
    if (node.kind === "plan") {
      shouldLabel = showPlanLabelsEverywhere || isHover || isSelected || isFocus || isPrimary;
    } else if (node.kind === "cluster") {
      shouldLabel = (view.scale >= 1.05 && (isHover || isSelected)) || view.scale >= 1.15;
    } else if (node.kind === "task") {
      shouldLabel = isHover || isSelected || view.scale >= GRAPH_CONST.showTaskLabelsAt;
    }
    if (!shouldLabel) return;

    const pos = worldToScreen(view, node.x, node.y);
    const sx = pos.x * ratio;
    const sy = pos.y * ratio;
    ctx.fillText(node.label, sx + 10, sy - 8);
  });
}

function drawMinimapFlagship(snapshot) {
  const canvas = nodes.minimap;
  const model = graphState.model;
  const view = graphState.view;
  const graphCanvas = nodes.graph;
  if (!canvas || !model || !view || !graphCanvas) return;
  const ratio = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  const w = Math.max(1, Math.floor(rect.width * ratio));
  const h = Math.max(1, Math.floor(rect.height * ratio));
  if (canvas.width !== w) canvas.width = w;
  if (canvas.height !== h) canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const pad = 10 * ratio;
  const plans = model.nodes.filter((n) => n.kind === "plan");
  const bounds = boundsOfNodes(plans);
  const bw = Math.max(1, bounds.maxX - bounds.minX);
  const bh = Math.max(1, bounds.maxY - bounds.minY);
  const sx = (w - pad * 2) / bw;
  const sy = (h - pad * 2) / bh;
  const scale = Math.min(sx, sy);
  const mapX = (x) => pad + (x - bounds.minX) * scale;
  const mapY = (y) => pad + (y - bounds.minY) * scale;

  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = "rgba(12, 16, 22, 0.35)";
  ctx.fillRect(0, 0, w, h);

  ctx.fillStyle = "rgba(125, 211, 199, 0.7)";
  plans.forEach((plan) => {
    ctx.beginPath();
    ctx.arc(mapX(plan.x), mapY(plan.y), 2.2 * ratio, 0, Math.PI * 2);
    ctx.fill();
  });

  // Viewport rectangle in world coords.
  const graphRect = graphCanvas.getBoundingClientRect();
  const vw = Math.max(1, graphRect.width);
  const vh = Math.max(1, graphRect.height);
  const left = (-view.offsetX) / view.scale;
  const top = (-view.offsetY) / view.scale;
  const right = (vw - view.offsetX) / view.scale;
  const bottom = (vh - view.offsetY) / view.scale;

  const rx = mapX(left);
  const ry = mapY(top);
  const rw = (right - left) * scale;
  const rh = (bottom - top) * scale;
  ctx.strokeStyle = "rgba(132, 169, 255, 0.85)";
  ctx.lineWidth = 1.2 * ratio;
  ctx.strokeRect(rx, ry, rw, rh);
}

function openClusterDetail(snapshot, clusterNode) {
  const token = startDetailLoad("cluster", clusterNode.id);
  nodes.detailKicker.textContent = "Кластер";
  nodes.detailTitle.textContent = clusterNode.label || "Кластер";
  const counts = clusterNode.counts || {};
  renderDetailMeta([
    `Задач: ${counts.done || 0}/${counts.total || 0} сделано`,
    `В работе: ${counts.active || 0}`,
    `В очереди: ${counts.backlog || 0}`,
    `Отложено: ${counts.parked || 0}`,
  ]);
  clear(nodes.detailBody);

  const sections = [];
  sections.push(
    renderDetailSection("О чём кластер", [
      renderDetailText(clusterNode.label || null, "Пока без ярлыка."),
    ])
  );

  const members = Array.isArray(clusterNode.members) ? clusterNode.members.slice() : [];

  const selection = state.detailSelection;
  if (selection) {
    selection.graphTasksPager = {
      kind: "cluster",
      id: clusterNode.id,
      cursor: null,
      has_more: true,
      loading: false,
      last_error: null,
      started: false,
      limit: 200,
      tasks: [],
      seen: new Set(),
    };
  }

  const caption = document.createElement("div");
  caption.className = "detail-caption";

  const listHost = document.createElement("div");
  listHost.className = "detail-list";

  const loadBtn = renderLoadMoreButton(
    "Load more tasks",
    () => {
      void loadMore(false);
    },
    false
  );

  function previewTasks(snapshotNow) {
    return (snapshotNow.tasks || [])
      .filter((task) => members.includes(task.id))
      .slice()
      .sort(
        (a, b) =>
          statusRank(a.status) - statusRank(b.status) || (a.id || "").localeCompare(b.id || "")
      )
      .slice(0, 10);
  }

  function renderList() {
    const snapshotNow = state.snapshot || snapshot;
    const current = state.detailSelection;
    const pager = current?.graphTasksPager;
    const started = !!pager?.started;
    const tasks = started
      ? (pager?.tasks || []).slice(0, GRAPH_LIMITS.maxTasksInPlan)
      : previewTasks(snapshotNow);

    clear(listHost);
    if (!tasks.length) {
      const msg = started ? "No tasks loaded yet." : "Нет задач в этом кластере.";
      listHost.append(renderDetailText(null, msg));
    } else {
      tasks.forEach((task) => listHost.append(renderTaskListButton(task, snapshotNow)));
    }
    listHost.append(loadBtn);
  }

  function updateControls(snapshotNow) {
    const current = state.detailSelection;
    const pager = current?.graphTasksPager;
    if (!pager || pager.kind !== "cluster" || pager.id !== clusterNode.id) return;

    const loaded = Array.isArray(pager.tasks) ? pager.tasks.length : 0;
    const capReached = loaded >= GRAPH_LIMITS.maxTasksInPlan;
    const baseCaption = pager.started
      ? `Loaded: ${loaded} tasks`
      : `Preview: ${previewTasks(snapshotNow).length} tasks`;
    caption.textContent = pager.last_error ? `${baseCaption} — ${pager.last_error}` : baseCaption;

    const canLoadMore = !capReached && pager.has_more;
    loadBtn.style.display = canLoadMore ? "" : "none";
    loadBtn.disabled = !!pager.loading;
    loadBtn.textContent = pager.loading
      ? "Loading…"
      : pager.last_error
        ? "Retry loading tasks"
        : "Load more tasks";
  }

  async function loadMore(prefetch) {
    if (!isCurrentDetail(token)) return;
    const current = state.detailSelection;
    const pager = current?.graphTasksPager;
    if (!pager || pager.kind !== "cluster" || pager.id !== clusterNode.id) return;
    if (pager.loading) return;

    if (Array.isArray(pager.tasks) && pager.tasks.length >= GRAPH_LIMITS.maxTasksInPlan) {
      pager.has_more = false;
      pager.started = true;
      renderList();
      updateControls(state.snapshot || snapshot);
      return;
    }

    if (!pager.has_more && !prefetch) return;

    pager.loading = true;
    pager.last_error = null;
    updateControls(state.snapshot || snapshot);
    try {
      const payload = await fetchGraphCluster(clusterNode.id, {
        cursor: pager.cursor,
        limit: pager.limit,
      });
      if (!isCurrentDetail(token)) return;
      ingestGraphOverlay(payload);
      const merged = mergeGraphOverlay(state.snapshot || snapshot);
      render(merged);

      if (!pager.started) {
        pager.started = true;
        pager.tasks = [];
        pager.seen = new Set();
      }

      const items = Array.isArray(payload?.tasks) ? payload.tasks : [];
      items.forEach((task) => {
        if (!task || !task.id) return;
        if (pager.seen.has(task.id)) return;
        if (pager.tasks.length >= GRAPH_LIMITS.maxTasksInPlan) return;
        pager.seen.add(task.id);
        pager.tasks.push(task);
      });

      const pagination = payload?.pagination || {};
      const hasMore = !!pagination.has_more;
      const nextCursor = pagination.next_cursor ?? null;
      if (hasMore && (nextCursor === null || nextCursor === "")) {
        pager.last_error = "Paging stalled: server returned has_more without next_cursor.";
        pager.cursor = null;
        pager.has_more = false;
        return;
      }

      pager.cursor = nextCursor;
      pager.has_more = hasMore && pager.tasks.length < GRAPH_LIMITS.maxTasksInPlan;
    } catch (err) {
      if (isCurrentDetail(token)) {
        pager.last_error = toUserError(err);
      }
    } finally {
      if (isCurrentDetail(token)) {
        const latest = state.detailSelection?.graphTasksPager;
        if (latest && latest.kind === "cluster" && latest.id === clusterNode.id) {
          latest.loading = false;
        }
      }
    }

    if (!isCurrentDetail(token)) return;
    const snapshotFinal = state.snapshot || snapshot;
    renderList();
    updateControls(snapshotFinal);
  }

  renderList();
  updateControls(snapshot);
  sections.push(renderDetailSection("Задачи", [caption, listHost]));

  nodes.detailBody.append(...sections);
  setDetailVisible(true);
  if (!isCurrentDetail(token)) return;

  const lens = normalizeLens(snapshot?.lens || state.lens);
  if (lens !== "work") return;
  const planId = (clusterNode.plan_id || "").trim();
  const plan = planId ? (snapshot.plans || []).find((p) => p && p.id === planId) : null;
  const expectedTotal = typeof plan?.task_counts?.total === "number" ? plan.task_counts.total : null;
  const presentTotal = planId
    ? (snapshot.tasks || []).filter((task) => (task?.plan_id || "").trim() === planId).length
    : 0;
  const expectedBounded =
    expectedTotal !== null ? Math.min(expectedTotal, GRAPH_LIMITS.maxTasksInPlan) : null;
  const likelyTruncated =
    expectedBounded !== null && expectedBounded > 0 && presentTotal < expectedBounded;
  if (!likelyTruncated) return;
  void loadMore(true);
}

function updateHud(snapshot) {
  if (!nodes.hudWhere || !nodes.hudLod || !nodes.hudFocus || !nodes.hudSelected) return;
  const workspace = (snapshot?.workspace || state.workspaceOverride || "").trim() || "auto";
  const projectLabel = (state.currentProjectLabel || "").trim() || "current";
  const where = `${projectLabel} · ${workspace}`;
  nodes.hudWhere.textContent = where;
  const lod = graphState.model?.lod || graphState.lod;
  nodes.hudLod.textContent = lodLabel(lod);

  const focus = snapshot?.focus || { kind: "none" };
  if (focus.kind === "none" || !focus.id) {
    nodes.hudFocus.textContent = "нет";
  } else if (focus.kind === "plan") {
    nodes.hudFocus.textContent = `цель ${focus.id}${focus.title ? ` — ${focus.title}` : ""}`;
  } else {
    nodes.hudFocus.textContent = `задача ${focus.id}${focus.title ? ` — ${focus.title}` : ""}`;
  }

  const selected =
    state.detailSelection?.kind === "task"
      ? `задача ${state.detailSelection.id}`
      : state.detailSelection?.kind === "plan"
        ? `цель ${state.detailSelection.id}`
        : state.detailSelection?.kind === "cluster"
          ? `кластер`
          : graphState.model?.selectedPlanId
            ? `цель ${graphState.model.selectedPlanId}`
            : "нет";
  const local = graphState.local?.centerId ? `local ${graphState.local.centerId}` : "";
  nodes.hudSelected.textContent = local ? `${selected} · ${local}` : selected;

  if (nodes.hudWarning) {
    const warning = graphState.model?.warning || null;
    if (warning) {
      nodes.hudWarning.hidden = false;
      nodes.hudWarning.textContent = warning;
    } else {
      nodes.hudWarning.hidden = true;
      nodes.hudWarning.textContent = "";
    }
  }
}

function renderGraphFlagship(snapshot) {
  const canvas = nodes.graph;
  if (!canvas) return;
  if (graphState.lodDebounceTimer) {
    window.clearTimeout(graphState.lodDebounceTimer);
    graphState.lodDebounceTimer = 0;
  }

  const rect = canvas.getBoundingClientRect();
  const width = Math.max(1, rect.width);
  const height = Math.max(1, rect.height);
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(width * ratio));
  canvas.height = Math.max(1, Math.floor(height * ratio));
  graphState.pixelRatio = ratio;

  // Preserve the world center when resizing.
  if (graphState.view && graphState.lastCanvasWidth && graphState.lastCanvasHeight) {
    const center = screenToWorld(graphState.view, graphState.lastCanvasWidth * 0.5, graphState.lastCanvasHeight * 0.5);
    graphState.view.offsetX = width * 0.5 - center.x * graphState.view.scale;
    graphState.view.offsetY = height * 0.5 - center.y * graphState.view.scale;
  }
  graphState.lastCanvasWidth = width;
  graphState.lastCanvasHeight = height;

  ensureGraphViewFlagship(width, height, snapshot);
  ensureLayoutRestore(snapshot);
  if (!graphState.view) return;

  const lodBefore = graphState.lod;
  graphState.lod = computeLodHysteresis(graphState.view.scale, lodBefore);

  const snapshotKey = graphDataKey(snapshot);
  const selectedPlanId = ensureSelectedPlanId(snapshot);
  const displayKey = `${snapshotKey}:${selectedPlanId || "none"}:${graphState.lod}`;
  const needsModel = !graphState.model || graphState.displayKey !== displayKey;

  if (needsModel) {
    const display = buildDisplayModelFlagship(snapshot, graphState.view, graphState.lod);
    mergeDisplayModelIntoCache(display);
    graphState.displayKey = displayKey;
    graphState.snapshotKey = snapshotKey;
    if (!graphState.view.restored) {
      // First run: fit to all plans in overview mode.
      fitViewToBounds(graphState.view, display.bounds, width, height, true);
      graphState.view.restored = true;
      graphState.lod = computeLod(graphState.view.scale);
    }
    startSettleAnimation();
  } else if (lodBefore !== graphState.lod) {
    // LOD changed via zoom: rebuild visible nodes, but keep positions.
    const display = buildDisplayModelFlagship(snapshot, graphState.view, graphState.lod);
    mergeDisplayModelIntoCache(display);
    graphState.displayKey = `${snapshotKey}:${selectedPlanId || "none"}:${graphState.lod}`;
    startSettleAnimation();
  }

  ensureGraphHandlersFlagship();
  refreshLocalGraph();
  updateHud(snapshot);
  drawGraphFlagship(snapshot, performance.now());
  drawMinimapFlagship(snapshot);
  scheduleSaveViewState(snapshot);
}

function ensureGraphHandlersFlagship() {
  if (graphState.handlersReady) return;
  const canvas = nodes.graph;
  if (!canvas) return;
  graphState.handlersReady = true;
  canvas.style.cursor = "grab";

  canvas.addEventListener("pointerdown", (event) => {
    if (UI_MODE !== "flagship") return;
    const view = graphState.view;
    const model = graphState.model;
    if (!view || !model) return;
    const rect = canvas.getBoundingClientRect();
    view.moved = false;
    view.dragging = true;
    view.lastX = event.clientX - rect.left;
    view.lastY = event.clientY - rect.top;
    canvas.setPointerCapture(event.pointerId);
    canvas.style.cursor = "grabbing";
  });

  canvas.addEventListener("pointermove", (event) => {
    if (UI_MODE !== "flagship") return;
    const view = graphState.view;
    const model = graphState.model;
    if (!view || !model) return;
    const rect = canvas.getBoundingClientRect();
    const localX = event.clientX - rect.left;
    const localY = event.clientY - rect.top;

    if (view.dragging) {
      const dx = localX - view.lastX;
      const dy = localY - view.lastY;
      view.offsetX += dx;
      view.offsetY += dy;
      view.lastX = localX;
      view.lastY = localY;
      view.moved = true;
      drawGraphFlagship(state.snapshot, performance.now());
      drawMinimapFlagship(state.snapshot);
      scheduleSaveViewState(state.snapshot);
      return;
    }

    const hit = hitTestNode(model, view, localX, localY);
    const nextHover = hit ? hit.id : null;
    if (nextHover !== graphState.hoverId) {
      graphState.hoverId = nextHover;
      drawGraphFlagship(state.snapshot, performance.now());
    }
    canvas.style.cursor = hit ? "pointer" : "grab";
  });

  canvas.addEventListener("pointerup", (event) => {
    if (UI_MODE !== "flagship") return;
    const view = graphState.view;
    const model = graphState.model;
    if (!view || !model) return;
    const rect = canvas.getBoundingClientRect();
    const localX = event.clientX - rect.left;
    const localY = event.clientY - rect.top;
    const moved = !!view.moved;
    view.dragging = false;
    canvas.style.cursor = "grab";

    if (!moved) {
      const hit = hitTestNode(model, view, localX, localY);
      if (hit && state.snapshot) {
        if (hit.kind === "plan") {
          state.selectedPlanId = hit.id;
          setDetailVisible(false);
          const camera = {
            offsetX: graphState.lastCanvasWidth * 0.5 - hit.x * 1.05,
            offsetY: graphState.lastCanvasHeight * 0.5 - hit.y * 1.05,
            scale: 1.05,
          };
          startCameraAnimation(camera, 420);
          const plan =
            (state.snapshot.plans || []).find((p) => p && p.id === hit.id) ||
            ({
              id: hit.id,
              title: hit.label,
              description: null,
              context: null,
              status: hit.status || "ACTIVE",
              priority: "MEDIUM",
              updated_at_ms: state.snapshot.generated_at_ms || 0,
              task_counts: hit.counts || { total: 0, done: 0, active: 0, backlog: 0, parked: 0 },
            });
          renderPlanDetail(state.snapshot, plan);
          renderGraphFlagship(state.snapshot);
          pushNavEntry({ camera });
          return;
        }
        if (hit.kind === "cluster") {
          state.selectedPlanId = hit.plan_id || state.selectedPlanId;
          setDetailVisible(false);
          const camera = {
            offsetX: graphState.lastCanvasWidth * 0.5 - hit.x * 1.45,
            offsetY: graphState.lastCanvasHeight * 0.5 - hit.y * 1.45,
            scale: 1.45,
          };
          startCameraAnimation(camera, 420);
          openClusterDetail(state.snapshot, hit);
          renderGraphFlagship(state.snapshot);
          pushNavEntry({ camera });
          return;
        }
        if (hit.kind === "task") {
          state.selectedPlanId = hit.plan_id || state.selectedPlanId;
          setDetailVisible(false);
          renderGraphFlagship(state.snapshot);
          const task = (state.snapshot.tasks || []).find((t) => t.id === hit.id);
          if (task) {
            renderTaskDetail(state.snapshot, task);
            pushNavEntry();
          }
        }
      }
    }
  });

  canvas.addEventListener("pointerleave", () => {
    if (UI_MODE !== "flagship") return;
    const view = graphState.view;
    if (view && view.dragging) return;
    graphState.hoverId = null;
    if (state.snapshot) {
      drawGraphFlagship(state.snapshot, performance.now());
    }
  });

  canvas.addEventListener(
    "wheel",
    (event) => {
      if (UI_MODE !== "flagship") return;
      const view = graphState.view;
      if (!view || !state.snapshot) return;
      const rect = canvas.getBoundingClientRect();
      event.preventDefault();
      const zoom = event.deltaY < 0 ? 1.1 : 0.91;
      const mouseX = event.clientX - rect.left;
      const mouseY = event.clientY - rect.top;
      const world = screenToWorld(view, mouseX, mouseY);
      view.scale = clamp(view.scale * zoom, 0.45, 2.2);
      view.offsetX = mouseX - world.x * view.scale;
      view.offsetY = mouseY - world.y * view.scale;
      const lodBefore = graphState.lod;
      const nextLod = computeLodHysteresis(view.scale, lodBefore);
      if (lodBefore !== nextLod) {
        graphState.lod = nextLod;
        // Premium semantic zoom: do not rebuild instantly on threshold crossings. Give the
        // gesture a brief window to “settle” so we avoid flicker when hovering near a boundary.
        scheduleLodDebouncedRebuild(state.snapshot);
      }
      drawGraphFlagship(state.snapshot, performance.now());
      drawMinimapFlagship(state.snapshot);
      updateHud(state.snapshot);
      scheduleSaveViewState(state.snapshot);
    },
    { passive: false }
  );

  canvas.addEventListener("dblclick", (event) => {
    if (UI_MODE !== "flagship") return;
    if (!graphState.view || !graphState.model) return;
    const rect = canvas.getBoundingClientRect();
    const localX = event.clientX - rect.left;
    const localY = event.clientY - rect.top;
    const hit = hitTestNode(graphState.model, graphState.view, localX, localY);
    if (hit) {
      toggleLocalGraph(hit.id);
      if (state.snapshot) {
        renderGraphFlagship(state.snapshot);
        pushNavEntry();
      }
      return;
    }
    // Double click on empty space: home.
    if (state.snapshot) {
      clearLocalGraph();
      const bounds = graphState.model.bounds || boundsOfNodes(graphState.model.nodes.filter((n) => n.kind === "plan"));
      fitViewToBounds(graphState.view, bounds, graphState.lastCanvasWidth, graphState.lastCanvasHeight, true);
      graphState.lod = computeLod(graphState.view.scale);
      renderGraphFlagship(state.snapshot);
      pushNavEntry();
    }
  });

  if (nodes.minimap) {
    nodes.minimap.addEventListener("click", (event) => {
      if (UI_MODE !== "flagship") return;
      const view = graphState.view;
      const model = graphState.model;
      if (!view || !model || !nodes.graph) return;
      const rect = nodes.minimap.getBoundingClientRect();
      const ratio = window.devicePixelRatio || 1;
      const x = (event.clientX - rect.left) * ratio;
      const y = (event.clientY - rect.top) * ratio;
      const w = rect.width * ratio;
      const h = rect.height * ratio;
      const plans = model.nodes.filter((n) => n.kind === "plan");
      const bounds = boundsOfNodes(plans);
      const pad = 10 * ratio;
      const bw = Math.max(1, bounds.maxX - bounds.minX);
      const bh = Math.max(1, bounds.maxY - bounds.minY);
      const scale = Math.min((w - pad * 2) / bw, (h - pad * 2) / bh);
      const worldX = bounds.minX + (x - pad) / scale;
      const worldY = bounds.minY + (y - pad) / scale;
      startCameraAnimation(
        {
          offsetX: graphState.lastCanvasWidth * 0.5 - worldX * view.scale,
          offsetY: graphState.lastCanvasHeight * 0.5 - worldY * view.scale,
          scale: view.scale,
        },
        360
      );
    });
  }
}

function renderGraph(snapshot) {
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

function render(snapshot) {
  state.snapshot = snapshot;
  const lensFromSnapshot = normalizeLens(snapshot?.lens || state.lens);
  if (lensFromSnapshot !== state.lens) {
    setLens(lensFromSnapshot);
  } else {
    applyLensCopy();
  }
  if (!state.selectedPlanId) {
    state.selectedPlanId = snapshot.primary_plan_id || snapshot.plans[0]?.id || null;
  }
  renderSummary(snapshot);
  renderGoals(snapshot);
  renderChecklist(snapshot);
  renderTasks(snapshot);
  if (UI_MODE === "legacy") {
    renderGraph(snapshot);
  } else {
    renderGraphFlagship(snapshot);
    if (state.navStack.length === 0) {
      pushNavEntry();
    }
  }
}

function renderError(payload) {
  const message = payload.error.message || "Не удалось загрузить снимок.";
  nodes.focus.textContent = "Ошибка Viewer";
  nodes.focusSub.textContent = payload.error.code;
  nodes.planBreakdown.textContent = payload.error.recovery || "Проверьте настройки сервера.";
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
  if (snapshotMutation.pending) {
    snapshotMutation.queued = true;
    return;
  }
  snapshotMutation.pending = true;
  try {
    const response = await fetchWithTimeout(
      workspaceUrlWithParams("/api/snapshot", { lens: state.lens || "work" }),
      { cache: "no-store" },
      7000
    );
    const payload = await response.json();
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
    render(mergeGraphOverlay(payload));
  } catch (err) {
    renderError({
      error: { code: "NETWORK_ERROR", message: "Snapshot unavailable." },
    });
  } finally {
    snapshotMutation.pending = false;
    if (snapshotMutation.queued) {
      snapshotMutation.queued = false;
      void loadSnapshot();
    }
  }
}

window.addEventListener("resize", () => {
  if (state.snapshot) {
    if (UI_MODE === "legacy") {
      renderGraph(state.snapshot);
    } else {
      renderGraphFlagship(state.snapshot);
    }
  }
  syncExplorerWindowDom({ persist: true, clamp: true });
  syncDetailWindowDom({ persist: true, clamp: true });
});

if (nodes.detailClose) {
  nodes.detailClose.addEventListener("click", () => closeDetailWindow());
}

if (nodes.detailPin) {
  nodes.detailPin.addEventListener("click", () => setDetailPinned(!windowUi.detail.pinned));
}

if (nodes.sidebarPin) {
  nodes.sidebarPin.addEventListener("click", () => setSidebarPinned(!windowUi.explorer.pinned));
}

if (nodes.sidebarPanel) {
  nodes.sidebarPanel.addEventListener(
    "pointerdown",
    () => {
      bringWindowToFront("explorer");
    },
    true
  );
  nodes.sidebarPanel.addEventListener("focusin", () => bringWindowToFront("explorer"));

  const handle = nodes.sidebarPanel.querySelector(".top");
  if (handle) {
    handle.addEventListener("pointerdown", (event) => startWindowDrag("explorer", nodes.sidebarPanel, event));
  }
}

if (nodes.detailPanel) {
  nodes.detailPanel.addEventListener(
    "pointerdown",
    () => {
      bringWindowToFront("detail");
    },
    true
  );
  nodes.detailPanel.addEventListener("focusin", () => bringWindowToFront("detail"));

  const handle = nodes.detailPanel.querySelector(".detail-head");
  if (handle) {
    handle.addEventListener("pointerdown", (event) => startWindowDrag("detail", nodes.detailPanel, event));
  }
}

window.addEventListener("pointermove", handleWindowDragMove, true);
window.addEventListener("pointerup", handleWindowDragEnd, true);
window.addEventListener("pointercancel", handleWindowDragEnd, true);

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
    applyExplorerWindowPreference({ defaultOpen: true });
    applyDetailWindowPreference({ defaultOpen: false });
    state.selectedPlanId = null;
    state.detailSelection = null;
    clearLocalGraph();
    clearGraphOverlay();
    state.navStack = [];
    state.navIndex = -1;
    updateNavButtons();
    setDetailVisible(false);
    renderProjectSelect();
    await loadWorkspaces();
    renderWorkspaceSelect(null);
    stopLiveEvents();
    loadAbout();
    await loadSnapshot();
    startLiveEvents();
  });
}

if (nodes.workspace) {
  nodes.workspace.addEventListener("change", async () => {
    const next = (nodes.workspace.value || "").trim();
    if (!next) {
      setWorkspaceOverride(null);
    } else {
      setWorkspaceOverride(next);
    }
    workspaceMutation.recoveredGuardMismatch = false;
    state.selectedPlanId = null;
    state.detailSelection = null;
    clearLocalGraph();
    clearGraphOverlay();
    state.navStack = [];
    state.navIndex = -1;
    updateNavButtons();
    setDetailVisible(false);
    stopLiveEvents();
    loadAbout();
    await loadSnapshot();
    startLiveEvents();
  });
}

if (nodes.lens) {
  nodes.lens.addEventListener("change", async () => {
    const next = (nodes.lens.value || "").trim();
    setLens(next);
    state.selectedPlanId = null;
    state.detailSelection = null;
    clearLocalGraph();
    clearGraphOverlay();
    graphState.view = null;
    graphState.model = null;
    graphState.displayKey = null;
    graphState.snapshotKey = 0;
    if (layoutCache.saveTimer) {
      window.clearTimeout(layoutCache.saveTimer);
      layoutCache.saveTimer = 0;
    }
    if (layoutCache.layoutSaveTimer) {
      window.clearTimeout(layoutCache.layoutSaveTimer);
      layoutCache.layoutSaveTimer = 0;
    }
    layoutCache.nodesById.clear();
    layoutCache.fade.clear();
    layoutCache.visibleIds.clear();
    layoutCache.renderIds.clear();
    state.navStack = [];
    state.navIndex = -1;
    updateNavButtons();
    setDetailVisible(false);
    await loadSnapshot();
  });
}

if (nodes.runnerAutostart) {
  nodes.runnerAutostart.addEventListener("change", async () => {
    if (autostartMutation.pending) return;
    autostartMutation.pending = true;
    const desired = nodes.runnerAutostart.checked;
    nodes.runnerAutostart.disabled = true;
    try {
      await postJson(`/api/settings/runner_autostart`, { enabled: desired });
    } catch (err) {
      if (state.snapshot) {
        nodes.runnerAutostart.checked = !!state.snapshot.runner?.autostart?.enabled;
      }
    } finally {
      autostartMutation.pending = false;
      nodes.runnerAutostart.disabled = false;
      loadSnapshot();
    }
  });
}

const paletteState = {
  open: false,
  items: [],
  index: 0,
  restoreFocusEl: null,
  background: null,
};

const paletteRemote = {
  timer: 0,
  seq: 0,
};

const PALETTE_CONST = {
  debounceMs: 140,
  remoteLimit: 90,
  remoteTimeoutMs: 3500,
};

function cancelPaletteRemoteSearch() {
  paletteRemote.seq += 1;
  if (paletteRemote.timer) {
    window.clearTimeout(paletteRemote.timer);
    paletteRemote.timer = 0;
  }
}

async function fetchPaletteRemoteItems(query, expectedSeq) {
  try {
    const q = (query || "").trim();
    if (!q) return null;
    const response = await fetchWithTimeout(
      workspaceUrlWithParams("/api/search", {
        q,
        lens: state.lens || "work",
        limit: PALETTE_CONST.remoteLimit,
      }),
      { cache: "no-store" },
      PALETTE_CONST.remoteTimeoutMs
    );
    const payload = await response.json();
    if (expectedSeq !== paletteRemote.seq) return null;
    if (!response.ok || ("error" in payload && payload.error)) return null;
    const items = payload?.items;
    if (!Array.isArray(items)) return null;
    return items;
  } catch {
    return null;
  }
}

function schedulePaletteRemoteSearch(query) {
  if (!state.snapshot) return;
  const q = (query || "").trim();

  cancelPaletteRemoteSearch();
  if (!q || q.length < 2) return;

  const seq = paletteRemote.seq;
  paletteRemote.timer = window.setTimeout(async () => {
    paletteRemote.timer = 0;
    if (!paletteIsOpen()) return;
    if (!state.snapshot) return;

    const items = await fetchPaletteRemoteItems(q, seq);
    if (!items) return;
    if (!paletteIsOpen()) return;
    if (!state.snapshot) return;

    paletteState.items = paletteRankAndSlice(state.snapshot, items, q);
    paletteState.index = 0;
    renderPaletteResults(paletteState.items);
  }, PALETTE_CONST.debounceMs);
}

function cameraSnapshot() {
  const view = graphState.view;
  if (!view) return null;
  return { offsetX: view.offsetX, offsetY: view.offsetY, scale: view.scale };
}

function selectionSnapshot() {
  const sel = state.detailSelection;
  const kind = (sel?.kind || "").trim();
  const id = (sel?.id || "").trim();
  if (!kind || !id) return null;
  return { kind, id };
}

function localSnapshot() {
  if (!localGraphIsActive()) return null;
  const centerId = (graphState.local?.centerId || "").trim();
  if (!centerId) return null;
  return { centerId, hops: graphState.local?.hops || 2 };
}

function navEntryEquals(a, b) {
  const aSel = a?.selection || null;
  const bSel = b?.selection || null;
  const aLocal = a?.local || null;
  const bLocal = b?.local || null;
  const aCam = a?.camera || null;
  const bCam = b?.camera || null;

  const camKey = (cam) => {
    if (!cam) return "";
    const rx = Math.round((cam.offsetX || 0) * 10) / 10;
    const ry = Math.round((cam.offsetY || 0) * 10) / 10;
    const rs = Math.round((cam.scale || 0) * 100) / 100;
    return `${rx},${ry},${rs}`;
  };

  return (
    (a?.selectedPlanId || null) === (b?.selectedPlanId || null) &&
    (aSel?.kind || null) === (bSel?.kind || null) &&
    (aSel?.id || null) === (bSel?.id || null) &&
    (aLocal?.centerId || null) === (bLocal?.centerId || null) &&
    (aLocal?.hops || null) === (bLocal?.hops || null) &&
    camKey(aCam) === camKey(bCam)
  );
}

function updateNavButtons() {
  const canBack = state.navIndex > 0;
  const canForward = state.navIndex >= 0 && state.navIndex < state.navStack.length - 1;
  if (nodes.btnBack) nodes.btnBack.disabled = !canBack;
  if (nodes.btnForward) nodes.btnForward.disabled = !canForward;
}

function pushNavEntry(override) {
  if (navMutation.applying) return;
  const base = {
    selectedPlanId: state.selectedPlanId || null,
    selection: selectionSnapshot(),
    local: localSnapshot(),
    camera: cameraSnapshot(),
  };
  const entry = { ...base, ...(override || {}) };

  const current = state.navStack[state.navIndex] || null;
  if (current && navEntryEquals(current, entry)) {
    updateNavButtons();
    return;
  }

  if (state.navIndex < state.navStack.length - 1) {
    state.navStack.splice(state.navIndex + 1);
  }
  state.navStack.push(entry);
  state.navIndex = state.navStack.length - 1;
  updateNavButtons();
}

function applyNavEntry(entry) {
  if (!entry || !state.snapshot) return;
  navMutation.applying = true;
  try {
    state.selectedPlanId = (entry.selectedPlanId || "").trim() || null;

    if (entry.local?.centerId) {
      graphState.local.centerId = entry.local.centerId;
      graphState.local.hops = entry.local.hops || 2;
      graphState.local.key = null;
    } else {
      clearLocalGraph();
    }

    setDetailVisible(false);
    state.detailSelection = null;

    renderGraphFlagship(state.snapshot);

    const sel = entry.selection || null;
    if (sel?.kind === "plan") {
      const plan = (state.snapshot.plans || []).find((p) => p && p.id === sel.id);
      if (plan) {
        renderPlanDetail(state.snapshot, plan);
      }
    } else if (sel?.kind === "task") {
      const task = (state.snapshot.tasks || []).find((t) => t && t.id === sel.id);
      if (task) {
        renderTaskDetail(state.snapshot, task);
      }
    } else if (sel?.kind === "cluster") {
      const cluster = layoutCache.nodesById.get(sel.id);
      if (cluster && cluster.kind === "cluster") {
        openClusterDetail(state.snapshot, cluster);
      }
    }

    if (entry.camera && graphState.view) {
      startCameraAnimation(entry.camera, 320);
    }
  } finally {
    navMutation.applying = false;
    updateNavButtons();
  }
}

function navBack() {
  if (state.navIndex <= 0) return;
  state.navIndex -= 1;
  applyNavEntry(state.navStack[state.navIndex]);
}

function navForward() {
  if (state.navIndex < 0 || state.navIndex >= state.navStack.length - 1) return;
  state.navIndex += 1;
  applyNavEntry(state.navStack[state.navIndex]);
}

function paletteModalTargets() {
  const targets = [];
  if (nodes.shell) targets.push(nodes.shell);
  if (nodes.detailPanel) targets.push(nodes.detailPanel);
  return targets;
}

function setPaletteBackgroundDisabled(disabled) {
  const desired = !!disabled;
  const targets = paletteModalTargets();

  if (desired) {
    try {
      const active = document.activeElement;
      paletteState.restoreFocusEl =
        active && typeof active === "object" && "focus" in active ? active : null;
    } catch {
      paletteState.restoreFocusEl = null;
    }

    paletteState.background = targets.map((el) => ({
      el,
      inert: !!el.inert,
      ariaHidden: el.getAttribute("aria-hidden"),
    }));

    targets.forEach((el) => {
      try {
        el.inert = true;
      } catch {
        // ignore inert failures
      }
      el.setAttribute("aria-hidden", "true");
    });
    return;
  }

  const background = Array.isArray(paletteState.background) ? paletteState.background : [];
  background.forEach((entry) => {
    if (!entry || !entry.el) return;
    const el = entry.el;
    try {
      el.inert = !!entry.inert;
    } catch {
      // ignore inert failures
    }
    if (entry.ariaHidden === null) {
      el.removeAttribute("aria-hidden");
    } else {
      el.setAttribute("aria-hidden", entry.ariaHidden);
    }
  });
  paletteState.background = null;
}

function restorePaletteFocus() {
  const el = paletteState.restoreFocusEl;
  paletteState.restoreFocusEl = null;

  window.setTimeout(() => {
    try {
      if (el && typeof el.focus === "function" && document.contains(el)) {
        el.focus();
        return;
      }
    } catch {
      // ignore
    }
    nodes.btnFocus?.focus?.();
  }, 0);
}

function paletteFocusableElements() {
  const panel = nodes.palettePanel || nodes.palette;
  if (!panel) return [];
  const candidates = Array.from(
    panel.querySelectorAll(
      "input, button, select, textarea, a[href], [tabindex]:not([tabindex='-1'])"
    )
  );
  return candidates.filter((el) => {
    if (!el || !(el instanceof HTMLElement)) return false;
    if (el.hasAttribute("disabled")) return false;
    if (el.getAttribute("aria-hidden") === "true") return false;
    return el.getClientRects().length > 0;
  });
}

function handlePaletteModalKeydown(event) {
  if (!paletteIsOpen() || !event) return;

  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    setPaletteOpen(false);
    return;
  }

  if (event.key !== "Tab") return;

  const focusables = paletteFocusableElements();
  const input = nodes.paletteInput;
  if (!focusables.length) {
    event.preventDefault();
    input?.focus?.();
    return;
  }

  const active = document.activeElement;
  const index = focusables.findIndex((el) => el === active);
  const last = focusables.length - 1;
  const backwards = !!event.shiftKey;

  let nextIndex = index;
  if (backwards) {
    nextIndex = index <= 0 ? last : index - 1;
  } else {
    nextIndex = index < 0 || index >= last ? 0 : index + 1;
  }

  event.preventDefault();
  focusables[nextIndex]?.focus?.();
}

function setPaletteOpen(open) {
  if (!nodes.palette || !nodes.paletteInput || !nodes.paletteResults) return;
  cancelPaletteRemoteSearch();
  const desired = !!open;
  if (desired === paletteState.open) {
    if (desired) {
      window.setTimeout(() => {
        nodes.paletteInput?.focus?.();
      }, 0);
    }
    return;
  }
  paletteState.open = desired;
  paletteState.items = [];
  paletteState.index = 0;

  if (desired) {
    setPaletteBackgroundDisabled(true);
    nodes.palette.hidden = false;
    nodes.palette.classList.add("is-open");
    nodes.palette.setAttribute("aria-hidden", "false");
    nodes.paletteInput.value = "";
    nodes.paletteResults.textContent = "";
    window.setTimeout(() => {
      nodes.paletteInput.focus();
    }, 0);
  } else {
    nodes.palette.hidden = true;
    nodes.palette.classList.remove("is-open");
    nodes.palette.setAttribute("aria-hidden", "true");
    setPaletteBackgroundDisabled(false);
    restorePaletteFocus();
  }
}

function paletteIsOpen() {
  return !!paletteState.open;
}

function paletteRankAndSlice(snapshot, items, query) {
  const q = (query || "").trim();
  const needle = q.toLowerCase();
  const selectedPlanId = state.selectedPlanId || snapshot.primary_plan_id || "";

  const scoreOf = (item) => {
    if (!needle) return 0;
    const id = (item.id || "").toLowerCase();
    const title = (item.title || "").toLowerCase();
    if (id === needle) return -10;
    if (id.startsWith(needle)) return -6;
    if (id.includes(needle)) return -4;
    if (title.includes(needle)) return -2;
    return 10;
  };

  const filtered = needle
    ? items.filter((item) => {
        const id = (item.id || "").toLowerCase();
        const title = (item.title || "").toLowerCase();
        return id.includes(needle) || title.includes(needle);
      })
    : items.slice();

  filtered.sort((a, b) => {
    const diff = scoreOf(a) - scoreOf(b);
    if (diff !== 0) return diff;
    const planBiasA = a.kind === "plan" || a.kind === "anchor" ? -1 : 0;
    const planBiasB = b.kind === "plan" || b.kind === "anchor" ? -1 : 0;
    if (planBiasA !== planBiasB) return planBiasA - planBiasB;
    const inPlanA = a.plan_id === selectedPlanId ? -1 : 0;
    const inPlanB = b.plan_id === selectedPlanId ? -1 : 0;
    if (inPlanA !== inPlanB) return inPlanA - inPlanB;
    return (a.id || "").localeCompare(b.id || "");
  });

  return filtered.slice(0, 10);
}

function paletteItemsFor(snapshot, query) {
  const isKnowledge = normalizeLens(snapshot?.lens || state.lens) === "knowledge";

  const items = [];
  (snapshot.plans || []).forEach((plan) => {
    if (!plan || !plan.id) return;
    items.push({
      kind: isKnowledge ? "anchor" : "plan",
      id: plan.id,
      title: plan.title || plan.id,
      plan_id: plan.id,
    });
  });
  (snapshot.tasks || []).forEach((task) => {
    if (!task || !task.id) return;
    items.push({
      kind: isKnowledge ? "knowledge_key" : "task",
      id: task.id,
      title: task.title || task.id,
      plan_id: task.plan_id || "",
    });
  });
  return paletteRankAndSlice(snapshot, items, query);
}

function renderPaletteResults(items) {
  if (!nodes.paletteResults) return;
  clear(nodes.paletteResults);

  if (!items || items.length === 0) {
    const empty = document.createElement("div");
    empty.className = "palette-hint";
    empty.textContent = "Ничего не найдено.";
    nodes.paletteResults.append(empty);
    return;
  }

  items.forEach((item, index) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "palette-item";
    if (index === paletteState.index) btn.classList.add("is-active");
    const title = document.createElement("div");
    title.className = "palette-item-title";
    title.textContent = item.title || item.id;
    const meta = document.createElement("div");
    meta.className = "palette-item-meta";
    const id = document.createElement("span");
    id.className = "badge dim";
    id.textContent = item.id;
    const kind = document.createElement("span");
    kind.className = "badge accent";
    switch (item.kind) {
      case "plan":
        kind.textContent = "PLAN";
        break;
      case "task":
        kind.textContent = "TASK";
        break;
      case "anchor":
        kind.textContent = "ANCHOR";
        break;
      case "knowledge_key":
        kind.textContent = "KEY";
        break;
      default:
        kind.textContent = ((item.kind || "ITEM") + "").toUpperCase();
        break;
    }
    meta.append(id, kind);
    btn.append(title, meta);
    btn.addEventListener("click", () => {
      setPaletteOpen(false);
      if (!state.snapshot) return;
      void jumpToPaletteItem(state.snapshot, item);
    });
    nodes.paletteResults.append(btn);
  });
}

function zoomFlagship(factor) {
  if (UI_MODE !== "flagship") return;
  const view = graphState.view;
  if (!view || !state.snapshot) return;
  const width = graphState.lastCanvasWidth || nodes.graph?.getBoundingClientRect().width || 1;
  const height = graphState.lastCanvasHeight || nodes.graph?.getBoundingClientRect().height || 1;
  const center = screenToWorld(view, width * 0.5, height * 0.5);
  view.scale = clamp(view.scale * factor, 0.45, 2.2);
  view.offsetX = width * 0.5 - center.x * view.scale;
  view.offsetY = height * 0.5 - center.y * view.scale;
  renderGraphFlagship(state.snapshot);
}

function homeFlagship() {
  if (UI_MODE !== "flagship") return;
  const view = graphState.view;
  const model = graphState.model;
  if (!view || !model || !state.snapshot) return;
  clearLocalGraph();
  const bounds =
    model.bounds || boundsOfNodes((model.nodes || []).filter((n) => n.kind === "plan"));
  fitViewToBounds(view, bounds, graphState.lastCanvasWidth, graphState.lastCanvasHeight, true);
  graphState.lod = computeLod(view.scale);
  renderGraphFlagship(state.snapshot);
  pushNavEntry();
}

function fitFlagship() {
  if (UI_MODE !== "flagship") return;
  const view = graphState.view;
  const model = graphState.model;
  if (!view || !model || !state.snapshot) return;
  const bounds =
    model.bounds || boundsOfNodes((model.nodes || []).filter((n) => n.kind === "plan"));
  fitViewToBounds(view, bounds, graphState.lastCanvasWidth, graphState.lastCanvasHeight, false);
  graphState.lod = computeLod(view.scale);
  renderGraphFlagship(state.snapshot);
  pushNavEntry();
}

function focusFlagship() {
  if (UI_MODE !== "flagship") return;
  const snapshot = state.snapshot;
  const view = graphState.view;
  if (!snapshot || !view) return;
  const focus = snapshot.focus || { kind: "none" };
  if (focus.kind === "none" || !focus.id) return;

  if (focus.kind === "plan") {
    state.selectedPlanId = focus.id;
    renderGraphFlagship(snapshot);
    const node = layoutCache.nodesById.get(focus.id);
    let camera = null;
    if (node) {
      camera = {
        offsetX: graphState.lastCanvasWidth * 0.5 - node.x * 1.05,
        offsetY: graphState.lastCanvasHeight * 0.5 - node.y * 1.05,
        scale: 1.05,
      };
      startCameraAnimation(camera, 420);
    }
    pushNavEntry(camera ? { camera } : null);
    return;
  }

  if (focus.kind === "task") {
    if (focus.plan_id) {
      state.selectedPlanId = focus.plan_id;
    }
    renderGraphFlagship(snapshot);
    const node = layoutCache.nodesById.get(focus.id);
    let camera = null;
    if (node) {
      camera = {
        offsetX: graphState.lastCanvasWidth * 0.5 - node.x * 1.45,
        offsetY: graphState.lastCanvasHeight * 0.5 - node.y * 1.45,
        scale: 1.45,
      };
      startCameraAnimation(camera, 420);
    }
    pushNavEntry(camera ? { camera } : null);
  }
}

function findMatch(snapshot, query) {
  const q = (query || "").trim();
  if (!q) return null;
  const raw = q.toUpperCase();

  if (/^PLAN-\d+$/.test(raw)) {
    const plan = (snapshot.plans || []).find((p) => (p.id || "").toUpperCase() === raw);
    if (plan) return { kind: "plan", id: plan.id };
  }
  if (/^TASK-\d+$/.test(raw)) {
    const task = (snapshot.tasks || []).find((t) => (t.id || "").toUpperCase() === raw);
    if (task) return { kind: "task", id: task.id, plan_id: task.plan_id };
  }

  // Generic id match (supports anchors / knowledge keys).
  const idNeedle = q.toLowerCase();
  const planById = (snapshot.plans || []).find((p) => ((p.id || "") + "").toLowerCase() === idNeedle);
  if (planById) return { kind: "plan", id: planById.id };
  const taskById = (snapshot.tasks || []).find((t) => ((t.id || "") + "").toLowerCase() === idNeedle);
  if (taskById) return { kind: "task", id: taskById.id, plan_id: taskById.plan_id };

  const needle = q.toLowerCase();
  const plan = (snapshot.plans || []).find((p) => ((p.title || "") + "").toLowerCase().includes(needle));
  if (plan) return { kind: "plan", id: plan.id };
  const task = (snapshot.tasks || []).find((t) => ((t.title || "") + "").toLowerCase().includes(needle));
  if (task) return { kind: "task", id: task.id, plan_id: task.plan_id };
  return null;
}

function searchFlagship(query) {
  if (UI_MODE !== "flagship") return;
  const snapshot = state.snapshot;
  if (!snapshot) return;
  const match = findMatch(snapshot, query);
  if (!match) return;

  if (match.kind === "plan") {
    state.selectedPlanId = match.id;
    setDetailVisible(false);
    renderGraphFlagship(snapshot);
    const node = layoutCache.nodesById.get(match.id);
    let camera = null;
    if (node) {
      camera = {
        offsetX: graphState.lastCanvasWidth * 0.5 - node.x * 1.05,
        offsetY: graphState.lastCanvasHeight * 0.5 - node.y * 1.05,
        scale: 1.05,
      };
      startCameraAnimation(camera, 420);
    }
    const plan = (snapshot.plans || []).find((p) => p && p.id === match.id);
    if (plan) {
      renderPlanDetail(snapshot, plan);
    }
    pushNavEntry(camera ? { camera } : null);
    return;
  }

  if (match.kind === "task") {
    if (match.plan_id) state.selectedPlanId = match.plan_id;
    renderGraphFlagship(snapshot);
    const node = layoutCache.nodesById.get(match.id);
    let camera = null;
    if (node) {
      camera = {
        offsetX: graphState.lastCanvasWidth * 0.5 - node.x * 1.45,
        offsetY: graphState.lastCanvasHeight * 0.5 - node.y * 1.45,
        scale: 1.45,
      };
      startCameraAnimation(camera, 420);
    }
    const task = (snapshot.tasks || []).find((t) => t.id === match.id);
    if (task) {
      renderTaskDetail(snapshot, task);
      pushNavEntry(camera ? { camera } : null);
    }
  }
}

async function jumpToPaletteItem(snapshot, item) {
  if (UI_MODE !== "flagship") return;
  if (!snapshot || !item) return;

  const id = (item.id || "").trim();
  if (!id) return;

  const inSnapshot =
    (snapshot.plans || []).some((p) => p && p.id === id) ||
    (snapshot.tasks || []).some((t) => t && t.id === id);
  if (inSnapshot) {
    searchFlagship(id);
    return;
  }

  const kind = (item.kind || "").trim();
  const lens = normalizeLens(snapshot?.lens || state.lens);

  // Hierarchical subgraph materialization:
  // - For plans: fetch a bounded plan-scoped page.
  // - For tasks: fetch a bounded local graph (root + cluster neighbors).
  // This keeps jumps usable even when /api/snapshot is truncated.
  if (lens === "work") {
    if ((kind === "plan" || id.startsWith("PLAN-")) && id.startsWith("PLAN-")) {
      clearLocalGraph();
      state.selectedPlanId = id;
      try {
        const payload = await fetchGraphPlan(id, { limit: GRAPH_LIMITS.maxTasksInPlan });
        ingestGraphOverlay(payload);
        const merged = mergeGraphOverlay(state.snapshot || snapshot);
        render(merged);
      } catch {
        // fall through to cheap fallback
      }
      if ((state.snapshot?.plans || []).some((p) => p && p.id === id)) {
        searchFlagship(id);
        return;
      }
    }

    if ((kind === "task" || id.startsWith("TASK-")) && id.startsWith("TASK-")) {
      try {
        const payload = await fetchGraphLocal(id, { hops: 2, limit: 240 });
        ingestGraphOverlay(payload);
        const merged = mergeGraphOverlay(state.snapshot || snapshot);
        render(merged);
      } catch {
        // fall through to cheap fallback
      }
      if ((state.snapshot?.tasks || []).some((t) => t && t.id === id)) {
        searchFlagship(id);
        return;
      }
    }
  }

  const currentSnapshot = state.snapshot || snapshot;

  if (kind === "plan" || kind === "anchor") {
    clearLocalGraph();
    state.selectedPlanId = id;
    renderGraphFlagship(currentSnapshot);

    const node = layoutCache.nodesById.get(id);
    let camera = null;
    if (node) {
      camera = {
        offsetX: graphState.lastCanvasWidth * 0.5 - node.x * 1.05,
        offsetY: graphState.lastCanvasHeight * 0.5 - node.y * 1.05,
        scale: 1.05,
      };
      startCameraAnimation(camera, 420);
    }

    const fallback = {
      id,
      title: item.title || id,
      description: null,
      context: null,
      status: item.anchor_status || "ACTIVE",
      priority: "MEDIUM",
      updated_at_ms: currentSnapshot.generated_at_ms || Date.now(),
      task_counts: { total: 0, done: 0, active: 0, backlog: 0, parked: 0 },
      kind: item.anchor_kind || "anchor",
      refs: [],
      aliases: [],
      parent_id: null,
      depends_on: [],
    };
    renderPlanDetail(currentSnapshot, fallback);
    pushNavEntry(camera ? { camera } : null);
    return;
  }

  if (kind === "task" || kind === "knowledge_key") {
    if (item.plan_id) state.selectedPlanId = item.plan_id;
    renderGraphFlagship(currentSnapshot);

    const node = layoutCache.nodesById.get(id);
    let camera = null;
    if (node) {
      camera = {
        offsetX: graphState.lastCanvasWidth * 0.5 - node.x * 1.45,
        offsetY: graphState.lastCanvasHeight * 0.5 - node.y * 1.45,
        scale: 1.45,
      };
      startCameraAnimation(camera, 420);
    }

    const key = item.key || item.title || id;
    const anchorId = item.anchor_id || item.plan_id || "";
    const cardId = item.card_id || item.context || null;
    const fallback = {
      id,
      plan_id: item.plan_id || "",
      title: item.title || id,
      description: null,
      context: cardId,
      status: "TODO",
      priority: "MEDIUM",
      blocked: false,
      updated_at_ms: currentSnapshot.generated_at_ms || Date.now(),
      parked_until_ts_ms: null,
      card_id: cardId,
      key,
      anchor_id: anchorId,
    };
    renderTaskDetail(currentSnapshot, fallback);
    pushNavEntry(camera ? { camera } : null);
  }
}

if (nodes.btnBack) nodes.btnBack.addEventListener("click", () => navBack());
if (nodes.btnForward) nodes.btnForward.addEventListener("click", () => navForward());
if (nodes.btnHome) nodes.btnHome.addEventListener("click", () => homeFlagship());
if (nodes.btnFit) nodes.btnFit.addEventListener("click", () => fitFlagship());
if (nodes.btnFocus) nodes.btnFocus.addEventListener("click", () => focusFlagship());
if (nodes.btnZoomIn) nodes.btnZoomIn.addEventListener("click", () => zoomFlagship(1.15));
if (nodes.btnZoomOut) nodes.btnZoomOut.addEventListener("click", () => zoomFlagship(0.87));
if (nodes.btnRefresh) nodes.btnRefresh.addEventListener("click", () => loadSnapshot());
if (nodes.btnExplorer)
  nodes.btnExplorer.addEventListener("click", () => {
    if (sidebarIsOpen()) {
      if (windowUi.explorer.pinned) {
        bringWindowToFront("explorer");
        return;
      }
      closeExplorerWindow();
      return;
    }
    setSidebarVisible(true);
  });
if (nodes.sidebarClose)
  nodes.sidebarClose.addEventListener("click", () => closeExplorerWindow());

if (nodes.graphSearch) {
  nodes.graphSearch.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      searchFlagship(nodes.graphSearch.value || "");
    }
  });
}

if (nodes.palette) {
  nodes.palette.addEventListener("click", (event) => {
    const target = event.target;
    const action = target?.dataset?.action || "";
    if (action === "palette-close") {
      setPaletteOpen(false);
    }
  });

  nodes.palette.addEventListener("keydown", handlePaletteModalKeydown, true);
}

window.addEventListener(
  "mousedown",
  (event) => {
    if (!sidebarIsOpen()) return;
    if (paletteIsOpen()) return;
    if (windowUi.explorer.pinned) return;

    const target = event.target;
    if (nodes.sidebarPanel && target && nodes.sidebarPanel.contains(target)) return;
    if (nodes.btnExplorer && target && nodes.btnExplorer.contains(target)) return;
    closeExplorerWindow({ focus: false });
  },
  true
);

if (nodes.paletteInput) {
  nodes.paletteInput.addEventListener("input", () => {
    if (!state.snapshot) return;
    const q = nodes.paletteInput.value || "";
    paletteState.items = paletteItemsFor(state.snapshot, q);
    paletteState.index = 0;
    renderPaletteResults(paletteState.items);
    schedulePaletteRemoteSearch(q);
  });

  nodes.paletteInput.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setPaletteOpen(false);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      paletteState.index = clamp(paletteState.index + 1, 0, Math.max(0, paletteState.items.length - 1));
      renderPaletteResults(paletteState.items);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      paletteState.index = clamp(paletteState.index - 1, 0, Math.max(0, paletteState.items.length - 1));
      renderPaletteResults(paletteState.items);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const item = paletteState.items[paletteState.index];
      if (!item) return;
      setPaletteOpen(false);
      if (!state.snapshot) return;
      void jumpToPaletteItem(state.snapshot, item);
    }
  });
}

window.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && !event.shiftKey && (event.key || "").toLowerCase() === "k") {
    event.preventDefault();
    setPaletteOpen(true);
    if (state.snapshot) {
      paletteState.items = paletteItemsFor(state.snapshot, "");
      paletteState.index = 0;
      renderPaletteResults(paletteState.items);
    }
    return;
  }

  if (event.key === "Escape") {
    if (paletteIsOpen()) {
      setPaletteOpen(false);
      return;
    }
    if (sidebarIsOpen() && !windowUi.explorer.pinned) {
      closeExplorerWindow();
      return;
    }
    if (detailIsOpen() && !windowUi.detail.pinned) {
      closeDetailWindow();
    }
  }
});

updateNavButtons();

function stopLiveEvents() {
  const source = liveMutation.source;
  liveMutation.source = null;
  liveMutation.open = false;
  liveMutation.lastEventId = null;
  if (!source) return;
  try {
    source.close();
  } catch {
    // ignore close failures
  }
}

function requestSnapshotRefresh() {
  const now = Date.now();
  const minIntervalMs = 700;
  const since = now - (snapshotMutation.lastRequestedAt || 0);
  const delay = since < minIntervalMs ? minIntervalMs - since : 120;
  if (snapshotMutation.timer) {
    window.clearTimeout(snapshotMutation.timer);
  }
  snapshotMutation.timer = window.setTimeout(() => {
    snapshotMutation.timer = 0;
    snapshotMutation.lastRequestedAt = Date.now();
    loadSnapshot();
  }, delay);
}

function startLiveEvents() {
  stopLiveEvents();
  if (typeof window.EventSource !== "function") {
    return;
  }
  const url = workspaceUrl("/api/events");
  let source = null;
  try {
    source = new EventSource(url);
  } catch {
    return;
  }

  liveMutation.source = source;

  source.addEventListener("ready", (event) => {
    liveMutation.open = true;
    liveMutation.failures = 0;
    if (event && event.lastEventId) {
      liveMutation.lastEventId = event.lastEventId;
    }
  });

  source.addEventListener("bm_event", (event) => {
    liveMutation.lastMessageAt = Date.now();
    if (event && event.lastEventId) {
      liveMutation.lastEventId = event.lastEventId;
    }
    requestSnapshotRefresh();
  });

  source.addEventListener("eof", (event) => {
    if (event && event.lastEventId) {
      liveMutation.lastEventId = event.lastEventId;
    }
    // Server budgets intentionally close the stream; EventSource will reconnect.
  });

  source.onerror = () => {
    liveMutation.open = false;
    liveMutation.failures += 1;
  };
}

async function boot() {
  await loadProjects();
  renderProjectSelect();
  applyExplorerWindowPreference({ defaultOpen: true });
  applyDetailWindowPreference({ defaultOpen: false });
  await loadWorkspaces();
  renderWorkspaceSelect(null);
  loadLensFromStorage();
  loadAbout();
  await loadSnapshot();
  startLiveEvents();
}

void boot();

async function refreshProjects() {
  if (projectsMutation.pending) return;
  projectsMutation.pending = true;
  const before = state.projectOverride;
  try {
    await loadProjects();
    renderProjectSelect();
    applyExplorerWindowPreference({ defaultOpen: true });
    applyDetailWindowPreference({ defaultOpen: false });
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
  if (liveMutation.open) return;
  loadSnapshot();
}, 20000);

window.setInterval(() => {
  if (document.visibilityState !== "visible") return;
  void refreshProjects();
}, 10000);

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    void refreshProjects();
    loadSnapshot();
    startLiveEvents();
  } else {
    stopLiveEvents();
  }
});
