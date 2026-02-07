import React from "react";
import { cn } from "@/lib/cn";

export function GlassPanel({
  className,
  children,
  intensity = "low",
}: React.PropsWithChildren<{
  className?: string;
  intensity?: "low" | "medium" | "high";
}>) {
  const base =
    "bg-white/50 backdrop-blur-xl border border-gray-200/50 shadow-sm shadow-black/[0.03]";
  const tuned =
    intensity === "high"
      ? "bg-white/65"
      : intensity === "medium"
        ? "bg-white/55"
        : "bg-white/45";

  return <div className={cn(base, tuned, className)}>{children}</div>;
}

