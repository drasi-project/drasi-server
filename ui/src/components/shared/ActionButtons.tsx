import { useState } from "react";
import { Play, Square, Trash2 } from "lucide-react";
import ConfirmDialog from "./ConfirmDialog";

interface ActionButtonsProps {
  status: string;
  componentName?: string;
  onStart?: () => void;
  onStop?: () => void;
  onDelete?: () => void;
  deleteDisabled?: boolean;
  deleteDisabledReason?: string;
  compact?: boolean;
}

export default function ActionButtons({
  status,
  componentName = "this component",
  onStart,
  onStop,
  onDelete,
  deleteDisabled = false,
  deleteDisabledReason,
  compact = false,
}: ActionButtonsProps) {
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const isRunning = status === "Running";
  const isStopped = status === "Stopped" || status === "Error";
  const iconSize = compact ? 14 : 16;

  const handleDeleteClick = () => {
    if (!deleteDisabled) {
      setShowDeleteConfirm(true);
    }
  };

  const handleConfirmDelete = () => {
    setShowDeleteConfirm(false);
    onDelete?.();
  };

  return (
    <>
      <div className="flex items-center gap-1">
        {/* Start/Stop button - always show one or the other */}
        {isStopped && onStart && (
          <button
            onClick={onStart}
            className="p-1.5 rounded-md transition-colors hover:bg-drasi-running/10 text-drasi-running/70 hover:text-drasi-running"
            title="Start"
          >
            <Play size={iconSize} fill="currentColor" />
          </button>
        )}
        {isRunning && onStop && (
          <button
            onClick={onStop}
            className="p-1.5 rounded-md transition-colors hover:bg-drasi-error/10 text-drasi-error/70 hover:text-drasi-error"
            title="Stop"
          >
            <Square size={iconSize} fill="currentColor" />
          </button>
        )}
        {onDelete && (
          <button
            onClick={handleDeleteClick}
            disabled={deleteDisabled}
            className={`p-1.5 rounded-md transition-colors ${
              deleteDisabled
                ? "opacity-30 cursor-not-allowed text-drasi-text-secondary/50"
                : "hover:bg-drasi-error/10 text-drasi-text-secondary/50 hover:text-drasi-error"
            }`}
            title={deleteDisabled ? deleteDisabledReason : "Delete"}
          >
            <Trash2 size={iconSize} />
          </button>
        )}
      </div>

      <ConfirmDialog
        open={showDeleteConfirm}
        title={`Delete ${componentName}?`}
        message="This will permanently remove this component from the server. This action cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        onConfirm={handleConfirmDelete}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </>
  );
}
