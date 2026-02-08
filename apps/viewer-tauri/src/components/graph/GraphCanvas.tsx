import { useCallback, useEffect, useMemo, useRef } from "react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { useViewport } from "./useViewport";
import { useForceLayout } from "./useForceLayout";
import { GraphNode } from "./GraphNode";
import { Minimap } from "./Minimap";
import { Minus, Plus, Scan } from "lucide-react";
import type { GraphEdgeDto, GraphNodeDto } from "@/api/types";

function edgeColor(rel: string, highlighted: boolean): string {
  const r = rel.toLowerCase();
  if (r === "blocks") return highlighted ? "rgba(244,63,94,0.85)" : "rgba(244,63,94,0.22)";
  if (r === "supports") return highlighted ? "rgba(34,197,94,0.85)" : "rgba(34,197,94,0.22)";
  if (r === "contains") return highlighted ? "rgba(17,24,39,0.35)" : "rgba(17,24,39,0.08)";
  return highlighted ? "rgba(17,24,39,0.35)" : "rgba(17,24,39,0.08)";
}

function drawEdges(args: {
  canvas: HTMLCanvasElement;
  nodes: Map<string, { x: number; y: number }>;
  edges: GraphEdgeDto[];
  focusId: string | null;
  viewX: number;
  viewY: number;
  scale: number;
  cw: number;
  ch: number;
}) {
  const { canvas, nodes, edges, focusId, viewX, viewY, scale, cw, ch } = args;
  const dpr = window.devicePixelRatio || 1;
  if (canvas.width !== cw * dpr || canvas.height !== ch * dpr) {
    canvas.width = cw * dpr;
    canvas.height = ch * dpr;
    canvas.style.width = `${cw}px`;
    canvas.style.height = `${ch}px`;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, cw, ch);

  const toScreen = (wx: number, wy: number): [number, number] => [wx * scale + viewX, wy * scale + viewY];

  for (const e of edges) {
    const from = nodes.get(e.from);
    const to = nodes.get(e.to);
    if (!from || !to) continue;

    const [sx1, sy1] = toScreen(from.x, from.y);
    const [sx2, sy2] = toScreen(to.x, to.y);

    // Cull off-screen
    if (
      (sx1 < -120 && sx2 < -120) ||
      (sx1 > cw + 120 && sx2 > cw + 120) ||
      (sy1 < -120 && sy2 < -120) ||
      (sy1 > ch + 120 && sy2 > ch + 120)
    ) {
      continue;
    }

    const highlighted = !!focusId && (e.from === focusId || e.to === focusId);
    const dimmed = !!focusId && !highlighted;

    const dx = sx2 - sx1;
    const dy = sy2 - sy1;
    const dist = Math.sqrt(dx * dx + dy * dy) || 1;
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
    ctx.strokeStyle = edgeColor(e.rel, highlighted);
    ctx.lineWidth = highlighted ? 1.8 : 1.0;
    ctx.globalAlpha = dimmed ? 0.08 : 1;
    ctx.setLineDash(e.rel === "contains" ? [] : [5, 4]);
    ctx.stroke();
    ctx.setLineDash([]);

    // Arrowhead
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
    ctx.fillStyle = edgeColor(e.rel, highlighted);
    ctx.globalAlpha = dimmed ? 0.08 : 0.6;
    ctx.fill();
    ctx.globalAlpha = 1;
  }
}

