import { create } from "zustand";
import { viewerApi } from "@/api/viewer";
import type {
  DocEntryDto,
  GraphSliceDto,
  PlanDto,
  ProjectDto,
  ReasoningRefDto,
  StepDetailDto,
  StepListDto,
  TaskDto,
  TaskStepsSummaryDto,
  TaskSummaryDto,
} from "@/api/types";

export type CenterView = "graph" | "plan" | "notes" | "trace" | "knowledge";

type LoadStatus = "idle" | "loading" | "ready" | "error";

const LS = {
  active_view: "bm.viewer.active_view",
  storage_dir: "bm.viewer.storage_dir",
  workspace: "bm.viewer.workspace",
  task_id: "bm.viewer.task_id",
} as const;

function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function lsSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // ignore
  }
}

let notesPollTimer: number | null = null;
let tracePollTimer: number | null = null;
let graphPollTimer: number | null = null;

function stopPolling() {
  if (notesPollTimer != null) window.clearInterval(notesPollTimer);
  if (tracePollTimer != null) window.clearInterval(tracePollTimer);
  if (graphPollTimer != null) window.clearInterval(graphPollTimer);
  notesPollTimer = null;
  tracePollTimer = null;
  graphPollTimer = null;
}

function startPolling() {
  stopPolling();
  const pollDocs = async (kind: "notes" | "trace") => {
    const s = useStore.getState();
    if (!s.selected_storage_dir || !s.selected_workspace || !s.reasoning_ref) return;
    const doc = kind === "notes" ? s.reasoning_ref.notes_doc : s.reasoning_ref.trace_doc;
    const branch = s.reasoning_ref.branch;
    try {
      const slice = await viewerApi.docsShowTail({
        storage_dir: s.selected_storage_dir,
        workspace: s.selected_workspace,
        branch,
        doc,
        limit: 80,
      });
      const lastSeq = slice.entries.at(-1)?.seq ?? 0;
      useStore.setState((prev) => {
        const prevLast = kind === "notes" ? prev.notes_last_seq : prev.trace_last_seq;
        if (lastSeq === prevLast) return prev;
        if (kind === "notes") {
          return { ...prev, notes_entries: slice.entries, notes_last_seq: lastSeq };
        }
        return { ...prev, trace_entries: slice.entries, trace_last_seq: lastSeq };
      });
    } catch (err) {
      // polling errors are kept quiet; UI should show last known data
      void err;
    }
  };

  notesPollTimer = window.setInterval(() => void pollDocs("notes"), 1000);
  tracePollTimer = window.setInterval(() => void pollDocs("trace"), 1000);

  graphPollTimer = window.setInterval(() => {
    const s = useStore.getState();
    if (!s.selected_storage_dir || !s.selected_workspace || !s.reasoning_ref) return;
    void s.load_graph({ quiet: true });
  }, 2500);
}

export interface ViewerState {
  // UI
  active_view: CenterView;
  set_active_view: (view: CenterView) => void;
  command_palette_open: boolean;
  set_command_palette_open: (open: boolean) => void;
  knowledge_focus_card_id: string | null;
  set_knowledge_focus_card_id: (card_id: string | null) => void;
  knowledge_focus_anchor_id: string | null;
  set_knowledge_focus_anchor_id: (anchor_id: string | null) => void;

  // Discovery
  projects_status: LoadStatus;
  projects_error: string | null;
  projects: ProjectDto[];
  init: () => Promise<void>;
  scan_projects: () => Promise<void>;

  selected_storage_dir: string | null;
  selected_workspace: string | null;
  select_workspace: (storage_dir: string, workspace: string) => Promise<void>;

  // Tasks
  tasks_status: LoadStatus;
  tasks_error: string | null;
  tasks: TaskSummaryDto[];
  load_tasks: () => Promise<void>;

  selected_task_id: string | null;
  selected_task: TaskDto | null;
  selected_plan: PlanDto | null;
  reasoning_ref: ReasoningRefDto | null;
  select_task: (task_id: string) => Promise<void>;

  // Steps
  steps_status: LoadStatus;
  steps_error: string | null;
  steps: StepListDto[];
  steps_summary: TaskStepsSummaryDto | null;
  selected_step_id: string | null;
  selected_step: StepDetailDto | null;
  select_step: (step_id: string) => Promise<void>;

  // Docs
  notes_entries: DocEntryDto[];
  notes_last_seq: number;
  trace_entries: DocEntryDto[];
  trace_last_seq: number;
  load_docs_tail: (doc: "notes" | "trace") => Promise<void>;

