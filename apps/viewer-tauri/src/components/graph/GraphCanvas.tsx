import { useCallback, useEffect, useMemo, useRef } from "react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { useViewport } from "./useViewport";
import { useForceLayout } from "./useForceLayout";
import { GraphNode } from "./GraphNode";
import { Minimap } from "./Minimap";
import { EmptyState } from "@/components/ui/EmptyState";
import { Skeleton } from "@/components/ui/Skeleton";
import { Activity, AlertTriangle, Minus, Plus, Scan } from "lucide-react";
import type { GraphEdgeDto, GraphNodeDto } from "@/api/types";

const MAX_CANVAS_DPR = 1.5;
const MAX_CANVAS_DPR_INTERACT = 1.0;
const MIN_CANVAS_DPR = 0.7;
// Cap canvas backing-store pixels to keep 4K fullscreen smooth.
// (Clearing a 13M+ px 2D canvas every frame is a common source of jank.)
const MAX_CANVAS_PIXELS_IDLE = 10_000_000;
const MAX_CANVAS_PIXELS_INTERACT = 4_000_000;
const EDGE_LOW_DETAIL_THRESHOLD = 700;
const EDGE_VERY_LOW_DETAIL_THRESHOLD = 1400;

type EdgeDraw = {
  from: string;
  to: string;
  rel: string;
  relLower: string;
  risk: boolean;
  weight: number;
};

function parseEdgeMeta(edge: GraphEdgeDto): { risk: boolean; weight: number } {
  if (!edge.meta_json) return { risk: false, weight: 1 };
  try {
    const raw = JSON.parse(edge.meta_json) as { risk?: boolean; weight?: number };
    return {
      risk: !!raw.risk,
      weight: typeof raw.weight === "number" && raw.weight > 0 ? raw.weight : 1,
    };
  } catch {
    return { risk: false, weight: 1 };
  }
}

function edgeColor(relLower: string, highlighted: boolean, risk: boolean): string {
  if (risk) return highlighted ? "rgba(244,63,94,0.88)" : "rgba(244,63,94,0.26)";
  const r = relLower;
  if (r === "blocks") return highlighted ? "rgba(244,63,94,0.85)" : "rgba(244,63,94,0.22)";
  if (r === "supports" || r === "knows")
    return highlighted ? "rgba(34,197,94,0.85)" : "rgba(34,197,94,0.22)";
  if (r === "touches" || r === "refines")
    return highlighted ? "rgba(139,92,246,0.85)" : "rgba(139,92,246,0.22)";
  if (r === "tests") return highlighted ? "rgba(168,85,247,0.85)" : "rgba(168,85,247,0.22)";
  if (r === "justifies") return highlighted ? "rgba(14,165,233,0.85)" : "rgba(14,165,233,0.22)";
  if (r === "updates") return highlighted ? "rgba(245,158,11,0.85)" : "rgba(245,158,11,0.22)";
  if (r === "invokes" || r === "reads_writes" || r === "persists" || r === "delegates") {
    return highlighted ? "rgba(17,24,39,0.52)" : "rgba(17,24,39,0.18)";
  }
  if (r === "read_only") return highlighted ? "rgba(75,85,99,0.6)" : "rgba(75,85,99,0.2)";
  if (r === "contains") return highlighted ? "rgba(17,24,39,0.35)" : "rgba(17,24,39,0.08)";
  return highlighted ? "rgba(17,24,39,0.35)" : "rgba(17,24,39,0.08)";
}

interface DrawArgs {
  canvas: HTMLCanvasElement;
  nodes: Map<string, { _x: number; _y: number }>;
  edges: EdgeDraw[];
  focusId: string | null;
  interacting: boolean;
  viewX: number;
  viewY: number;
  scale: number;
  cw: number;
  ch: number;
}

