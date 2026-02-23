import { Activity } from "lucide-react";

export interface EventEntry {
  id: string;
  timestamp: string;
  message: string;
  type: "info" | "success" | "warning" | "error";
}

interface EventBarProps {
  events: EventEntry[];
  onDismiss?: () => void;
}

export default function EventBar({ events, onDismiss }: EventBarProps) {
  if (events.length === 0) return null;

  const typeColors = {
    info: "text-drasi-text-secondary",
    success: "text-drasi-running",
    warning: "text-drasi-warning",
    error: "text-drasi-error",
  };

  return (
    <div className="event-bar">
      <Activity
        size={12}
        className="text-drasi-text-secondary mr-2 flex-shrink-0"
      />
      <div className="flex items-center gap-4 overflow-hidden">
        {events.slice(0, 5).map((ev) => (
          <span
            key={ev.id}
            className={`flex items-center gap-1.5 whitespace-nowrap ${typeColors[ev.type]}`}
          >
            <span className="text-drasi-text-secondary">
              {new Date(ev.timestamp).toLocaleTimeString()}
            </span>
            <span>{ev.message}</span>
          </span>
        ))}
      </div>
      {onDismiss && (
        <button
          onClick={onDismiss}
          className="ml-auto text-drasi-text-secondary hover:text-drasi-text-primary pl-4"
        >
          ×
        </button>
      )}
    </div>
  );
}
