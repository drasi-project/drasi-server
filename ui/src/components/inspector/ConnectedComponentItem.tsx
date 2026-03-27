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
  kind: _kind,
  accentColor,
  onNavigate,
  onStart,
  onStop,
}: ConnectedComponentItemProps) {
  const canStart = status === "Stopped" || status === "Error";
  const canStop = status === "Running";
  const isTransitioning = status === "Starting" || status === "Stopping";

  return (
    <div className="flex items-center justify-between p-2 rounded-lg bg-drasi-card border border-drasi-border/50 hover:border-opacity-50 transition-colors gap-1"
         style={{ borderColor: `${accentColor}30` }}>
      <div className="flex items-center gap-1.5 min-w-0 flex-1">
        <div 
          className="w-1.5 h-1.5 rounded-full shrink-0" 
          style={{ backgroundColor: accentColor }} 
        />
        <button
          onClick={() => onNavigate?.(id, type)}
          className="text-xs font-medium text-drasi-text-primary hover:underline truncate text-left"
          title={`Open ${id} in inspector`}
        >
          {id}
        </button>
      </div>
      <div className="flex items-center gap-1 shrink-0">
        {(onStart || onStop) && (
          <>
            {canStart && onStart && (
              <button
                onClick={() => onStart(id)}
                className="p-0.5 rounded hover:bg-drasi-running/10 text-drasi-running/70 hover:text-drasi-running transition-colors"
                title="Start"
              >
                <Play size={10} />
              </button>
            )}
            {canStop && onStop && (
              <button
                onClick={() => onStop(id)}
                className="p-0.5 rounded hover:bg-drasi-error/10 text-drasi-error/70 hover:text-drasi-error transition-colors"
                title="Stop"
              >
                <Square size={10} />
              </button>
            )}
            {isTransitioning && (
              <div className="w-4 h-4 flex items-center justify-center">
                <div className="w-2.5 h-2.5 border-2 border-drasi-text-secondary border-t-transparent rounded-full animate-spin" />
              </div>
            )}
          </>
        )}
        <StatusBadge status={status} />
      </div>
    </div>
  );
}
