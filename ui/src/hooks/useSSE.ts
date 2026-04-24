import { useEffect, useRef, useState, useCallback } from "react";
import { subscribeToQueryResults } from "@/api/sse";
import type { ChangeEvent, QueryResultRow } from "@/api/types";

export function useQueryResults(queryId: string | null, instanceId?: string) {
  const [results, setResults] = useState<QueryResultRow[]>([]);
  const [recentChanges, setRecentChanges] = useState<ChangeEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const unsubRef = useRef<(() => void) | null>(null);

  const handleEvent = useCallback((event: ChangeEvent) => {
    setRecentChanges((prev) => [event, ...prev.slice(0, 99)]);

    setResults((prev) => {
      switch (event.type) {
        case "added":
          return [...prev, event.data];
        case "updated":
          return prev.map((row) =>
            isSameRow(row, event.data) ? event.data : row,
          );
        case "deleted":
          return prev.filter((row) => !isSameRow(row, event.data));
        default:
          return prev;
      }
    });
  }, []);

  useEffect(() => {
    if (!queryId) return;

    setConnected(true);
    unsubRef.current = subscribeToQueryResults(
      queryId,
      handleEvent,
      instanceId,
    );

    return () => {
      unsubRef.current?.();
      setConnected(false);
    };
  }, [queryId, instanceId, handleEvent]);

  return { results, recentChanges, connected };
}

function isSameRow(a: QueryResultRow, b: QueryResultRow): boolean {
  const aId = a["id"] ?? a["elementId"] ?? a["_id"];
  const bId = b["id"] ?? b["elementId"] ?? b["_id"];
  return aId !== undefined && aId === bId;
}