export function GraphCanvas() {
  const selected_task_id = useStore((s) => s.selected_task_id);
  const reasoning_ref = useStore((s) => s.reasoning_ref);
  const graph_status = useStore((s) => s.graph_status);
  const graph_error = useStore((s) => s.graph_error);
  const graph_slice = useStore((s) => s.graph_slice);
  const load_graph = useStore((s) => s.load_graph);

  const selectedId = useStore((s) => s.graph_selected_id);
  const select_graph_node = useStore((s) => s.select_graph_node);

  // kick load if needed
  useEffect(() => {
    if (!reasoning_ref) return;
    if (graph_slice) return;
    void load_graph();
  }, [graph_slice, load_graph, reasoning_ref]);

  const nodes: GraphNodeDto[] = useMemo(() => {
    if (!graph_slice) return [];
    return graph_slice.nodes.filter((n) => !n.deleted);
  }, [graph_slice]);
  const edges: GraphEdgeDto[] = useMemo(() => {
    if (!graph_slice) return [];
    return graph_slice.edges.filter((e) => !e.deleted);
  }, [graph_slice]);

  const edgeCanvasRef = useRef<HTMLCanvasElement | null>(null);

  const viewport = useViewport();

  const onFrame = useCallback(
    (simNodes: { id: string; _x: number; _y: number }[], frameEdges: GraphEdgeDto[]) => {
      const canvas = edgeCanvasRef.current;
      if (!canvas) return;
      const map = new Map<string, { x: number; y: number }>();
      for (const n of simNodes) map.set(n.id, { x: n._x, y: n._y });
      drawEdges({
        canvas,
        nodes: map,
        edges: frameEdges,
        focusId: selectedId,
        viewX: viewport.viewXRef.current,
        viewY: viewport.viewYRef.current,
        scale: viewport.scaleRef.current,
        cw: viewport.containerSizeRef.current.w,
        ch: viewport.containerSizeRef.current.h,
      });
    },
    [selectedId, viewport.containerSizeRef, viewport.scaleRef, viewport.viewXRef, viewport.viewYRef],
  );

  const { simNodes, nodeVersion, draggingNodeId, bumpAlpha } = useForceLayout({
    nodes,
    edges,
    onFrame,
  });

  // Neighbors set for dimming logic (cheap)
  const neighborSet = useMemo(() => {
    const s = new Set<string>();
    if (!selectedId) return s;
    for (const e of edges) {
      if (e.from === selectedId) s.add(e.to);
      if (e.to === selectedId) s.add(e.from);
    }
    return s;
  }, [edges, selectedId]);

  if (!selected_task_id) {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-gray-500">
        Select a task to view its reasoning graph.
      </div>
    );
  }

  if (graph_status === "loading" && !graph_slice) {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-gray-500">
        Loading graph…
      </div>
    );
  }

  if (graph_status === "error") {
    return (
      <div className="w-full h-full flex items-center justify-center text-[13px] text-rose-600 px-6 text-center">
        Failed to load graph: {graph_error}
      </div>
    );
  }

  return (
    <div className="w-full h-full relative">
      <div
        ref={viewport.containerRef}
        className="absolute inset-0 overflow-hidden"
        onWheel={viewport.handleWheel}
        onPointerDown={viewport.handlePointerDown}
        onPointerMove={viewport.handlePointerMove}
        onPointerUp={viewport.handlePointerUp}
        onPointerCancel={viewport.handlePointerUp}
        onClick={(e) => {
          // Prevent "glitchy" deselection when clicking on a node / overlay.
          const target = e.target as HTMLElement | null;
          if (target?.closest?.("[data-no-pan]")) return;
          select_graph_node(null);
        }}
      >
        <canvas ref={edgeCanvasRef} className="absolute inset-0" />

        {/* Nodes */}
        <div
          className="absolute inset-0"
          style={{
            transform: `translate(${viewport.viewX}px, ${viewport.viewY}px) scale(${viewport.scale})`,
            transformOrigin: "0 0",
          }}
        >
          {/* `nodeVersion` forces React to re-evaluate the children list when nodes change */}
          <div key={nodeVersion}>
            {simNodes.map((n) => {
              const dimmed = !!selectedId && n.id !== selectedId && !neighborSet.has(n.id);
              return (
                <GraphNode
                  key={n.id}
                  node={n}
                  selected={n.id === selectedId}
                  dimmed={dimmed}
                  scale={viewport.scale}
                  onSelect={(id) => {
                    select_graph_node(id);
                    bumpAlpha(0.12);
                  }}
                  onDragStart={(id) => {
                    draggingNodeId.current = id;
                    bumpAlpha(0.18);
                  }}
                  onDragEnd={() => {
                    draggingNodeId.current = null;
                    bumpAlpha(0.12);
                  }}
                />
              );
            })}
          </div>
        </div>

        {/* Overlays */}
        <div className="absolute top-4 right-4 z-30 flex items-center gap-2" data-no-pan>
          <div className="bg-white/55 ring-1 ring-black/[0.03] rounded-xl overflow-hidden flex">
            <button
              className={cn("px-3 py-2 text-gray-700 hover:bg-black/5 transition-colors")}
              onClick={(e) => {
                e.stopPropagation();
                viewport.zoomIn();
              }}
              title="Zoom in"
            >
              <Plus size={14} />
            </button>
            <button
              className={cn("px-3 py-2 text-gray-700 hover:bg-black/5 transition-colors border-l border-gray-200/60")}
              onClick={(e) => {
                e.stopPropagation();
                viewport.zoomOut();
              }}
              title="Zoom out"
            >
              <Minus size={14} />
            </button>
            <button
              className={cn("px-3 py-2 text-gray-700 hover:bg-black/5 transition-colors border-l border-gray-200/60")}
              onClick={(e) => {
                e.stopPropagation();
                viewport.centerView();
              }}
              title="Center view"
            >
              <Scan size={14} />
            </button>
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