function drawEdges(args: DrawArgs) {
  const { canvas, nodes, edges, focusId, interacting, viewX, viewY, scale, cw, ch } = args;
  if (cw < 2 || ch < 2) return;
  const baseDpr = window.devicePixelRatio || 1;
  const maxDpr = interacting ? MAX_CANVAS_DPR_INTERACT : MAX_CANVAS_DPR;
  const areaCap = interacting ? MAX_CANVAS_PIXELS_INTERACT : MAX_CANVAS_PIXELS_IDLE;
  const area = Math.max(1, cw * ch);
  const areaDpr = Math.sqrt(areaCap / area);
  const dpr = Math.max(MIN_CANVAS_DPR, Math.min(baseDpr, maxDpr, areaDpr));
  const pxW = Math.max(1, Math.round(cw * dpr));
  const pxH = Math.max(1, Math.round(ch * dpr));
  if (canvas.width !== pxW || canvas.height !== pxH) {
    canvas.width = pxW;
    canvas.height = pxH;
    canvas.style.width = `${cw}px`;
    canvas.style.height = `${ch}px`;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  // Use exact pixel ratio after rounding to avoid canvas thrash on fractional sizes.
  const sx = pxW / cw;
  const sy = pxH / ch;
  ctx.setTransform(sx, 0, 0, sy, 0, 0);
  ctx.clearRect(0, 0, cw, ch);
  const toScreen = (wx: number, wy: number): [number, number] => [wx * scale + viewX, wy * scale + viewY];
  const lowDetail = interacting || edges.length >= EDGE_LOW_DETAIL_THRESHOLD || scale < 0.42;
  const veryLowDetail =
    (!focusId && interacting) || (edges.length >= EDGE_VERY_LOW_DETAIL_THRESHOLD && !focusId);

  for (let edgeIndex = 0; edgeIndex < edges.length; edgeIndex++) {
    const edge = edges[edgeIndex];
    const from = nodes.get(edge.from);
    const to = nodes.get(edge.to);
    if (!from || !to) continue;

    const [sx1, sy1] = toScreen(from._x, from._y);
    const [sx2, sy2] = toScreen(to._x, to._y);

    if (
      (sx1 < -120 && sx2 < -120) ||
      (sx1 > cw + 120 && sx2 > cw + 120) ||
      (sy1 < -120 && sy2 < -120) ||
      (sy1 > ch + 120 && sy2 > ch + 120)
    ) {
      continue;
    }

    const highlighted = !!focusId && (edge.from === focusId || edge.to === focusId);
    if (veryLowDetail && !highlighted && edgeIndex % 2 === 1) continue;
    const dimmed = !!focusId && !highlighted;
    const { risk, weight } = edge;
    const dx = sx2 - sx1;
    const dy = sy2 - sy1;
    const dist = Math.sqrt(dx * dx + dy * dy) || 1;
    if (lowDetail) {
      ctx.beginPath();
      ctx.moveTo(sx1, sy1);
      ctx.lineTo(sx2, sy2);
      ctx.strokeStyle = edgeColor(edge.relLower, highlighted, risk);
      ctx.lineWidth = highlighted ? 1.7 : 0.95;
      ctx.globalAlpha = dimmed ? 0.08 : 0.72;
      ctx.stroke();
      ctx.globalAlpha = 1;
      continue;
    }

    const curvature = Math.min(dist * 0.18, 56);
    const mx = (sx1 + sx2) / 2;
    const my = (sy1 + sy2) / 2;
    const nx = -dy / dist;
    const ny = dx / dist;
    const cpx = mx + nx * curvature * 0.35;
    const cpy = my + ny * curvature * 0.35;

    ctx.beginPath();
    ctx.moveTo(sx1, sy1);
    ctx.quadraticCurveTo(cpx, cpy, sx2, sy2);
    ctx.strokeStyle = edgeColor(edge.relLower, highlighted, risk);
    const base = highlighted ? 1.8 : 1.0;
    ctx.lineWidth = base + Math.min(weight, 6) * 0.22;
    ctx.globalAlpha = dimmed ? 0.08 : 1;
    ctx.setLineDash(edge.relLower === "contains" ? [] : [5, 4]);
    ctx.stroke();
    ctx.setLineDash([]);

    const arrowLen = highlighted ? 9 : 7;
    const arrowW = highlighted ? 3.2 : 2.6;
    const ax = sx2 - cpx;
    const ay = sy2 - cpy;
    const al = Math.sqrt(ax * ax + ay * ay) || 1;
    const ux = ax / al;
    const uy = ay / al;
    ctx.beginPath();
    ctx.moveTo(sx2, sy2);
    ctx.lineTo(sx2 - ux * arrowLen + uy * arrowW, sy2 - uy * arrowLen - ux * arrowW);
    ctx.lineTo(sx2 - ux * arrowLen - uy * arrowW, sy2 - uy * arrowLen + ux * arrowW);
    ctx.closePath();
    ctx.fillStyle = edgeColor(edge.relLower, highlighted, risk);
    ctx.globalAlpha = dimmed ? 0.08 : 0.6;
    ctx.fill();
    ctx.globalAlpha = 1;

    if (highlighted) {
      ctx.font = "9px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillStyle = edgeColor(edge.relLower, true, risk);
      ctx.globalAlpha = 0.9;
      const label = weight > 1 ? `${edge.rel} ×${weight}` : edge.rel;
      ctx.fillText(label, cpx, cpy - 6);
      ctx.globalAlpha = 1;
    }
  }
}

function architectureNodeToGraphNode(
  node: {
    id: string;
    label: string;
    node_type: string;
    status?: string | null;
    tags: string[];
    layer: string;
    cluster_id: string;
    risk_score: number;
    evidence_score: number;
    last_ts_ms: number;
    refs: string[];
  },
  idx: number,
): GraphNodeDto {
  return {
    id: node.id,
    node_type: node.node_type,
    title: node.label,
    text: null,
    tags: node.tags,
    status: node.status ?? null,
    meta_json: JSON.stringify({
      layer: node.layer,
      cluster_id: node.cluster_id,
      risk_score: node.risk_score,
      evidence_score: node.evidence_score,
      refs: node.refs,
    }),
    deleted: false,
    last_seq: idx + 1,
    last_ts_ms: node.last_ts_ms,
  };
}

function architectureEdgeToGraphEdge(
  edge: { from: string; to: string; rel: string; weight: number; risk: boolean },
  idx: number,
  ts: number,
): GraphEdgeDto {
  return {
    from: edge.from,
    to: edge.to,
    rel: edge.rel,
    meta_json: JSON.stringify({ weight: edge.weight, risk: edge.risk }),
    deleted: false,
    last_seq: idx + 1,
    last_ts_ms: ts,
  };
}

const ARCHITECTURE_MODES = ["combined", "system", "execution", "reasoning", "risk"] as const;

export function GraphCanvas() {
  const selected_workspace = useStore((s) => s.selected_workspace);
  const selected_task_id = useStore((s) => s.selected_task_id);
  const selected_plan = useStore((s) => s.selected_plan);
  const reasoning_ref = useStore((s) => s.reasoning_ref);

  const graph_mode = useStore((s) => s.graph_mode);
  const set_graph_mode = useStore((s) => s.set_graph_mode);

  const graph_status = useStore((s) => s.graph_status);
  const graph_error = useStore((s) => s.graph_error);
  const graph_slice = useStore((s) => s.graph_slice);
  const load_graph = useStore((s) => s.load_graph);

  const architecture_status = useStore((s) => s.architecture_status);
  const architecture_error = useStore((s) => s.architecture_error);
  const architecture_lens = useStore((s) => s.architecture_lens);
  const architecture_mode = useStore((s) => s.architecture_mode);
  const set_architecture_mode = useStore((s) => s.set_architecture_mode);
  const architecture_scope_kind = useStore((s) => s.architecture_scope_kind);
  const set_architecture_scope_kind = useStore((s) => s.set_architecture_scope_kind);
  const architecture_time_window = useStore((s) => s.architecture_time_window);
  const set_architecture_time_window = useStore((s) => s.set_architecture_time_window);
  const architecture_include_draft = useStore((s) => s.architecture_include_draft);
  const set_architecture_include_draft = useStore((s) => s.set_architecture_include_draft);
  const load_architecture_lens = useStore((s) => s.load_architecture_lens);

  const selectedId = useStore((s) => s.graph_selected_id);
  const select_graph_node = useStore((s) => s.select_graph_node);

  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;

  useEffect(() => {
    return () => {
      if (typeof document !== "undefined") {
        document.body.classList.remove("bm-no-select");
      }
    };
  }, []);

  useEffect(() => {
    if (!selected_workspace) return;
    if (graph_mode === "architecture") {
      if (architecture_lens) return;
      void load_architecture_lens();
      return;
    }
    if (!reasoning_ref) return;
    if (graph_slice) return;
    void load_graph();
  }, [
    selected_workspace,
    graph_mode,
    architecture_lens,
    load_architecture_lens,
    reasoning_ref,
    graph_slice,
    load_graph,
  ]);

  const nodes: GraphNodeDto[] = useMemo(() => {
    if (graph_mode === "architecture") {
      if (!architecture_lens) return [];
      return [...architecture_lens.nodes]
        .sort((a, b) => a.id.localeCompare(b.id))
        .map((n, idx) => architectureNodeToGraphNode(n, idx));
    }
    if (!graph_slice) return [];
    return graph_slice.nodes.filter((n) => !n.deleted);
  }, [graph_mode, architecture_lens, graph_slice]);

  const edges: GraphEdgeDto[] = useMemo(() => {
    if (graph_mode === "architecture") {
      if (!architecture_lens) return [];
      return [...architecture_lens.edges]
        .sort(
          (a, b) =>
            a.from.localeCompare(b.from) || a.rel.localeCompare(b.rel) || a.to.localeCompare(b.to),
        )
        .map((e, idx) => architectureEdgeToGraphEdge(e, idx, architecture_lens.generated_at_ms));
    }
    if (!graph_slice) return [];
    return graph_slice.edges.filter((e) => !e.deleted);
  }, [graph_mode, architecture_lens, graph_slice]);

  const edgeDraw: EdgeDraw[] = useMemo(() => {
    return edges.map((e) => {
      const { risk, weight } = parseEdgeMeta(e);
      const relLower = e.rel.toLowerCase();
      return { from: e.from, to: e.to, rel: e.rel, relLower, risk, weight };
    });
  }, [edges]);

  const edgeCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const e of edges) {
      counts.set(e.from, (counts.get(e.from) || 0) + 1);
      counts.set(e.to, (counts.get(e.to) || 0) + 1);
    }
    return counts;
  }, [edges]);

  const edgeCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const drawNodeLookupRef = useRef<Map<string, { _x: number; _y: number }>>(new Map());
  const edgesRef = useRef<EdgeDraw[]>([]);
  edgesRef.current = edgeDraw;
  const edgeDirtyRef = useRef(true);
  const interactionUntilRef = useRef(0);

  const viewport = useViewport();
  const { viewXRef, viewYRef, scaleRef, containerSizeRef, isPanningRef } = viewport;

  const markEdgeDirty = useCallback(() => {
    edgeDirtyRef.current = true;
  }, []);

  const touchInteraction = useCallback(
    (ttlMs = 180) => {
      interactionUntilRef.current = Math.max(
        interactionUntilRef.current,
        performance.now() + ttlMs,
      );
      edgeDirtyRef.current = true;
    },
    [],
  );

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      touchInteraction(220);
      viewport.handleWheel(e);
    },
    [touchInteraction, viewport],
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      viewport.handlePointerDown(e);
      if (isPanningRef.current) touchInteraction(260);
    },
    [touchInteraction, viewport, isPanningRef],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      // While panning, keep interaction "hot" so the edges canvas can use
      // a cheaper DPR and avoid jank on 4K.
      if (isPanningRef.current) touchInteraction(200);
      viewport.handlePointerMove(e);
    },
    [touchInteraction, viewport, isPanningRef],
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      viewport.handlePointerUp(e);
      touchInteraction(260);
    },
    [touchInteraction, viewport],
  );

  const onFrame = useCallback(
    () => {
      // Mark edges dirty; a dedicated rAF loop redraws at most once per frame.
      edgeDirtyRef.current = true;
    },
    [],
  );

  const { simNodes, nodeVersion, draggingNodeId, bumpAlpha } = useForceLayout({
    nodes,
    edges,
    onFrame,
  });

  useEffect(() => {
    const next = new Map<string, { _x: number; _y: number }>();
    for (const n of simNodes) next.set(n.id, n);
    drawNodeLookupRef.current = next;
    edgeDirtyRef.current = true;
  }, [simNodes, nodeVersion]);

  // Dedicated edge redraw loop:
  // - decouples canvas edges from force-layout step frequency
  // - keeps edges in sync with viewport pan/zoom even when simulation is idle/throttled
  useEffect(() => {
    let raf = 0;
    const last = {
      viewX: NaN,
      viewY: NaN,
      scale: NaN,
      cw: 0,
      ch: 0,
      focusId: null as string | null,
      edgesLen: -1,
      interacting: false,
    };
    const loop = () => {
      const canvas = edgeCanvasRef.current;
      if (canvas) {
        const viewX = viewXRef.current;
        const viewY = viewYRef.current;
        const scale = scaleRef.current;
        const cw = containerSizeRef.current.w;
        const ch = containerSizeRef.current.h;
        const focusId = selectedIdRef.current;
        const edgesNow = edgesRef.current;
        const interacting =
          !!draggingNodeId.current ||
          isPanningRef.current ||
          performance.now() < interactionUntilRef.current;
        // Toggle a CSS class on the container to enable "performance mode"
        // (disables expensive blur/shadows) while interacting.
        const containerEl = viewport.containerRef.current;
        if (containerEl) {
          containerEl.classList.toggle("bm-graph-interacting", interacting);
        }

        if (
          viewX !== last.viewX ||
          viewY !== last.viewY ||
          scale !== last.scale ||
          cw !== last.cw ||
          ch !== last.ch ||
          focusId !== last.focusId ||
          edgesNow.length !== last.edgesLen ||
          interacting !== last.interacting
        ) {
          edgeDirtyRef.current = true;
        }

        if (edgeDirtyRef.current) {
          edgeDirtyRef.current = false;
          last.viewX = viewX;
          last.viewY = viewY;
          last.scale = scale;
          last.cw = cw;
          last.ch = ch;
          last.focusId = focusId;
          last.edgesLen = edgesNow.length;
          last.interacting = interacting;
          drawEdges({
            canvas,
            nodes: drawNodeLookupRef.current,
            edges: edgesNow,
            focusId,
            interacting,
            viewX,
            viewY,
            scale,
            cw,
            ch,
          });
        }
      }
      raf = window.requestAnimationFrame(loop);
    };
    raf = window.requestAnimationFrame(loop);
    return () => window.cancelAnimationFrame(raf);
  }, [containerSizeRef, scaleRef, viewXRef, viewYRef, isPanningRef, draggingNodeId]);

  const handleNodeSelect = useCallback(
    (id: string) => {
      select_graph_node(id);
      bumpAlpha(0.12);
      markEdgeDirty();
    },
    [select_graph_node, bumpAlpha, markEdgeDirty],
  );

  const handleDragStart = useCallback(
    (id: string) => {
      draggingNodeId.current = id;
      bumpAlpha(0.18);
      markEdgeDirty();
    },
    [draggingNodeId, bumpAlpha, markEdgeDirty],
  );

  const handleDragEnd = useCallback(() => {
    draggingNodeId.current = null;
    bumpAlpha(0.12);
    markEdgeDirty();
  }, [draggingNodeId, bumpAlpha, markEdgeDirty]);

  const handleContainerClick = useCallback(
    (e: React.MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest?.("[data-no-pan]")) return;
      select_graph_node(null);
    },
    [select_graph_node],
  );

  const neighborSet = useMemo(() => {
    const s = new Set<string>();
    if (!selectedId) return s;
    for (const e of edges) {
      if (e.from === selectedId) s.add(e.to);
      if (e.to === selectedId) s.add(e.from);
    }
    return s;
  }, [edges, selectedId]);

  if (!selected_workspace) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <EmptyState icon={Activity} heading="Select workspace" description="Pick a workspace to inspect architecture." />
      </div>
    );
  }

  if (graph_mode === "reasoning" && !selected_task_id) {
    return (
      <div className="w-full h-full flex flex-col items-center justify-center gap-3">
        <EmptyState icon={Activity} heading="Select a task" description="Choose a task to inspect its reasoning graph." />
        <button
          className="px-3 py-1.5 rounded-lg bg-gray-900 text-white text-[12px]"
          onClick={() => set_graph_mode("architecture")}
        >
          Open architecture lens
        </button>
      </div>
    );
  }

  if (graph_mode === "reasoning" && graph_status === "loading" && !graph_slice) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <Skeleton variant="card" count={2} />
      </div>
    );
  }

  if (graph_mode === "reasoning" && graph_status === "error") {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-rose-600 px-6 text-center">
        Failed to load graph: {graph_error}
      </div>
    );
  }

  if (graph_mode === "architecture" && architecture_status === "loading" && !architecture_lens) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <Skeleton variant="card" count={2} />
      </div>
    );
  }

  if (graph_mode === "architecture" && architecture_status === "error") {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-rose-600 px-6 text-center">
        Failed to load architecture lens: {architecture_error}
      </div>
    );
  }

  if (nodes.length === 0) {
    return (
      <div className="w-full h-full flex items-center justify-center">
        <EmptyState
          icon={Activity}
          heading={graph_mode === "architecture" ? "Architecture is empty" : "Graph is empty"}
          description={
            graph_mode === "architecture"
              ? "No anchor/task/knowledge links found for this scope yet."
              : "No reasoning nodes found yet for this task."
          }
        />
      </div>
    );
  }

  return (
    <div className="w-full h-full relative">
      <div
        ref={viewport.containerRef}
        className="absolute inset-0 overflow-hidden select-none"
        style={{ touchAction: "none" }}
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onClick={handleContainerClick}
      >
        <canvas ref={edgeCanvasRef} className="absolute inset-0" />

        <div
          ref={viewport.transformRef}
          className="absolute inset-0"
          style={{ transformOrigin: "0 0" }}
        >
          <div>
            {simNodes.map((n) => {
              const dimmed = !!selectedId && n.id !== selectedId && !neighborSet.has(n.id);
              return (
                <GraphNode
                  key={n.id}
                  node={n}
                  dense={simNodes.length > 160}
                  selected={n.id === selectedId}
                  dimmed={dimmed}
                  edgeCount={edgeCounts.get(n.id) || 0}
                  scaleRef={viewport.scaleRef}
                  onSelect={handleNodeSelect}
                  onDragStart={handleDragStart}
                  onDragEnd={handleDragEnd}
                  onMove={() => touchInteraction(200)}
                />
              );
            })}
          </div>
        </div>

        <div className="absolute top-4 left-4 z-30 max-w-[min(70%,760px)] space-y-2" data-no-pan>
          <div className="bg-white/60 ring-1 ring-black/[0.04] rounded-xl p-1 flex items-center gap-1">
            <button
              className={cn(
                "px-2.5 py-1.5 rounded-lg text-[11px] font-medium",
                graph_mode === "architecture"
                  ? "bg-gray-900 text-white"
                  : "text-gray-600 hover:bg-black/5",
              )}
              onClick={() => set_graph_mode("architecture")}
            >
              Architecture lens
            </button>
            <button
              className={cn(
                "px-2.5 py-1.5 rounded-lg text-[11px] font-medium",
                graph_mode === "reasoning"
                  ? "bg-gray-900 text-white"
                  : "text-gray-600 hover:bg-black/5",
              )}
              disabled={!reasoning_ref}
              onClick={() => set_graph_mode("reasoning")}
            >
              Reasoning graph
            </button>
          </div>

          {graph_mode === "architecture" && architecture_lens && (
            <>
              <div className="bg-white/60 ring-1 ring-black/[0.04] rounded-xl p-2 flex flex-wrap items-center gap-2">
                <div className="flex items-center gap-1">
                  {ARCHITECTURE_MODES.map((m) => (
                    <button
                      key={m}
                      className={cn(
                        "px-2 py-1 rounded-md text-[10px] font-medium uppercase tracking-wide",
                        architecture_mode === m
                          ? "bg-gray-900 text-white"
                          : "text-gray-600 hover:bg-black/5",
                      )}
                      onClick={() => set_architecture_mode(m)}
                    >
                      {m}
                    </button>
                  ))}
                </div>

                <div className="flex items-center gap-1 ml-1">
                  <select
                    value={architecture_scope_kind}
                    onChange={(e) =>
                      set_architecture_scope_kind(e.target.value as "workspace" | "plan" | "task")
                    }
                    className="h-7 px-2 rounded-md bg-white/80 border border-gray-200 text-[11px] text-gray-700"
                  >
                    <option value="workspace">Workspace</option>
                    <option value="plan" disabled={!selected_plan}>Plan</option>
                    <option value="task" disabled={!selected_task_id}>Task</option>
                  </select>

                  <select
                    value={architecture_time_window}
                    onChange={(e) =>
                      set_architecture_time_window(e.target.value as "all" | "24h" | "7d")
                    }
                    className="h-7 px-2 rounded-md bg-white/80 border border-gray-200 text-[11px] text-gray-700"
                  >
                    <option value="all">All time</option>
                    <option value="7d">Last 7d</option>
                    <option value="24h">Last 24h</option>
                  </select>
                </div>

                <label className="ml-auto flex items-center gap-1.5 text-[11px] text-gray-600">
                  <input
                    type="checkbox"
                    checked={architecture_include_draft}
                    onChange={(e) => set_architecture_include_draft(e.target.checked)}
                  />
                  include draft
                </label>
              </div>

              <div className="bg-white/60 ring-1 ring-black/[0.04] rounded-xl px-3 py-2 text-[11px] text-gray-700 flex flex-wrap items-center gap-x-3 gap-y-1">
                <span>
                  anchors <b>{architecture_lens.summary.anchors_total}</b>
                </span>
                <span>
                  tasks <b>{architecture_lens.summary.tasks_total}</b>
                </span>
                <span>
                  knowledge <b>{architecture_lens.summary.knowledge_total}</b>
                </span>
                <span>
                  proven <b>{Math.round(architecture_lens.summary.proven_ratio * 100)}%</b>
                </span>
                <span className="inline-flex items-center gap-1">
                  <AlertTriangle size={12} className="text-rose-500" />
                  blocked <b>{architecture_lens.summary.blocked_total}</b>
                </span>
              </div>
              {(architecture_lens.hotspots.length > 0 || architecture_lens.next_actions.length > 0) && (
                <div className="bg-white/60 ring-1 ring-black/[0.04] rounded-xl px-3 py-2 text-[11px] text-gray-700 grid grid-cols-1 md:grid-cols-2 gap-2">
                  <div className="min-w-0">
                    <div className="text-[10px] uppercase tracking-widest text-gray-500 mb-1">hotspots</div>
                    <ul className="space-y-0.5">
                      {architecture_lens.hotspots.slice(0, 3).map((h) => (
                        <li key={h.id} className="truncate">
                          {h.label} <span className="text-gray-400 font-mono">d{h.degree}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                  <div className="min-w-0">
                    <div className="text-[10px] uppercase tracking-widest text-gray-500 mb-1">next</div>
                    <ul className="space-y-0.5">
                      {architecture_lens.next_actions.slice(0, 3).map((a, idx) => (
                        <li key={`${a}:${idx}`} className="truncate">
                          {a}
                        </li>
                      ))}
                    </ul>
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        <div className="absolute top-4 right-4 z-30 flex items-center gap-2" data-no-pan>
          <div className="bg-white/55 ring-1 ring-black/[0.03] rounded-xl overflow-hidden flex">
            {([
              { icon: Plus, fn: viewport.zoomIn, title: "Zoom in", border: false },
              { icon: Minus, fn: viewport.zoomOut, title: "Zoom out", border: true },
              { icon: Scan, fn: viewport.centerView, title: "Center view", border: true },
            ] as const).map(({ icon: I, fn, title, border }) => (
              <button
                key={title}
                className={cn(
                  "px-3 py-2 text-gray-700 hover:bg-black/5 transition-colors",
                  border && "border-l border-gray-200/60",
                )}
                onClick={(e) => {
                  e.stopPropagation();
                  fn();
                }}
                title={title}
              >
                <I size={14} />
              </button>
            ))}
          </div>
          <div className="px-3 py-2 rounded-xl bg-white/55 ring-1 ring-black/[0.03] text-[10px] text-gray-500 font-mono">
            {nodes.length}n / {edges.length}e
          </div>
        </div>

        <Minimap
          simNodes={simNodes}
          selectedId={selectedId}
          navigateTo={viewport.navigateTo}
          viewXRef={viewport.viewXRef}
          viewYRef={viewport.viewYRef}
          scaleRef={viewport.scaleRef}
          containerSizeRef={viewport.containerSizeRef}
        />
      </div>
    </div>
  );
}
