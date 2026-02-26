import { Play, Square } from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import type { ComponentStatus, ComponentType } from "@/utils/colors";

interface ConnectedComponentItemProps {
  id: string;
  type: ComponentType;
  status: ComponentStatus;
  kind?: string;
  accentColor: string;
  onNavigate?: (id: string, type: ComponentType) => void;
  onStart?: (id: string) => void;
  onStop?: (id: string) => void;
}

export default function ConnectedComponentItem({
  id,
  type,
  status,
  kind,
  accentColor,
  onNavigate,
  onStart,
  onStop,
}: ConnectedComponentItemProps) {
  const canStart = status === "Stopped" || status === "Error";
  const canStop = status === "Running";
  const isTransitioning = status === "Starting" || status === "Stopping";

  return (
    <div className="flex items-center justify-between p-2.5 rounded-lg bg-drasi-card border border-drasi-border/50 hover:border-opacity-50 transition-colors"
         style={{ borderColor: `${accentColor}30` }}>
      <div className="flex items-center gap-2 min-w-0 flex-1">
        <div 
          className="w-2 h-2 rounded-full shrink-0" 
          style={{ backgroundColor: accentColor }} 
        />
        <button
          onClick={() => onNavigate?.(id, type)}
          className="text-sm font-medium text-drasi-text-primary hover:underline truncate text-left"
          title={`Open ${id} in inspector`}
        >
          {id}
        </button>
        {kind && (
          <span className="text-[10px] text-drasi-text-secondary px-1.5 py-0.5 rounded bg-drasi-bg shrink-0">
            {kind}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 shrink-0 ml-2">
        {(onStart || onStop) && (
          <div className="flex items-center gap-1">
            {canStart && onStart && (
              <button
                onClick={() => onStart(id)}
                className="p-1 rounded hover:bg-drasi-bg text-drasi-text-secondary hover:text-green-500 transition-colors"
                title="Start"
              >
                <Play size={12} />
              </button>
            )}
            {canStop && onStop && (
              <button
                onClick={() => onStop(id)}
                className="p-1 rounded hover:bg-drasi-bg text-drasi-text-secondary hover:text-amber-500 transition-colors"
                title="Stop"
              >
                <Square size={12} />
              </button>
            )}
            {isTransitioning && (
              <div className="w-5 h-5 flex items-center justify-center">
                <div className="w-3 h-3 border-2 border-drasi-text-secondary border-t-transparent rounded-full animate-spin" />
              </div>
            )}
          </div>
        )}
        <StatusBadge status={status} />
      </div>
    </div>
  );
}
