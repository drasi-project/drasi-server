import { Activity, X } from "lucide-react";

export interface EventEntry {
  id: string;
  timestamp: string;
  message: string;
  type: "info" | "success" | "warning" | "error";
}

interface EventPanelProps {
  events: EventEntry[];
  open: boolean;
  onClose: () => void;
  onClear?: () => void;
}

const typeColors = {
  info: "text-drasi-text-secondary",
  success: "text-drasi-running",
  warning: "text-drasi-warning",
  error: "text-drasi-error",
};

const typeDots = {
  info: "bg-drasi-text-secondary",
  success: "bg-drasi-running",
  warning: "bg-drasi-warning",
  error: "bg-drasi-error",
};

/**
 * Slide-out left panel displaying recent activity events.
 * Toggled by the Activity icon in the header.
 */
export default function EventPanel({
  events,
  open,
  onClose,
  onClear,
}: EventPanelProps) {
  return (
    <>
      {/* Backdrop */}
      {open && (
        <div
          className="absolute inset-0 z-40"
          onClick={onClose}
        />
      )}

      {/* Panel */}
      <div
        className={`absolute top-0 left-0 bottom-0 z-50 w-80 bg-drasi-surface border-r border-drasi-border flex flex-col transition-transform duration-200 ease-in-out ${
          open ? "translate-x-0" : "-translate-x-full"
        }`}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-drasi-border flex-shrink-0">
          <div className="flex items-center gap-2">
            <Activity size={14} className="text-drasi-text-secondary" />
            <span className="text-sm font-semibold text-drasi-text-primary">
              Activity
            </span>
            {events.length > 0 && (
              <span className="text-xs text-drasi-text-secondary">
                ({events.length})
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            {onClear && events.length > 0 && (
              <button
                onClick={onClear}
                className="text-xs text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
              >
                Clear
              </button>
            )}
            <button
              onClick={onClose}
              className="p-1 rounded hover:bg-drasi-card text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
            >
              <X size={14} />
            </button>
          </div>
        </div>

        {/* Event list */}
        <div className="flex-1 overflow-y-auto">
          {events.length === 0 ? (
            <div className="flex items-center justify-center h-32 text-xs text-drasi-text-secondary">
              No recent activity
            </div>
          ) : (
            <div className="divide-y divide-drasi-border">
              {events.map((ev) => (
                <div
                  key={ev.id}
                  className="px-4 py-2.5 flex items-start gap-2.5"
                >
                  <span
                    className={`w-1.5 h-1.5 rounded-full mt-1.5 flex-shrink-0 ${typeDots[ev.type]}`}
                  />
                  <div className="min-w-0 flex-1">
                    <p
                      className={`text-xs leading-relaxed ${typeColors[ev.type]}`}
                    >
                      {ev.message}
                    </p>
                    <p className="text-[10px] text-drasi-text-secondary mt-0.5">
                      {new Date(ev.timestamp).toLocaleTimeString()}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </>
  );
}
