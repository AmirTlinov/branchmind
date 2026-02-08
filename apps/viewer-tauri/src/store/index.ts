import { create } from "zustand";
import { viewerApi } from "@/api/viewer";
import type {
  ArchitectureLensDto,
  ArchitectureProvenanceDto,
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
export type GraphMode = "architecture" | "reasoning";
export type ArchitectureMode = "combined" | "system" | "execution" | "reasoning" | "risk";
export type ArchitectureScopeKind = "workspace" | "plan" | "task";

type LoadStatus = "idle" | "loading" | "ready" | "error";

function graphSliceSignature(slice: GraphSliceDto | null): string {
  if (!slice) return "0:0:0:0";
  let maxSeq = 0;
  for (const n of slice.nodes) maxSeq = Math.max(maxSeq, n.last_seq);
  for (const e of slice.edges) maxSeq = Math.max(maxSeq, e.last_seq);
  return `${maxSeq}:${slice.nodes.length}:${slice.edges.length}:${slice.has_more ? 1 : 0}`;
}

const LS = {
  active_view: "bm.viewer.active_view",
  graph_mode: "bm.viewer.graph_mode",
  architecture_mode: "bm.viewer.architecture_mode",
  architecture_scope_kind: "bm.viewer.architecture_scope_kind",
  architecture_time_window: "bm.viewer.architecture_time_window",
  architecture_include_draft: "bm.viewer.architecture_include_draft",
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

function lsGetBool(key: string, fallback = false): boolean {
  const raw = lsGet(key);
  if (raw == null) return fallback;
  return raw === "1" || raw.toLowerCase() === "true";
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
    if (!s.selected_storage_dir || !s.selected_workspace) return;
    if (s.graph_mode === "architecture") {
      void s.load_architecture_lens({ quiet: true });
      return;
    }
    if (!s.reasoning_ref) return;
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
  task_step_summaries: Map<string, TaskStepsSummaryDto>;
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
  graph_mode: GraphMode;
  set_graph_mode: (mode: GraphMode) => void;
  graph_status: LoadStatus;
  graph_error: string | null;
  graph_slice: GraphSliceDto | null;
  graph_selected_id: string | null;
  select_graph_node: (id: string | null) => void;
  load_graph: (opts?: { quiet?: boolean }) => Promise<void>;

  architecture_mode: ArchitectureMode;
  set_architecture_mode: (mode: ArchitectureMode) => void;
  architecture_scope_kind: ArchitectureScopeKind;
  set_architecture_scope_kind: (kind: ArchitectureScopeKind) => void;
  architecture_time_window: "all" | "24h" | "7d";
  set_architecture_time_window: (window: "all" | "24h" | "7d") => void;
  architecture_include_draft: boolean;
  set_architecture_include_draft: (include: boolean) => void;
  architecture_status: LoadStatus;
  architecture_error: string | null;
  architecture_lens: ArchitectureLensDto | null;
  architecture_provenance_status: LoadStatus;
  architecture_provenance_error: string | null;
  architecture_provenance: ArchitectureProvenanceDto | null;
  load_architecture_lens: (opts?: { quiet?: boolean }) => Promise<void>;
  load_architecture_provenance: (node_id: string) => Promise<void>;
}

function resolveArchitectureScope(state: Pick<
  ViewerState,
  "architecture_scope_kind" | "selected_task_id" | "selected_plan"
>): { kind: ArchitectureScopeKind | "workspace"; id?: string } {
  if (state.architecture_scope_kind === "task" && state.selected_task_id) {
    return { kind: "task", id: state.selected_task_id };
  }
  if (state.architecture_scope_kind === "plan" && state.selected_plan?.id) {
    return { kind: "plan", id: state.selected_plan.id };
  }
  if (state.architecture_scope_kind === "workspace") {
    return { kind: "workspace" };
  }
  if (state.selected_task_id) {
    return { kind: "task", id: state.selected_task_id };
  }
  if (state.selected_plan?.id) {
    return { kind: "plan", id: state.selected_plan.id };
  }
  return { kind: "workspace" };
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

  // Graph mode
  graph_mode: (lsGet(LS.graph_mode) as GraphMode) || "architecture",
  set_graph_mode: (mode) => {
    set({ graph_mode: mode });
    lsSet(LS.graph_mode, mode);
    const s = get();
    if (!s.selected_storage_dir || !s.selected_workspace) return;
    if (mode === "architecture") {
      void s.load_architecture_lens();
      return;
    }
    if (s.reasoning_ref) {
      void s.load_graph();
    }
  },

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
      task_step_summaries: new Map(),
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
      architecture_status: "idle",
      architecture_error: null,
      architecture_lens: null,
      architecture_provenance_status: "idle",
      architecture_provenance_error: null,
      architecture_provenance: null,
    });
    lsSet(LS.storage_dir, storage_dir);
    lsSet(LS.workspace, workspace);

    await get().load_tasks();

    // Prefer live focus if present (BranchMind typically keeps it set).
    let openedTask = false;
    try {
      const focus = await viewerApi.focusGet({ storage_dir, workspace });
      const savedTask = lsGet(LS.task_id);
      const toOpen = focus || savedTask;
      if (toOpen) {
        openedTask = true;
        await get().select_task(toOpen);
      }
    } catch {
      // ignore focus errors; tasks still load
    }
    if (!openedTask) {
      await get().load_architecture_lens();
      startPolling();
    }
  },

  // Tasks
  tasks_status: "idle",
  tasks_error: null,
  tasks: [],
  task_step_summaries: new Map(),
  load_tasks: async () => {
    const { selected_storage_dir, selected_workspace } = get();
    if (!selected_storage_dir || !selected_workspace) return;
    set({ tasks_status: "loading", tasks_error: null, task_step_summaries: new Map() });
    try {
      const tasks = await viewerApi.tasksList({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        limit: 300,
        offset: 0,
      });
      set({ tasks, tasks_status: "ready" });

      // Lazy batch-fetch step summaries for all visible tasks (non-blocking).
      void Promise.allSettled(
        tasks.map((t) =>
          viewerApi
            .taskStepsSummary({
              storage_dir: selected_storage_dir,
              workspace: selected_workspace,
              task_id: t.id,
            })
            .then((summary) => {
              set((prev) => {
                const next = new Map(prev.task_step_summaries);
                next.set(t.id, summary);
                return { task_step_summaries: next };
              });
            }),
        ),
      );
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
      architecture_status: "idle",
      architecture_error: null,
      architecture_lens: null,
      architecture_provenance_status: "idle",
      architecture_provenance_error: null,
      architecture_provenance: null,
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

    // Warm architecture lens so graph view can explain "what/why/how" immediately.
    await get().load_architecture_lens();

    // Warm reasoning caches (docs + graph) when reasoning refs are available.
    if (reasoning) {
      await Promise.all([get().load_docs_tail("notes"), get().load_docs_tail("trace"), get().load_graph()]);
    }
    startPolling();
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
  architecture_mode: (lsGet(LS.architecture_mode) as ArchitectureMode) || "combined",
  set_architecture_mode: (mode) => {
    set({ architecture_mode: mode });
    lsSet(LS.architecture_mode, mode);
    const s = get();
    if (s.selected_storage_dir && s.selected_workspace) {
      void s.load_architecture_lens();
    }
  },
  architecture_scope_kind: (lsGet(LS.architecture_scope_kind) as ArchitectureScopeKind) || "task",
  set_architecture_scope_kind: (kind) => {
    set({ architecture_scope_kind: kind });
    lsSet(LS.architecture_scope_kind, kind);
    const s = get();
    if (s.selected_storage_dir && s.selected_workspace) {
      void s.load_architecture_lens();
    }
  },
  architecture_time_window:
    ((lsGet(LS.architecture_time_window) as "all" | "24h" | "7d") || "all"),
  set_architecture_time_window: (window) => {
    set({ architecture_time_window: window });
    lsSet(LS.architecture_time_window, window);
    const s = get();
    if (s.selected_storage_dir && s.selected_workspace) {
      void s.load_architecture_lens();
    }
  },
  architecture_include_draft: lsGetBool(LS.architecture_include_draft, false),
  set_architecture_include_draft: (include) => {
    set({ architecture_include_draft: include });
    lsSet(LS.architecture_include_draft, include ? "1" : "0");
    const s = get();
    if (s.selected_storage_dir && s.selected_workspace) {
      void s.load_architecture_lens();
    }
  },
  architecture_status: "idle",
  architecture_error: null,
  architecture_lens: null,
  architecture_provenance_status: "idle",
  architecture_provenance_error: null,
  architecture_provenance: null,
  graph_status: "idle",
  graph_error: null,
  graph_slice: null,
  graph_selected_id: null,
  select_graph_node: (id) => {
    set({ graph_selected_id: id });
    const s = get();
    if (s.graph_mode === "architecture" && id) {
      void s.load_architecture_provenance(id);
    } else {
      set({
        architecture_provenance: null,
        architecture_provenance_error: null,
        architecture_provenance_status: "idle",
      });
    }
  },
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
      set((prev) => {
        // Avoid "glitchy" graph resets: polling can return identical graph slices but with new object
        // identities. We only update state when the slice meaningfully changes.
        const prevSig = graphSliceSignature(prev.graph_slice);
        const nextSig = graphSliceSignature(slice);
        if (prev.graph_slice && prevSig === nextSig) {
          return { graph_status: "ready" };
        }
        return { graph_slice: slice, graph_status: "ready" };
      });
    } catch (err) {
      if (!quiet) set({ graph_status: "error", graph_error: String(err) });
    }
  },
  load_architecture_lens: async (opts) => {
    const quiet = opts?.quiet ?? false;
    const {
      selected_storage_dir,
      selected_workspace,
      architecture_mode,
      architecture_include_draft,
      architecture_time_window,
      graph_selected_id,
    } = get();
    if (!selected_storage_dir || !selected_workspace) return;
    if (!quiet) set({ architecture_status: "loading", architecture_error: null });
    try {
      const scope = resolveArchitectureScope(get());
      const lens = await viewerApi.architectureLensGet({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        input: {
          scope,
          mode: architecture_mode,
          include_draft: architecture_include_draft,
          time_window: architecture_time_window,
          limit: 220,
        },
      });
      const hasSelected = !!graph_selected_id && lens.nodes.some((n) => n.id === graph_selected_id);
      const selected = hasSelected ? graph_selected_id : null;
      const prevProvenance = get().architecture_provenance;
      const keepProvenance =
        selected && prevProvenance?.node_id === selected ? prevProvenance : null;
      set({
        architecture_lens: lens,
        architecture_status: "ready",
        architecture_error: null,
        graph_selected_id: selected,
        architecture_provenance: keepProvenance,
        architecture_provenance_error: null,
        architecture_provenance_status: keepProvenance ? "ready" : "idle",
      });
      if (selected && get().graph_mode === "architecture" && !keepProvenance) {
        void get().load_architecture_provenance(selected);
      }
    } catch (err) {
      if (!quiet) set({ architecture_status: "error", architecture_error: String(err) });
    }
  },
  load_architecture_provenance: async (node_id) => {
    const {
      selected_storage_dir,
      selected_workspace,
      architecture_include_draft,
      architecture_time_window,
      architecture_provenance,
    } = get();
    if (!selected_storage_dir || !selected_workspace) return;
    const cleanNodeId = node_id.trim();
    if (!cleanNodeId) return;
    if (architecture_provenance?.node_id === cleanNodeId) return;
    set({
      architecture_provenance_status: "loading",
      architecture_provenance_error: null,
    });
    try {
      const scope = resolveArchitectureScope(get());
      const data = await viewerApi.architectureProvenanceGet({
        storage_dir: selected_storage_dir,
        workspace: selected_workspace,
        input: {
          scope,
          node_id: cleanNodeId,
          include_draft: architecture_include_draft,
          time_window: architecture_time_window,
          limit: 80,
        },
      });
      set({
        architecture_provenance: data,
        architecture_provenance_status: "ready",
      });
    } catch (err) {
      set({
        architecture_provenance_status: "error",
        architecture_provenance_error: String(err),
      });
    }
  },
}));
