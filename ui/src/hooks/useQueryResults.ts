import { useState, useEffect, useRef, useCallback } from "react";
import * as api from "@/api/client";
import type { QueryResultRow } from "@/api/types";

interface StreamPayload {
  results?: Array<{
    type: "ADD" | "DELETE" | "UPDATE" | "aggregation" | "noop";
    data?: QueryResultRow;
    before?: QueryResultRow;
    after?: QueryResultRow;
  }>;
}

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
  const [results, setResults] = useState<QueryResultRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [streaming, setStreaming] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  // Map from stableKey → index for O(1) lookups during stream updates
  const indexRef = useRef<Map<string, number>>(new Map());

  // Apply streaming updates to the results set
  const applyStreamUpdate = useCallback((payload: StreamPayload) => {
    if (!payload?.results) return;

    setResults((prev) => {
      const updated = [...prev];
      const index = new Map(indexRef.current);

      for (const entry of payload.results!) {
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

      indexRef.current = index;
      return updated;
    });
  }, []);

  useEffect(() => {
    if (!queryId) {
      setResults([]);
      indexRef.current = new Map();
      setLoading(false);
      setError(null);
      setStreaming(false);
      return;
    }

    // Cleanup previous connections
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }

    setLoading(true);
    setError(null);
    setStreaming(false);

    const abortController = new AbortController();
    abortRef.current = abortController;

    // Fetch initial results
    api
      .getQueryResults(queryId, instanceId)
      .then((data) => {
        if (abortController.signal.aborted) return;
        // Deduplicate and build index
        const seen = new Map<string, number>();
        const deduped: QueryResultRow[] = [];
        for (const row of data) {
          const key = stableKey(row);
          if (!seen.has(key)) {
            seen.set(key, deduped.length);
            deduped.push(row);
          }
        }
        indexRef.current = seen;
        setResults(deduped);
        setLoading(false);

        // Start streaming
        const path = instanceId
          ? `/api/v1/instances/${instanceId}/queries/${queryId}/attach`
          : `/api/v1/queries/${queryId}/attach`;
        const es = new EventSource(path);
        eventSourceRef.current = es;

        es.onopen = () => {
          if (!abortController.signal.aborted) {
            setStreaming(true);
          }
        };

        es.onmessage = (event) => {
          if (abortController.signal.aborted) return;
          try {
            const payload: StreamPayload = JSON.parse(event.data);
            applyStreamUpdate(payload);
          } catch {
            // Skip heartbeats and malformed events
          }
        };

        es.onerror = () => {
          // EventSource auto-reconnects; just update streaming state
          if (!abortController.signal.aborted) {
            setStreaming(false);
          }
        };
      })
      .catch((err) => {
        if (abortController.signal.aborted) return;
        setError(String(err));
        setLoading(false);
      });

    return () => {
      abortController.abort();
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
    };
  }, [queryId, instanceId, applyStreamUpdate]);

  return { results, loading, error, streaming };
}
