import { useState, useEffect, useCallback } from "react";
import { instancePath } from "@/api/paths";
import { subscribeQueryResults, type StreamPayload } from "@/api/query-results-stream";
import type { QueryResultRow } from "@/api/types";

/**
 * Stable key for comparing rows (sorted JSON).
 */
function stableKey(value: unknown): string {
  return JSON.stringify(normalizeValue(value));
}

function normalizeValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(normalizeValue);
  }
  if (value && typeof value === "object") {
    const keys = Object.keys(value as Record<string, unknown>).sort();
    const normalized: Record<string, unknown> = {};
    for (const key of keys) {
      normalized[key] = normalizeValue((value as Record<string, unknown>)[key]);
    }
    return normalized;
  }
  return value;
}

/**
 * Hook to fetch and stream query results for a given query ID.
 * Returns results that stay up-to-date via SSE streaming.
 */
export function useQueryResults(
  queryId: string | null,
  instanceId?: string,
): {
  results: QueryResultRow[];
  loading: boolean;
  error: string | null;
  streaming: boolean;
} {
  const [{ rows: results }, setResultState] = useState<{
    rows: QueryResultRow[];
    index: Map<string, number>;
  }>({ rows: [], index: new Map() });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [streaming, setStreaming] = useState(false);

  // Apply streaming updates to the results set
  const applyStreamUpdate = useCallback((payload: StreamPayload) => {
    setResultState((prev) => {
      const updated = [...prev.rows];
      const index = new Map(prev.index);

      for (const entry of payload.results) {
        switch (entry.type) {
          case "ADD": {
            if (entry.data) {
              const targetKey = stableKey(entry.data);
              const idx = index.get(targetKey);
              if (idx !== undefined && idx < updated.length) {
                updated[idx] = entry.data;
              } else {
                index.set(targetKey, updated.length);
                updated.push(entry.data);
              }
            }
            break;
          }
          case "DELETE": {
            if (entry.data) {
              const targetKey = stableKey(entry.data);
              const idx = index.get(targetKey);
              if (idx !== undefined && idx < updated.length) {
                // Remove and rebuild index for shifted elements
                updated.splice(idx, 1);
                index.delete(targetKey);
                // Re-index entries after the removed one
                for (const [k, v] of index) {
                  if (v > idx) index.set(k, v - 1);
                }
              }
            }
            break;
          }
          case "UPDATE":
          case "aggregation": {
            if (entry.before) {
              const beforeKey = stableKey(entry.before);
              const idx = index.get(beforeKey);
              if (idx !== undefined && idx < updated.length && entry.after) {
                updated[idx] = entry.after;
                index.delete(beforeKey);
                index.set(stableKey(entry.after), idx);
              } else if (entry.after) {
                const afterKey = stableKey(entry.after);
                const afterIdx = index.get(afterKey);
                if (afterIdx !== undefined && afterIdx < updated.length) {
                  updated[afterIdx] = entry.after;
                } else {
                  index.set(afterKey, updated.length);
                  updated.push(entry.after);
                }
              }
            } else if (entry.after) {
              const afterKey = stableKey(entry.after);
              const afterIdx = index.get(afterKey);
              if (afterIdx !== undefined && afterIdx < updated.length) {
                updated[afterIdx] = entry.after;
              } else {
                index.set(afterKey, updated.length);
                updated.push(entry.after);
              }
            }
            break;
          }
          case "noop":
            break;
        }
      }

      return { rows: updated, index };
    });
  }, []);

  useEffect(() => {
    setResultState({ rows: [], index: new Map() });
    if (!queryId || !instanceId) {
      setLoading(false);
      setError(null);
      setStreaming(false);
      return;
    }

    setLoading(true);
    setError(null);
    setStreaming(false);

    const path = `/api/v1${instancePath(instanceId, "queries", queryId, "attach")}`;
    return subscribeQueryResults(path, {
      onSnapshot(data) {
        const seen = new Map<string, number>();
        const deduped: QueryResultRow[] = [];
        for (const row of data) {
          const key = stableKey(row);
          if (!seen.has(key)) {
            seen.set(key, deduped.length);
            deduped.push(row);
          }
        }
        setResultState({ rows: deduped, index: seen });
      },
      onChange: applyStreamUpdate,
      onStatus(status) {
        setLoading(status.loading);
        setStreaming(status.streaming);
        setError(status.error);
      },
    });
  }, [queryId, instanceId, applyStreamUpdate]);

  return { results, loading, error, streaming };
}
