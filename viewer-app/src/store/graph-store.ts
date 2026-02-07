/* ── Graph Store (graph model, view state, LOD) ── */

import { create } from "zustand";

export interface GraphNode {
  id: string;
  kind: "plan" | "task" | "cluster" | "knowledge";
  label: string;
  x: number;
  y: number;
  radius: number;
  color: string;
  status: string;
  planId: string | null;
  taskCount?: number;
  priority?: string | null;
  blocked?: boolean;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: "parent" | "similar" | "cluster";
  weight: number;
}

export type LODLevel = "overview" | "clusters" | "tasks";

export interface ViewState {
  offsetX: number;
  offsetY: number;
  scale: number;
}

export interface GraphState {
  nodes: GraphNode[];
  edges: GraphEdge[];
  nodesById: Map<string, GraphNode>;
  view: ViewState;
  lod: LODLevel;
  hoverId: string | null;
  selectedPlanId: string | null;
  canvasWidth: number;
  canvasHeight: number;
  snapshotKey: number;
  displayKey: string | null;

  setNodes: (nodes: GraphNode[]) => void;
  setEdges: (edges: GraphEdge[]) => void;
  setNodesAndEdges: (nodes: GraphNode[], edges: GraphEdge[]) => void;
  updateView: (partial: Partial<ViewState>) => void;
  setLod: (lod: LODLevel) => void;
  setBounds: (bounds: { w: number; h: number }) => void;
  setHoverId: (hoverId: string | null) => void;
  setSelectedPlanId: (selectedPlanId: string | null) => void;
  updateCanvas: (canvasWidth: number, canvasHeight: number) => void;
  setSnapshotKey: (snapshotKey: number) => void;
  setDisplayKey: (displayKey: string | null) => void;
}

export const useGraphStore = create<GraphState>((set) => ({
  nodes: [],
  edges: [],
  nodesById: new Map(),
  view: { offsetX: 0, offsetY: 0, scale: 1 },
  lod: "overview",
  hoverId: null,
  selectedPlanId: null,
  canvasWidth: 0,
  canvasHeight: 0,
  snapshotKey: 0,
  displayKey: null,

  setNodes: (nodes) => {
    const nodesById = new Map(nodes.map((n) => [n.id, n]));
    set({ nodes, nodesById });
  },
  setEdges: (edges) => set({ edges }),
  setNodesAndEdges: (nodes, edges) => {
    const nodesById = new Map(nodes.map((n) => [n.id, n]));
    set({ nodes, edges, nodesById });
  },
  updateView: (partial) => set((s) => ({ view: { ...s.view, ...partial } })),
  setLod: (lod) => set({ lod }),
  setBounds: (bounds) => set({ canvasWidth: bounds.w, canvasHeight: bounds.h }),
  setHoverId: (hoverId) => set({ hoverId }),
  setSelectedPlanId: (selectedPlanId) => set({ selectedPlanId }),
  updateCanvas: (canvasWidth, canvasHeight) => set({ canvasWidth, canvasHeight }),
  setSnapshotKey: (snapshotKey) => set({ snapshotKey }),
  setDisplayKey: (displayKey) => set({ displayKey }),
}));
