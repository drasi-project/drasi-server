import { Play, Square, Trash2 } from "lucide-react";

interface ActionButtonsProps {
  status: string;
  onStart?: () => void;
  onStop?: () => void;
  onDelete?: () => void;
  compact?: boolean;
}

export default function ActionButtons({
  status,
  onStart,
  onStop,
  onDelete,
  compact = false,
}: ActionButtonsProps) {
  const isRunning = status === "Running";
  const isStopped = status === "Stopped" || status === "Error";
  const btnClass = compact ? "p-1.5 rounded-lg" : "action-btn";

  return (
    <div className="flex items-center gap-2">
      {isStopped && onStart && (
        <button
          onClick={onStart}
          className={`${btnClass} bg-drasi-running/20 text-drasi-running hover:bg-drasi-running/30 transition-colors`}
          title="Start"
        >
          <Play size={compact ? 14 : 16} />
          {!compact && <span className="ml-1">Start</span>}
        </button>
      )}
      {isRunning && onStop && (
        <button
          onClick={onStop}
          className={`${btnClass} bg-drasi-warning/20 text-drasi-warning hover:bg-drasi-warning/30 transition-colors`}
          title="Stop"
        >
          <Square size={compact ? 14 : 16} />
          {!compact && <span className="ml-1">Stop</span>}
        </button>
      )}
      {onDelete && (
        <button
          onClick={onDelete}
          className={`${btnClass} bg-drasi-error/20 text-drasi-error hover:bg-drasi-error/30 transition-colors`}
          title="Delete"
        >
          <Trash2 size={compact ? 14 : 16} />
          {!compact && <span className="ml-1">Delete</span>}
        </button>
      )}
    </div>
  );
}
