import { MotionValue, useAnimationFrame } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { GraphEdgeDto, GraphNodeDto } from "@/api/types";

const SETTLE_ALPHA_DECAY = 0.965;
const SETTLE_DAMPING = 0.86;
const AMBIENT_ALPHA = 0.02;
const MIN_NODE_SPACING = 160;
const IDEAL_EDGE_DIST = 280;
const ACTIVE_TARGET_FRAME_MS = 16;
const IDLE_TARGET_FRAME_MS = 28;
const MAX_REPULSION_NEIGHBORS = 180;

export interface SimNode extends GraphNodeDto {
  label: string;
  x: MotionValue<number>;
  y: MotionValue<number>;
  _x: number;
  _y: number;
  vx: number;
  vy: number;
}

function hash32(s: string): number {
  // FNV-1a (fast, deterministic)
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function initialPos(id: string): { x: number; y: number } {
  const h1 = hash32(id);
  const h2 = hash32(id + "/r");
  const angle = (h1 % 360) * (Math.PI / 180);
  const radius = 180 + (h2 % 520);
  return { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius };
}

export function useForceLayout({
  nodes,
  edges,
  onFrame,
}: {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  onFrame?: (simNodes: SimNode[], edges: GraphEdgeDto[]) => void;
}): {
  simNodes: SimNode[];
  nodeVersion: number;
  draggingNodeId: React.RefObject<string | null>;
  alphaRef: React.RefObject<number>;
  bumpAlpha: (value?: number) => void;
} {
  const simNodesRef = useRef<SimNode[]>([]);
  const alphaRef = useRef(0.5);
  const draggingNodeId = useRef<string | null>(null);
  const lastTickTsRef = useRef(0);
  const [nodeVersion, setNodeVersion] = useState(0);

  const edgesRef = useRef(edges);
  edgesRef.current = edges;

  const onFrameRef = useRef(onFrame);
  onFrameRef.current = onFrame;

  const adjacency = useMemo(() => {
    const m = new Map<string, GraphEdgeDto[]>();
    for (const e of edges) {
      if (!m.has(e.from)) m.set(e.from, []);
      if (!m.has(e.to)) m.set(e.to, []);
      m.get(e.from)!.push(e);
      m.get(e.to)!.push(e);
    }
    return m;
  }, [edges]);

  // Build / update simulation nodes
  useEffect(() => {
    const existing = simNodesRef.current;
    const existingMap = new Map(existing.map((n) => [n.id, n]));

    const newSim: SimNode[] = nodes.map((n) => {
      const ex = existingMap.get(n.id);
      const label = n.title || n.text || n.id;
      if (ex) {
        return { ...ex, ...n, label };
      }
      const pos = initialPos(n.id);
      return {
        ...n,
        label,
        x: new MotionValue(pos.x),
        y: new MotionValue(pos.y),
        _x: pos.x,
        _y: pos.y,
        vx: 0,
        vy: 0,
      };
    });

    simNodesRef.current = newSim;
    alphaRef.current = 0.42;
    setNodeVersion((v) => v + 1);
  }, [nodes]);

  useAnimationFrame((t) => {
    const dragging = !!draggingNodeId.current;
    const targetFrameMs = dragging ? ACTIVE_TARGET_FRAME_MS : IDLE_TARGET_FRAME_MS;
    if (lastTickTsRef.current !== 0 && t - lastTickTsRef.current < targetFrameMs) {
      return;
    }
    const stepMs =
      lastTickTsRef.current === 0 ? targetFrameMs : Math.min(t - lastTickTsRef.current, 48);
    lastTickTsRef.current = t;
    const dt = stepMs / 16;
    const simNodes = simNodesRef.current;
    if (simNodes.length === 0) return;

    if (!draggingNodeId.current) {
      alphaRef.current = Math.max(alphaRef.current * SETTLE_ALPHA_DECAY, AMBIENT_ALPHA);
    }
    const alpha = alphaRef.current;
    const isIdle = alpha <= AMBIENT_ALPHA + 0.0005 && !draggingNodeId.current;

    // When the system is fully settled, skip the O(n^2) force computations.
    // NOTE: GraphCanvas now has its own rAF loop that redraws edges on viewport
    // pan/zoom (and on simulation movement), so `onFrame` is no longer needed
    // in the hard-idle path.
    let maxSpeed = 0;
    for (const n of simNodes) {
      maxSpeed = Math.max(maxSpeed, Math.abs(n.vx) + Math.abs(n.vy));
    }
    const isHardIdle = isIdle && maxSpeed < 0.001;
    if (isHardIdle) {
      // Prevent floating-point drift from accumulating tiny velocities.
      for (const n of simNodes) {
        if (Math.abs(n.vx) + Math.abs(n.vy) < 1e-6) {
          n.vx = 0;
          n.vy = 0;
        }
      }
      return;
    }

    const map = isIdle ? null : new Map(simNodes.map((n) => [n.id, n] as const));

    const nodeCount = simNodes.length;
    const repulsionStep =
      nodeCount > MAX_REPULSION_NEIGHBORS
        ? Math.ceil(nodeCount / MAX_REPULSION_NEIGHBORS)
        : 1;

    for (let i = 0; i < nodeCount; i++) {
      const node = simNodes[i];

      if (draggingNodeId.current === node.id) {
        node.vx = 0;
        node.vy = 0;
        continue;
      }

      let fx = 0;
      let fy = 0;

      // Repulsion
      for (let offset = 1; offset < nodeCount; offset += repulsionStep) {
        const j = (i + offset) % nodeCount;
        const other = simNodes[j];
        const dx = node._x - other._x;
        const dy = node._y - other._y;
        const distSq = dx * dx + dy * dy;
        const dist = Math.sqrt(distSq) || 1;

        if (dist < MIN_NODE_SPACING) {
          const overlap = MIN_NODE_SPACING - dist;
          fx += (dx / dist) * overlap * 0.55;
          fy += (dy / dist) * overlap * 0.55;
        } else if (dist < 700 && !isIdle) {
          const force = (700 * alpha) / (distSq + 900);
          fx += (dx / dist) * force;
          fy += (dy / dist) * force;
        }
      }

      // Gentle center gravity (keeps things bounded)
      if (!isIdle) {
        fx -= node._x * 0.00055 * alpha;
        fy -= node._y * 0.00055 * alpha;
      }

      // Edge attraction (local)
      if (!isIdle) {
        const connected = adjacency.get(node.id) || [];
        for (const e of connected) {
          const otherId = e.from === node.id ? e.to : e.from;
          const other = map?.get(otherId);
          if (!other) continue;
          const dx = node._x - other._x;
          const dy = node._y - other._y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          if (dist > IDEAL_EDGE_DIST) {
            const pull = (dist - IDEAL_EDGE_DIST) * 0.0032 * alpha;
            fx -= (dx / dist) * pull;
            fy -= (dy / dist) * pull;
          }
        }
      }

      // Integrate
      node.vx = (node.vx + fx) * SETTLE_DAMPING;
      node.vy = (node.vy + fy) * SETTLE_DAMPING;
      node._x += node.vx * dt;
      node._y += node.vy * dt;
      node.x.set(node._x);
      node.y.set(node._y);
    }

    onFrameRef.current?.(simNodes, edgesRef.current);
  });

  const bumpAlpha = useCallback((value = 0.1) => {
    alphaRef.current = Math.max(alphaRef.current, value);
  }, []);

  return {
    simNodes: simNodesRef.current,
    nodeVersion,
    draggingNodeId,
    alphaRef,
    bumpAlpha,
  };
}
