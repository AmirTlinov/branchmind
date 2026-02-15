import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/cn";

interface EmptyStateProps {
  icon: LucideIcon;
  heading: string;
  description?: string;
  className?: string;
}

export function EmptyState({ icon: Icon, heading, description, className }: EmptyStateProps) {
  return (
    <div className={cn("flex flex-col items-center justify-center gap-3 py-12 px-6 text-center", className)}>
      <div className="w-11 h-11 rounded-xl bg-white/60 ring-1 ring-black/[0.03] flex items-center justify-center">
        <Icon size={20} className="text-gray-400" />
      </div>
      <div className="text-[13px] font-medium text-gray-600">{heading}</div>
      {description && (
        <div className="text-[11px] text-gray-400 max-w-[240px] leading-relaxed">{description}</div>
      )}
    </div>
  );
}
