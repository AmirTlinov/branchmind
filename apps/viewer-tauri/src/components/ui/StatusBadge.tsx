import {
  CheckCircle2,
  CircleDashed,
  Loader2,
  Pause,
  Play,
  ShieldAlert,
} from "lucide-react";
import { cn } from "@/lib/cn";

type BadgeVariant = "active" | "completed" | "paused" | "blocked" | "open" | "wip" | "default";

const VARIANTS: Record<BadgeVariant, { label: string; color: string; icon: React.ReactNode }> = {
  active: {
    label: "Active",
    color: "bg-blue-50 text-blue-600 ring-blue-200/50",
    icon: <Play size={9} className="fill-current" />,
  },
  wip: {
    label: "WIP",
    color: "bg-blue-50 text-blue-600 ring-blue-200/50",
    icon: <Loader2 size={9} className="animate-spin" />,
  },
  completed: {
    label: "Done",
    color: "bg-emerald-50 text-emerald-600 ring-emerald-200/50",
    icon: <CheckCircle2 size={9} />,
  },
  paused: {
    label: "Paused",
    color: "bg-amber-50 text-amber-600 ring-amber-200/50",
    icon: <Pause size={9} />,
  },
  blocked: {
    label: "Blocked",
    color: "bg-rose-50 text-rose-600 ring-rose-200/50",
    icon: <ShieldAlert size={9} />,
  },
  open: {
    label: "Open",
    color: "bg-gray-50 text-gray-500 ring-gray-200/50",
    icon: <CircleDashed size={9} />,
  },
  default: {
    label: "",
    color: "bg-gray-50 text-gray-500 ring-gray-200/50",
    icon: null,
  },
};

interface StatusBadgeProps {
  status: string;
  className?: string;
}

function resolveVariant(status: string): BadgeVariant {
  const s = status.toLowerCase();
  if (s === "active" || s === "running" || s === "in_progress") return "active";
  if (s === "wip") return "wip";
  if (s === "completed" || s === "done" || s === "closed") return "completed";
  if (s === "paused" || s === "deferred") return "paused";
  if (s === "blocked") return "blocked";
  if (s === "open" || s === "pending" || s === "todo") return "open";
  return "default";
}

export function StatusBadge({ status, className }: StatusBadgeProps) {
  const variant = resolveVariant(status);
  const v = VARIANTS[variant];
  const label = v.label || status;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[9px] font-medium ring-1",
        v.color,
        className,
      )}
    >
      {v.icon}
      {label}
    </span>
  );
}
