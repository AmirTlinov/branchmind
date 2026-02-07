/* ── API Endpoints ── */

import { fetchJson, qs } from "./client";
import type {
  ProjectsResponse,
  WorkspacesResponse,
  AboutInfo,
  Snapshot,
  KnowledgeSnapshot,
  TaskDetail,
  PlanDetail,
  KnowledgeDetail,
  SearchResult,
  ViewerSettings,
} from "./types";

/* ── Project / workspace discovery ── */

export function getProjects(): Promise<ProjectsResponse> {
  return fetchJson<ProjectsResponse>("/api/projects");
}

export function getWorkspaces(project?: string): Promise<WorkspacesResponse> {
  return fetchJson<WorkspacesResponse>(`/api/workspaces${qs({ project })}`);
}

export function getAbout(project?: string): Promise<AboutInfo> {
  return fetchJson<AboutInfo>(`/api/about${qs({ project })}`);
}

/* ── Snapshot ── */

export function getSnapshot(
  project?: string,
  workspace?: string,
  lens: string = "work",
): Promise<Snapshot> {
  return fetchJson<Snapshot>(
    `/api/snapshot${qs({ project, workspace, lens })}`,
  );
}

export function getKnowledgeSnapshot(
  project?: string,
  workspace?: string,
): Promise<KnowledgeSnapshot> {
  return fetchJson<KnowledgeSnapshot>(
    `/api/snapshot${qs({ project, workspace, lens: "knowledge" })}`,
  );
}

/* ── Detail views ── */

export function getTaskDetail(
  taskId: string,
  project?: string,
  workspace?: string,
): Promise<TaskDetail> {
  return fetchJson<TaskDetail>(
    `/api/task/${encodeURIComponent(taskId)}${qs({ project, workspace })}`,
  );
}

export function getPlanDetail(
  planId: string,
  project?: string,
  workspace?: string,
): Promise<PlanDetail> {
  return fetchJson<PlanDetail>(
    `/api/plan/${encodeURIComponent(planId)}${qs({ project, workspace })}`,
  );
}

export function getKnowledgeDetail(
  project?: string,
  cardId?: string,
  maxChars?: number,
): Promise<KnowledgeDetail> {
  return fetchJson<KnowledgeDetail>(
    `/api/knowledge/${encodeURIComponent(cardId ?? "")}${qs({ project, max_chars: maxChars })}`,
  );
}

/* ── Search ── */

export function searchApi(
  project?: string,
  workspace?: string,
  query?: string,
  lens: string = "work",
  limit: number = 50,
): Promise<SearchResult> {
  return fetchJson<SearchResult>(
    `/api/search${qs({ project, workspace, q: query, lens, limit })}`,
  );
}

/* ── Settings ── */

export function getSettings(project?: string): Promise<ViewerSettings> {
  return fetchJson<ViewerSettings>(`/api/settings${qs({ project })}`);
}

export function setRunnerAutostart(
  project?: string,
  enabled?: boolean,
): Promise<unknown> {
  return fetchJson<unknown>("/api/settings/runner_autostart", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ project, enabled }),
  });
}
