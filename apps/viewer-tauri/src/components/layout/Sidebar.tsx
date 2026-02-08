import React, { useMemo, useState } from "react";
import { GlassPanel } from "@/components/ui/Glass";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/format";
import {
  ChevronRight,
  Command,
  Folder,
  RefreshCw,
  Search,
  ShieldAlert,
  Workflow,
} from "lucide-react";

function storageDirLabel(storage_dir: string, repo_root?: string | null): string {
  if (repo_root && storage_dir.startsWith(repo_root)) {
    let rel = storage_dir.slice(repo_root.length);
    if (rel.startsWith("/")) rel = rel.slice(1);
    return rel || "(store root)";
  }
  const parts = storage_dir.split("/").filter(Boolean);
  return parts.slice(Math.max(parts.length - 3, 0)).join("/");
}

function SectionHeader({
  title,
  right,
}: {
  title: string;
  right?: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between px-2 mb-2">
      <h2 className="text-[11px] font-bold text-gray-400 uppercase tracking-widest">{title}</h2>
      {right}
    </div>
  );
}

function StatusDot({ blocked }: { blocked: boolean }) {
  return (
    <div
      className={cn(
        "w-2 h-2 rounded-full shrink-0",
        blocked ? "bg-rose-400" : "bg-emerald-400",
      )}
      title={blocked ? "Blocked" : "OK"}
    />
  );
}

function TaskRow({
  id,
  title,
  blocked,
  status,
  updated_at_ms,
  selected,
  onClick,
}: {
  id: string;
  title: string;
  blocked: boolean;
  status: string;
  updated_at_ms: number;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full group/item flex items-center justify-between py-1.5 px-2 rounded-lg text-left",
        "hover:bg-black/5 transition-colors",
        selected && "bg-white/55 ring-1 ring-black/[0.04]",
      )}
      title={id}
    >
      <div className="flex items-center gap-2.5 overflow-hidden">
        <StatusDot blocked={blocked} />
        <span
          className={cn(
            "text-[12px] truncate",
            selected ? "text-gray-900 font-medium" : "text-gray-700",
          )}
        >
          {title}
        </span>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        {status && (
          <span className="text-[9px] text-gray-400 font-mono uppercase">{status}</span>
        )}
        <span className="text-[10px] text-gray-300 min-w-[28px] text-right font-medium">
          {formatRelative(updated_at_ms)}
        </span>
      </div>
    </button>
  );
}

