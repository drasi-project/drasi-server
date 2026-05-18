import { useState, useMemo, useRef, useEffect } from "react";
import { Search, X } from "lucide-react";
import type { EventEntry } from "@/components/events/EventPanel";

type FilterType = "all" | "info" | "success" | "warning" | "error";

const typeColors = {
  info: "text-[var(--drasi-text-secondary)]",
  success: "text-drasi-running",
  warning: "text-drasi-warning",
  error: "text-drasi-error",
};

const typeDots = {
  info: "bg-[var(--drasi-text-secondary)]",
  success: "bg-drasi-running",
  warning: "bg-drasi-warning",
  error: "bg-drasi-error",
};

const FILTERS: { id: FilterType; label: string }[] = [
  { id: "all", label: "All" },
  { id: "info", label: "Info" },
  { id: "success", label: "Success" },
  { id: "warning", label: "Warning" },
  { id: "error", label: "Error" },
];

interface LogsPanelProps {
  events: EventEntry[];
  onClear?: () => void;
}

export default function LogsPanel({ events, onClear }: LogsPanelProps) {
  const [searchText, setSearchText] = useState("");
  const [filter, setFilter] = useState<FilterType>("all");
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    let result = events;
    if (filter !== "all") {
      result = result.filter((e) => e.type === filter);
    }
    if (searchText.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter((e) => e.message.toLowerCase().includes(q));
    }
    return result;
  }, [events, filter, searchText]);

  // Auto-scroll to top when new events arrive
  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = 0;
    }
  }, [events.length]);

  return (
    <div className="flex flex-col h-full">
      {/* Search */}
      <div className="px-3 pt-3 pb-2 flex-shrink-0">
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--drasi-text-secondary)]"
          />
          <input
            type="text"
            placeholder="Search logs…"
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            className="w-full pl-8 pr-8 py-1.5 text-xs rounded-lg bg-[var(--drasi-card)] border border-[var(--drasi-border)] text-[var(--drasi-text-primary)] placeholder:text-[var(--drasi-text-secondary)] focus:outline-none focus:border-[var(--drasi-text-secondary)] transition-colors"
          />
          {searchText && (
            <button
              onClick={() => setSearchText("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)]"
            >
              <X size={12} />
            </button>
          )}
        </div>
      </div>

      {/* Filter chips */}
      <div className="px-3 pb-2 flex gap-1 flex-wrap flex-shrink-0">
        {FILTERS.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors ${
              filter === f.id
                ? "bg-[var(--drasi-card)] text-[var(--drasi-text-primary)] border border-[var(--drasi-text-secondary)]"
                : "text-[var(--drasi-text-secondary)] border border-[var(--drasi-border)] hover:border-[var(--drasi-text-secondary)]"
            }`}
          >
            {f.label}
          </button>
        ))}
        {onClear && events.length > 0 && (
          <button
            onClick={onClear}
            className="ml-auto px-2 py-0.5 rounded-full text-[10px] text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] transition-colors"
          >
            Clear
          </button>
        )}
      </div>

      {/* Event list */}
      <div ref={listRef} className="flex-1 overflow-y-auto">
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-xs text-[var(--drasi-text-secondary)]">
            {events.length === 0 ? "No recent activity" : "No matching logs"}
          </div>
        ) : (
          <div className="divide-y divide-[var(--drasi-border)]">
            {filtered.map((ev) => (
              <div
                key={ev.id}
                className="px-3 py-2 flex items-start gap-2"
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full mt-1.5 flex-shrink-0 ${typeDots[ev.type]}`}
                />
                <div className="min-w-0 flex-1">
                  <p className={`text-xs leading-relaxed break-words ${typeColors[ev.type]}`}>
                    {ev.message}
                  </p>
                  <p className="text-[10px] text-[var(--drasi-text-secondary)] mt-0.5">
                    {new Date(ev.timestamp).toLocaleTimeString()}
                  </p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
