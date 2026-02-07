/* ── Snapshot Store (data layer: projects, workspaces, snapshot) ── */

import { create } from "zustand";
import type {
  Snapshot,
  ProjectInfo,
  WorkspaceInfo,
  AboutInfo,
  KnowledgeSnapshot,
} from "../api/types";
import { getProjects, getWorkspaces, getAbout, getSnapshot, getKnowledgeSnapshot } from "../api/endpoints";

export interface SnapshotState {
  projects: ProjectInfo[];
  workspaces: WorkspaceInfo[];
  about: AboutInfo | null;
  snapshot: Snapshot | null;
  knowledgeSnapshot: KnowledgeSnapshot | null;

  project: string | undefined;
  workspace: string | undefined;
  lens: string;
  selectedPlanId: string | null;

  loading: boolean;
  error: string | null;
  lastRefreshMs: number;

  boot: () => Promise<void>;
  setProject: (project: string | undefined) => Promise<void>;
  setWorkspace: (workspace: string | undefined) => Promise<void>;
  setLens: (lens: string) => Promise<void>;
  setSelectedPlanId: (planId: string | null) => void;
  refresh: () => Promise<void>;
}

export const useSnapshotStore = create<SnapshotState>((set, get) => ({
  projects: [],
  workspaces: [],
  about: null,
  snapshot: null,
  knowledgeSnapshot: null,

  project: undefined,
  workspace: undefined,
  lens: "work",
  selectedPlanId: null,

  loading: false,
  error: null,
  lastRefreshMs: 0,

  boot: async () => {
    try {
      set({ loading: true, error: null });

      const projectsResp = await getProjects();
      set({ projects: projectsResp.projects });

      const workspacesResp = await getWorkspaces();
      // Auto-select recommended workspace if available
      const autoWorkspace = workspacesResp.workspace_recommended || workspacesResp.workspace_default;
      set({ workspaces: workspacesResp.workspaces, workspace: autoWorkspace });

      const about = await getAbout().catch(() => null);
      set({ about });

      const workspace = get().workspace;
      const lens = get().lens;

      if (lens === "knowledge") {
        const ks = await getKnowledgeSnapshot(undefined, workspace);
        set({ knowledgeSnapshot: ks, loading: false, lastRefreshMs: Date.now() });
      } else {
        const snapshot = await getSnapshot(undefined, workspace, lens);
        set({
          snapshot,
          loading: false,
          lastRefreshMs: Date.now(),
          selectedPlanId: snapshot.primary_plan_id ?? get().selectedPlanId,
        });
      }
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  setProject: async (project) => {
    set({ project, snapshot: null, knowledgeSnapshot: null, loading: true, error: null });
    try {
      const workspacesResp = await getWorkspaces(project);
      const autoWorkspace = workspacesResp.workspace_recommended || workspacesResp.workspace_default;
      set({ workspaces: workspacesResp.workspaces, workspace: autoWorkspace });

      const { workspace, lens } = get();
      if (lens === "knowledge") {
        const ks = await getKnowledgeSnapshot(project, workspace);
        set({ knowledgeSnapshot: ks, loading: false, lastRefreshMs: Date.now() });
      } else {
        const snapshot = await getSnapshot(project, workspace, lens);
        set({ snapshot, loading: false, lastRefreshMs: Date.now() });
      }
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  setWorkspace: async (workspace) => {
    set({ workspace, loading: true, error: null });
    try {
      const { project, lens } = get();
      if (lens === "knowledge") {
        const ks = await getKnowledgeSnapshot(project, workspace);
        set({ knowledgeSnapshot: ks, loading: false, lastRefreshMs: Date.now() });
      } else {
        const snapshot = await getSnapshot(project, workspace, lens);
        set({ snapshot, loading: false, lastRefreshMs: Date.now() });
      }
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  setLens: async (lens) => {
    set({ lens, loading: true, error: null });
    try {
      const { project, workspace } = get();
      if (lens === "knowledge") {
        const ks = await getKnowledgeSnapshot(project, workspace);
        set({ knowledgeSnapshot: ks, loading: false, lastRefreshMs: Date.now() });
      } else {
        const snapshot = await getSnapshot(project, workspace, lens);
        set({ snapshot, loading: false, lastRefreshMs: Date.now() });
      }
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  setSelectedPlanId: (planId) => set({ selectedPlanId: planId }),

  refresh: async () => {
    const { project, workspace, lens } = get();
    try {
      if (lens === "knowledge") {
        const ks = await getKnowledgeSnapshot(project, workspace);
        set({ knowledgeSnapshot: ks, lastRefreshMs: Date.now(), error: null });
      } else {
        const snapshot = await getSnapshot(project, workspace, lens);
        set({ snapshot, lastRefreshMs: Date.now(), error: null });
      }
    } catch (err) {
      set({ error: String(err) });
    }
  },
}));
