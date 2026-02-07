/* ── Detail Data Loader Hook ── */

import { useState, useEffect, useRef } from "react";
import { useUIStore } from "../store/ui-store";
import { useSnapshotStore } from "../store/snapshot-store";
import { getTaskDetail, getPlanDetail, getKnowledgeDetail } from "../api/endpoints";
import type { TaskDetail, PlanDetail, KnowledgeDetail } from "../api/types";

export type DetailData =
  | { kind: "plan"; data: PlanDetail }
  | { kind: "task"; data: TaskDetail }
  | { kind: "knowledge"; data: KnowledgeDetail }
  | null;

export function useDetailData() {
  const selection = useUIStore((s) => s.detailSelection);
  const project = useSnapshotStore((s) => s.project);
  const workspace = useSnapshotStore((s) => s.workspace);

  const [data, setData] = useState<DetailData>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const tokenRef = useRef(0);

  useEffect(() => {
    if (!selection) {
      setData(null);
      setError(null);
      return;
    }

    const token = ++tokenRef.current;
    setLoading(true);
    setError(null);

    const load = async () => {
      try {
        if (selection.kind === "task") {
          const detail = await getTaskDetail(selection.id, project, workspace);
          if (token === tokenRef.current) setData({ kind: "task", data: detail });
        } else if (selection.kind === "plan") {
          const detail = await getPlanDetail(selection.id, project, workspace);
          if (token === tokenRef.current) setData({ kind: "plan", data: detail });
        } else if (selection.kind === "knowledge") {
          const detail = await getKnowledgeDetail(project, selection.id);
          if (token === tokenRef.current) setData({ kind: "knowledge", data: detail });
        }
      } catch (err) {
        if (token === tokenRef.current) setError(String(err));
      } finally {
        if (token === tokenRef.current) setLoading(false);
      }
    };

    load();
  }, [selection?.kind, selection?.id, project, workspace]);

  return { data, loading, error, selection };
}
