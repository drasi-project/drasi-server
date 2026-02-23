import { getStatusColor } from "@/utils/colors";
import type { ComponentStatus } from "@/utils/colors";

interface StatusBadgeProps {
  status: ComponentStatus;
  size?: "sm" | "md";
}

export default function StatusBadge({ status, size = "sm" }: StatusBadgeProps) {
  const color = getStatusColor(status);
  const isAnimated = status === "Running" || status === "Starting";
  const dotSize = size === "sm" ? "w-2 h-2" : "w-3 h-3";

  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="relative flex">
        {isAnimated && (
          <span
            className={`absolute inline-flex ${dotSize} rounded-full opacity-50 animate-ping`}
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
  );
}
