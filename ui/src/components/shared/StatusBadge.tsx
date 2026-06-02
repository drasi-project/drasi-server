import { getStatusColor } from "@/utils/colors";
import type { ComponentStatus } from "@/utils/colors";
import { AlertCircle } from "lucide-react";

interface StatusBadgeProps {
  status: ComponentStatus;
  size?: "sm" | "md";
  error?: string;
}

export default function StatusBadge({ status, size = "sm", error }: StatusBadgeProps) {
  const color = getStatusColor(status);
  const isAnimated = status === "Running" || status === "Starting";
  const dotSize = size === "sm" ? "w-2 h-2" : "w-3 h-3";
  const showError = status === "Error" && error;

  return (
    <div className="flex flex-col gap-1">
      <span className="inline-flex items-center gap-1.5">
        <span className="relative flex">
          {isAnimated && (
            <span
              className={`absolute inline-flex ${dotSize} rounded-full opacity-75 animate-pulse`}
              style={{ backgroundColor: color }}
            />
          )}
          <span
            className={`relative inline-flex ${dotSize} rounded-full`}
            style={{ backgroundColor: color }}
          />
        </span>
        <span className="text-xs font-medium" style={{ color }}>
          {status}
        </span>
      </span>
      {showError && (
        <div className="flex items-start gap-1.5 mt-1 p-2 bg-red-500/10 rounded-md border border-red-500/20">
          <AlertCircle size={12} className="text-red-500 shrink-0 mt-0.5" />
          <span className="text-[10px] text-red-400 break-words leading-tight">
            {error}
          </span>
        </div>
      )}
    </div>
  );
}
