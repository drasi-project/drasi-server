import { useEffect, useCallback, useState, useRef } from "react";
import { subscribeComponentEvents } from "./useApi";
import type { ComponentEvent } from "@/api/types";
import type { EventEntry } from "@/components/events/EventPanel";

const MAX_ENTRIES = 200;

/**
 * Map a ComponentEvent status to an EventEntry severity type.
 */
function severityFor(status: string): EventEntry["type"] {
  switch (status) {
    case "Running":
      return "success";
    case "Error":
      return "error";
    case "Stopping":
    case "Stopped":
    case "Removed":
      return "warning";
    default:
      return "info";
  }
}

/**
 * Build a human-readable message for a ComponentEvent.
 */
function formatMessage(event: ComponentEvent): string {
  const kind = event.componentType;
  const id = event.componentId;
  const status = event.status.toLowerCase();
  const base = `${kind} '${id}' ${status}`;
  return event.message ? `${base} — ${event.message}` : base;
}

let idCounter = 0;

/**
 * Hook that subscribes to the shared ComponentEvents SSE stream and
 * maintains a rolling log of ComponentGraph structure/status events.
 *
 * These are the raw server-side events (Added, Removed, Starting,
 * Running, Stopped, Error, etc.) — not query-result or reaction data.
 */
export function useComponentEventLog(instanceId?: string) {
  const [entries, setEntries] = useState<EventEntry[]>([]);
  const instanceRef = useRef(instanceId);
  instanceRef.current = instanceId;

  useEffect(() => {
    const unsub = subscribeComponentEvents((event: ComponentEvent) => {
      // Skip internal components (prefixed with __)
      if (event.componentId.startsWith("__")) return;

      const entry: EventEntry = {
        id: `sse-${++idCounter}`,
        timestamp: event.timestamp || new Date().toISOString(),
        message: formatMessage(event),
        type: severityFor(event.status),
      };

      setEntries((prev) => [entry, ...prev.slice(0, MAX_ENTRIES - 1)]);
    }, instanceId);

    return unsub;
  }, [instanceId]);

  const clear = useCallback(() => setEntries([]), []);

  return { entries, clear };
}
