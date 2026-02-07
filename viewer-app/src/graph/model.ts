/* ── Graph Model Builder: snapshot → nodes + edges ── */

import type { Snapshot, TaskSummary } from "../api/types";
import type { GraphNode, GraphEdge } from "../store/graph-store";

const PLAN_RADIUS = 1050;
const TASK_RADIUS_BASE = 160;
const PLAN_NODE_R = 28;
const TASK_NODE_R = 8;


function hashToUnit(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) - h + s.charCodeAt(i)) | 0;
  }
  return ((h >>> 0) % 10000) / 10000;
}

function statusColor(status: string, kind: "plan" | "task"): string {
  if (status === "DONE" || status === "COMPLETED") return "#34d399";
  if (status === "ACTIVE" || status === "IN_PROGRESS") return kind === "plan" ? "#60a5fa" : "#a78bfa";
  if (status === "BLOCKED") return "#f87171";
  if (status === "PARKED") return "#fbbf24";
  return "#6b7280";
}

export function buildGraphModel(
  snapshot: Snapshot,
  selectedPlanId: string | null,
  _lens: string,
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const plans = snapshot.plans ?? [];
  const tasks = snapshot.tasks ?? [];
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  // Layout plans in a circle
  const planPositions = new Map<string, { x: number; y: number }>();
  plans.forEach((plan, i) => {
    const angle = (i / Math.max(plans.length, 1)) * Math.PI * 2 - Math.PI / 2;
    const jitter = hashToUnit(plan.id) * 0.15;
    const r = PLAN_RADIUS * (0.85 + jitter);
    const x = Math.cos(angle) * r;
    const y = Math.sin(angle) * r;
    planPositions.set(plan.id, { x, y });

    nodes.push({
      id: plan.id,
      kind: "plan",
      label: plan.title || plan.id,
      x, y,
      radius: PLAN_NODE_R,
      color: statusColor(plan.status, "plan"),
      status: plan.status,
      planId: null,
      taskCount: plan.task_counts?.total ?? 0,
      priority: plan.priority,
    });
  });

  // Layout tasks around their parent plan
  const tasksByPlan = new Map<string, TaskSummary[]>();
  const orphanTasks: TaskSummary[] = [];
  tasks.forEach((task) => {
    if (task.plan_id) {
      const list = tasksByPlan.get(task.plan_id) ?? [];
      list.push(task);
      tasksByPlan.set(task.plan_id, list);
    } else {
      orphanTasks.push(task);
    }
  });

  // Filter: if a plan is selected, only show its tasks
  const visiblePlanIds = selectedPlanId
    ? new Set([selectedPlanId])
    : new Set(plans.map((p) => p.id));

  for (const [planId, planTasks] of tasksByPlan) {
    if (!visiblePlanIds.has(planId)) continue;
    const center = planPositions.get(planId);
    if (!center) continue;

    const taskRadius = TASK_RADIUS_BASE + Math.sqrt(planTasks.length) * 12;
    planTasks.forEach((task, i) => {
      const angle = (i / Math.max(planTasks.length, 1)) * Math.PI * 2;
      const jitter = hashToUnit(task.id) * 0.2;
      const r = taskRadius * (0.7 + jitter);
      const x = center.x + Math.cos(angle) * r;
      const y = center.y + Math.sin(angle) * r;

      nodes.push({
        id: task.id,
        kind: "task",
        label: task.title || task.id,
        x, y,
        radius: TASK_NODE_R,
        color: statusColor(task.status, "task"),
        status: task.status,
        planId: task.plan_id,
        priority: task.priority,
        blocked: task.blocked,
      });

      edges.push({
        source: planId,
        target: task.id,
        kind: "parent",
        weight: 1,
      });
    });
  }

  // Orphan tasks in center
  orphanTasks.forEach((task, i) => {
    const angle = (i / Math.max(orphanTasks.length, 1)) * Math.PI * 2;
    const r = 80 + hashToUnit(task.id) * 40;
    nodes.push({
      id: task.id,
      kind: "task",
      label: task.title || task.id,
      x: Math.cos(angle) * r,
      y: Math.sin(angle) * r,
      radius: TASK_NODE_R,
      color: statusColor(task.status, "task"),
      status: task.status,
      planId: null,
      priority: task.priority,
      blocked: task.blocked,
    });
  });

  // KNN-style similarity edges between plans (cosmetic)
  if (plans.length > 1 && plans.length <= 30) {
    for (let i = 0; i < plans.length; i++) {
      const j = (i + 1) % plans.length;
      edges.push({
        source: plans[i].id,
        target: plans[j].id,
        kind: "similar",
        weight: 0.3,
      });
    }
  }

  return { nodes, edges };
}