export function Sidebar() {
  const projects_status = useStore((s) => s.projects_status);
  const projects_error = useStore((s) => s.projects_error);
  const projects = useStore((s) => s.projects);
  const scan_projects = useStore((s) => s.scan_projects);

  const selected_storage_dir = useStore((s) => s.selected_storage_dir);
  const selected_workspace = useStore((s) => s.selected_workspace);
  const select_workspace = useStore((s) => s.select_workspace);

  const tasks_status = useStore((s) => s.tasks_status);
  const tasks_error = useStore((s) => s.tasks_error);
  const tasks = useStore((s) => s.tasks);
  const selected_task_id = useStore((s) => s.selected_task_id);
  const selected_task = useStore((s) => s.selected_task);
  const select_task = useStore((s) => s.select_task);

  const set_palette_open = useStore((s) => s.set_command_palette_open);

  const [taskFilter, setTaskFilter] = useState("");
  const [projectsOpen, setProjectsOpen] = useState<Record<string, boolean>>({});
  const [projectGroupsOpen, setProjectGroupsOpen] = useState<Record<string, boolean>>({});

  const groupedProjects = useMemo(() => {
    const map = new Map<string, typeof projects>();
    for (const p of projects) {
      const name = p.display_name || "Unknown";
      const arr = map.get(name);
      if (arr) arr.push(p);
      else map.set(name, [p]);
    }
    return Array.from(map.entries())
      .map(([name, items]) => ({
        name,
        items: [...items].sort((a, b) => a.storage_dir.localeCompare(b.storage_dir)),
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [projects]);

  const filteredTasks = useMemo(() => {
    const q = taskFilter.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => `${t.id} ${t.title}`.toLowerCase().includes(q));
  }, [taskFilter, tasks]);

  return (
    <GlassPanel
      intensity="low"
      className="h-full flex flex-col border-r border-gray-200/50 bg-[#EBECF0]/80 backdrop-blur-xl rounded-none"
    >
      {/* Search Header */}
      <div className="p-4 pb-2">
        {selected_task ? (
          <div className="mb-2 min-w-0">
            <div className="flex items-center justify-between gap-2 min-w-0">
              <div
                className="text-[12px] font-semibold text-gray-800 truncate"
                title={selected_task.title}
              >
                {selected_task.title}
              </div>
              <div
                className="text-[10px] text-gray-300 font-mono shrink-0"
                title={selected_task.id}
              >
                {selected_task.id}
              </div>
            </div>
          </div>
        ) : (
          <div className="mb-2 text-[11px] text-gray-400">Select a task</div>
        )}
        <div className="relative group">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-400 w-3.5 h-3.5 group-focus-within:text-gray-600 transition-colors" />
          <input
            value={taskFilter}
            onChange={(e) => setTaskFilter(e.target.value)}
            placeholder="Filter tasks…"
            className="w-full bg-white/55 hover:bg-white/80 focus:bg-white border border-transparent focus:border-gray-200 rounded-xl py-2 pl-9 pr-10 text-xs outline-none transition-all shadow-sm placeholder:text-gray-400"
          />
          <button
            onClick={() => set_palette_open(true)}
            className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-50 hover:opacity-80 transition-opacity"
            title="Command palette"
          >
            <Command size={11} />
            <span className="text-[10px] font-bold">K</span>
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-6 custom-scrollbar">
        {/* Projects */}
        <div>
          <SectionHeader
            title="Projects"
            right={
              <button
                onClick={() => void scan_projects()}
                className="p-1 hover:bg-black/5 rounded text-gray-400 hover:text-gray-600 transition-colors"
                title="Rescan"
              >
                <RefreshCw size={12} />
              </button>
            }
          />

          {projects_status === "loading" && (
            <div className="px-2 text-[12px] text-gray-500">Scanning…</div>
          )}
          {projects_status === "error" && (
            <div className="px-2 text-[12px] text-rose-600">
              Scan failed: {projects_error}
            </div>
          )}
          {projects_status === "ready" && projects.length === 0 && (
            <div className="px-2 text-[12px] text-gray-500 space-y-1">
              <div>No stores found.</div>
              <div className="text-[11px] text-gray-400">
                Tip: set <span className="font-mono">BRANCHMIND_VIEWER_SCAN_ROOTS</span> to add
                scan roots.
              </div>
            </div>
          )}

          <div className="space-y-1">
            {groupedProjects.map((g) => {
              if (g.items.length === 1) {
                const p = g.items[0];
                const isActiveProject = selected_storage_dir === p.storage_dir;
                const isOpen = projectsOpen[p.project_id] ?? isActiveProject;
                return (
                  <div key={p.project_id} className="group">
                    <button
                      type="button"
                      onClick={() =>
                        setProjectsOpen((prev) => ({
                          ...prev,
                          [p.project_id]: !isOpen,
                        }))
                      }
                      className="w-full flex items-center justify-between px-2 py-1.5 rounded-lg cursor-pointer hover:bg-black/5 transition-colors list-none outline-none select-none mb-0.5 group/summary text-left"
                    >
                      <div className="flex items-center gap-2.5 overflow-hidden min-w-0">
                        <ChevronRight
                          size={14}
                          className={cn(
                            "text-gray-300 shrink-0 transition-transform",
                            isOpen && "rotate-90",
                          )}
                        />
                        <Folder
                          size={14}
                          className="text-gray-400 group-hover/summary:text-gray-500 transition-colors shrink-0"
                        />
                        <span className="text-[13px] font-medium text-gray-700 group-hover/summary:text-gray-900 transition-colors truncate">
                          {p.display_name}
                        </span>
                      </div>
                    </button>

                    {isOpen && (
                      <div className="space-y-[1px] pl-0 relative">
                      {/* Guide Line */}
                      <div className="absolute left-[15px] top-0 bottom-2 w-[1px] bg-gray-200/50" />

                      {p.workspaces.map((w) => {
                        const active = isActiveProject && selected_workspace === w.workspace;
                        return (
                          <button
                            key={w.workspace}
                            onClick={() => void select_workspace(p.storage_dir, w.workspace)}
                            className={cn(
                              "w-full flex items-center justify-between py-1.5 px-2 pl-9 rounded-lg hover:bg-black/5 cursor-pointer relative text-left",
                              active && "bg-gray-50/70 ring-1 ring-black/[0.03]",
                            )}
                            title={w.project_guard ? `guard: ${w.project_guard}` : w.workspace}
                          >
                            <div className="flex items-center gap-2 overflow-hidden">
                              <Workflow size={13} className="text-gray-400 shrink-0" />
                              <span
                                className={cn(
                                  "text-[12px] truncate",
                                  active ? "text-gray-900 font-medium" : "text-gray-700",
                                )}
                              >
                                {w.workspace}
                              </span>
                            </div>
                            {w.project_guard && (
                              <span className="text-[10px] text-gray-400 font-mono shrink-0">
                                guard
                              </span>
                          )}
                        </button>
                      );
                    })}
                      </div>
                    )}
                  </div>
                );
              }

              const isGroupOpen = g.items.some((p) => p.storage_dir === selected_storage_dir);
              const isGroupDetailsOpen = projectGroupsOpen[g.name] ?? isGroupOpen;
              return (
                <div key={`group:${g.name}`} className="group">
                  <button
                    type="button"
                    onClick={() =>
                      setProjectGroupsOpen((prev) => ({
                        ...prev,
                        [g.name]: !isGroupDetailsOpen,
                      }))
                    }
                    className="w-full flex items-center justify-between px-2 py-1.5 rounded-lg cursor-pointer hover:bg-black/5 transition-colors list-none outline-none select-none mb-0.5 group/summary text-left"
                  >
                    <div className="flex items-center gap-2.5 overflow-hidden min-w-0">
                      <ChevronRight
                        size={14}
                        className={cn(
                          "text-gray-300 shrink-0 transition-transform",
                          isGroupDetailsOpen && "rotate-90",
                        )}
                      />
                      <Folder
                        size={14}
                        className="text-gray-400 group-hover/summary:text-gray-500 transition-colors shrink-0"
                      />
                      <span className="text-[13px] font-medium text-gray-700 group-hover/summary:text-gray-900 transition-colors truncate">
                        {g.name}
                      </span>
                    </div>
                    <span className="text-[10px] text-gray-300 font-mono shrink-0">{g.items.length}</span>
                  </button>

                  {isGroupDetailsOpen && (
                    <div className="space-y-1 pl-3">
                    {g.items.map((p) => {
                      const isActiveProject = selected_storage_dir === p.storage_dir;
                      const label = storageDirLabel(p.storage_dir, p.repo_root);
                      const isStoreOpen = projectsOpen[p.project_id] ?? isActiveProject;
                      return (
                        <div key={p.project_id} className="group">
                          <button
                            type="button"
                            onClick={() =>
                              setProjectsOpen((prev) => ({
                                ...prev,
                                [p.project_id]: !isStoreOpen,
                              }))
                            }
                            className="w-full flex items-center justify-between px-2 py-1.5 rounded-lg cursor-pointer hover:bg-black/5 transition-colors list-none outline-none select-none mb-0.5 text-left"
                          >
                            <div className="flex items-center gap-2 overflow-hidden min-w-0">
                              <ChevronRight
                                size={13}
                                className={cn(
                                  "text-gray-300 shrink-0 transition-transform",
                                  isStoreOpen && "rotate-90",
                                )}
                              />
                              <Folder size={13} className="text-gray-300 shrink-0" />
                              <span className="text-[12px] text-gray-600 truncate" title={p.storage_dir}>
                                {label}
                              </span>
                            </div>
                          </button>

                          {isStoreOpen && (
                            <div className="space-y-[1px] pl-0 relative">
                            {/* Guide Line */}
                            <div className="absolute left-[15px] top-0 bottom-2 w-[1px] bg-gray-200/50" />

                            {p.workspaces.map((w) => {
                              const active = isActiveProject && selected_workspace === w.workspace;
                              return (
                                <button
                                  key={w.workspace}
                                  onClick={() => void select_workspace(p.storage_dir, w.workspace)}
                                  className={cn(
                                    "w-full flex items-center justify-between py-1.5 px-2 pl-9 rounded-lg hover:bg-black/5 cursor-pointer relative text-left",
                                    active && "bg-gray-50/70 ring-1 ring-black/[0.03]",
                                  )}
                                  title={w.project_guard ? `guard: ${w.project_guard}` : w.workspace}
                                >
                                  <div className="flex items-center gap-2 overflow-hidden">
                                    <Workflow size={13} className="text-gray-400 shrink-0" />
                                    <span
                                      className={cn(
                                        "text-[12px] truncate",
                                        active ? "text-gray-900 font-medium" : "text-gray-700",
                                      )}
                                    >
                                      {w.workspace}
                                    </span>
                                  </div>
                                  {w.project_guard && (
                                    <span className="text-[10px] text-gray-400 font-mono shrink-0">
                                      guard
                                    </span>
                                  )}
                                </button>
                              );
                            })}
                            </div>
                          )}
                        </div>
                      );
                    })}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {/* Tasks */}
        <div>
          <SectionHeader title="Tasks" />

          {!selected_workspace && (
            <div className="px-2 text-[12px] text-gray-500">Pick a workspace.</div>
          )}
          {selected_workspace && tasks_status === "loading" && (
            <div className="px-2 text-[12px] text-gray-500">Loading tasks…</div>
          )}
          {selected_workspace && tasks_status === "error" && (
            <div className="px-2 text-[12px] text-rose-600">Failed: {tasks_error}</div>
          )}

          <div className="space-y-1">
            {selected_workspace &&
              tasks_status === "ready" &&
              filteredTasks.map((t) => (
                <TaskRow
                  key={t.id}
                  id={t.id}
                  title={t.title}
                  blocked={t.blocked}
                  status={t.status}
                  updated_at_ms={t.updated_at_ms}
                  selected={t.id === selected_task_id}
                  onClick={() => void select_task(t.id)}
                />
              ))}
          </div>
        </div>
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-white/20 bg-white/30 backdrop-blur-md flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="text-[11px] font-semibold text-gray-700 truncate">BranchMind Viewer</div>
          <div className="text-[10px] text-gray-400 truncate">
            {selected_workspace ? selected_workspace : "No workspace selected"}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {selected_storage_dir && (
            <button
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(selected_storage_dir);
                } catch {
                  // ignore
                }
              }}
              className="p-1.5 rounded-lg hover:bg-black/5 text-gray-400 hover:text-gray-700 transition-colors"
              title="Copy storage_dir"
            >
              <ShieldAlert size={14} />
            </button>
          )}
          <button
            onClick={() => set_palette_open(true)}
            className="px-2 py-1.5 rounded-xl hover:bg-black/5 text-gray-500 hover:text-gray-800 transition-colors text-[11px] font-semibold flex items-center gap-1.5"
            title="Command palette"
          >
            <Command size={12} />K
          </button>
        </div>
      </div>
    </GlassPanel>
  );
}