  // Graph
  graph_status: LoadStatus;
  graph_error: string | null;
  graph_slice: GraphSliceDto | null;
  graph_selected_id: string | null;
  select_graph_node: (id: string | null) => void;
  load_graph: (opts?: { quiet?: boolean }) => Promise<void>;
}

export const useStore = create<ViewerState>((set, get) => ({
  // UI
  active_view: (lsGet(LS.active_view) as CenterView) || "graph",
  set_active_view: (view) => {
    set({ active_view: view });
    lsSet(LS.active_view, view);
  },
  command_palette_open: false,
  set_command_palette_open: (open) => set({ command_palette_open: open }),
  knowledge_focus_card_id: null,
  set_knowledge_focus_card_id: (card_id) => set({ knowledge_focus_card_id: card_id }),
  knowledge_focus_anchor_id: null,
  set_knowledge_focus_anchor_id: (anchor_id) => set({ knowledge_focus_anchor_id: anchor_id }),

  // Discovery
  projects_status: "idle",
  projects_error: null,
  projects: [],
  init: async () => {
    await get().scan_projects();

    const projects = get().projects;
    if (projects.length === 0) return;

    const savedStorageDir = lsGet(LS.storage_dir);
    const savedWorkspace = lsGet(LS.workspace);
    if (savedStorageDir && savedWorkspace) {
      const p = projects.find((x) => x.storage_dir === savedStorageDir);
      const hasWs = p?.workspaces.some((w) => w.workspace === savedWorkspace) ?? false;
      if (hasWs) {
        await get().select_workspace(savedStorageDir, savedWorkspace);
        return;
      }
    }

    const first = projects[0];
    const ws = first.workspaces[0]?.workspace;
    if (ws) {
      await get().select_workspace(first.storage_dir, ws);
    }
  },
  scan_projects: async () => {
    set({ projects_status: "loading", projects_error: null });
    try {
      const raw = await viewerApi.projectsScan();

      // Best-effort dedupe: scan roots may include multiple paths that canonicalize to the same store.
      // We prefer keeping the first instance and unioning workspaces by id.
      const byStorageDir = new Map<string, ProjectDto>();
      for (const p of raw) {
        const existing = byStorageDir.get(p.storage_dir);
        if (!existing) {
          byStorageDir.set(p.storage_dir, p);
          continue;
        }

        const wsById = new Map<string, (typeof p.workspaces)[number]>();
        for (const w of [...existing.workspaces, ...p.workspaces]) wsById.set(w.workspace, w);

        byStorageDir.set(p.storage_dir, {
          ...existing,
          ...p,
          workspaces: Array.from(wsById.values()).sort((a, b) =>
            a.workspace.localeCompare(b.workspace),
          ),
        });
      }

      const projects = Array.from(byStorageDir.values()).sort((a, b) =>
        a.display_name.localeCompare(b.display_name),
      );
      set({ projects, projects_status: "ready" });
    } catch (err) {
      set({ projects_status: "error", projects_error: String(err) });
    }
  },

  selected_storage_dir: null,
  selected_workspace: null,
  select_workspace: async (storage_dir, workspace) => {
    stopPolling();
    set({
      selected_storage_dir: storage_dir,
      selected_workspace: workspace,
      knowledge_focus_card_id: null,
      knowledge_focus_anchor_id: null,
      tasks: [],
      tasks_status: "idle",
      selected_task_id: null,
      selected_task: null,
      selected_plan: null,
      reasoning_ref: null,
      steps: [],
      steps_summary: null,
      selected_step_id: null,
      selected_step: null,
      notes_entries: [],
      notes_last_seq: 0,
      trace_entries: [],
      trace_last_seq: 0,
      graph_slice: null,
      graph_selected_id: null,
    });
    lsSet(LS.storage_dir, storage_dir);
    lsSet(LS.workspace, workspace);

    await get().load_tasks();

    // Prefer live focus if present (BranchMind typically keeps it set).
    try {
      const focus = await viewerApi.focusGet({ storage_dir, workspace });
      const savedTask = lsGet(LS.task_id);
      const toOpen = focus || savedTask;
      if (toOpen) await get().select_task(toOpen);
    } catch {
      // ignore focus errors; tasks still load
    }
  },

  // Tasks
  tasks_status: "idle",
  tasks_error: null,
  tasks: [],
  load_tasks: async () => {
    const { selected_storage_dir, selected_workspace } = get();
    if (!selected_storage_dir || !selected_workspace) return;
    set({ tasks_status: "loading", tasks_error: null });
    try {
      const tasks = await viewerApi.tasksList({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        limit: 300,
        offset: 0,
      });
      set({ tasks, tasks_status: "ready" });
    } catch (err) {
      set({ tasks_status: "error", tasks_error: String(err) });
    }
  },

  selected_task_id: null,
  selected_task: null,
  selected_plan: null,
  reasoning_ref: null,
  select_task: async (task_id) => {
    const { selected_storage_dir, selected_workspace } = get();
    if (!selected_storage_dir || !selected_workspace) return;

    stopPolling();
    lsSet(LS.task_id, task_id);
    set({
      selected_task_id: task_id,
      knowledge_focus_card_id: null,
      knowledge_focus_anchor_id: null,
      selected_task: null,
      selected_plan: null,
      reasoning_ref: null,
      steps: [],
      steps_summary: null,
      selected_step_id: null,
      selected_step: null,
      notes_entries: [],
      notes_last_seq: 0,
      trace_entries: [],
      trace_last_seq: 0,
      graph_slice: null,
      graph_selected_id: null,
    });

    // Main entities
    const task = await viewerApi.tasksGet({
      storage_dir: selected_storage_dir,
      workspace: selected_workspace,
      id: task_id,
    });
    if (!task) {
      set({ selected_task: null });
      return;
    }

    const [plan, steps, summary, reasoning] = await Promise.all([
      viewerApi.plansGet({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        id: task.parent_plan_id,
      }),
      viewerApi.stepsList({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        task_id: task.id,
        limit: 2000,
      }),
      viewerApi.taskStepsSummary({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        task_id: task.id,
      }),
      viewerApi.reasoningRefGet({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        id: task.id,
        kind: "task",
      }),
    ]);

    set({
      selected_task: task,
      selected_plan: plan,
      steps,
      steps_summary: summary,
      reasoning_ref: reasoning,
      steps_status: "ready",
    });

    // Warm caches (docs + graph) so the center view has content immediately.
    if (reasoning) {
      await Promise.all([get().load_docs_tail("notes"), get().load_docs_tail("trace"), get().load_graph()]);
      startPolling();
    }
  },

  // Steps
  steps_status: "idle",
  steps_error: null,
  steps: [],
  steps_summary: null,
  selected_step_id: null,
  selected_step: null,
  select_step: async (step_id) => {
    const { selected_storage_dir, selected_workspace, selected_task_id } = get();
    if (!selected_storage_dir || !selected_workspace || !selected_task_id) return;
    set({ selected_step_id: step_id, selected_step: null, steps_error: null });
    try {
      const detail = await viewerApi.stepsDetail({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        task_id: selected_task_id,
        selector: { step_id },
      });
      set({ selected_step: detail });
    } catch (err) {
      set({ steps_error: String(err) });
    }
  },

  // Docs
  notes_entries: [],
  notes_last_seq: 0,
  trace_entries: [],
  trace_last_seq: 0,
  load_docs_tail: async (doc) => {
    const { selected_storage_dir, selected_workspace, reasoning_ref } = get();
    if (!selected_storage_dir || !selected_workspace || !reasoning_ref) return;
    const docId = doc === "notes" ? reasoning_ref.notes_doc : reasoning_ref.trace_doc;
    try {
      const slice = await viewerApi.docsShowTail({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        branch: reasoning_ref.branch,
        doc: docId,
        limit: 80,
      });
      const lastSeq = slice.entries.at(-1)?.seq ?? 0;
      if (doc === "notes") {
        set({ notes_entries: slice.entries, notes_last_seq: lastSeq });
      } else {
        set({ trace_entries: slice.entries, trace_last_seq: lastSeq });
      }
    } catch (err) {
      void err;
    }
  },

  // Graph
  graph_status: "idle",
  graph_error: null,
  graph_slice: null,
  graph_selected_id: null,
  select_graph_node: (id) => set({ graph_selected_id: id }),
  load_graph: async (opts) => {
    const quiet = opts?.quiet ?? false;
    const { selected_storage_dir, selected_workspace, reasoning_ref } = get();
    if (!selected_storage_dir || !selected_workspace || !reasoning_ref) return;
    if (!quiet) set({ graph_status: "loading", graph_error: null });
    try {
      const slice = await viewerApi.graphQuery({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        branch: reasoning_ref.branch,
        doc: reasoning_ref.graph_doc,
        input: { limit: 200, include_edges: true, edges_limit: 700 },
      });
      set({ graph_slice: slice, graph_status: "ready" });
    } catch (err) {
      if (!quiet) set({ graph_status: "error", graph_error: String(err) });
    }
  },
}));
