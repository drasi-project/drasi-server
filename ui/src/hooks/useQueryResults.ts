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

  // Apply streaming updates to the results set
  const applyStreamUpdate = useCallback((payload: StreamPayload) => {
    if (!payload?.results) return;

    setResults((prev) => {
      const updated = [...prev];

      for (const entry of payload.results!) {
        switch (entry.type) {
          case "ADD": {
            if (entry.data) {
              const targetKey = stableKey(entry.data);
              const idx = updated.findIndex((r) => stableKey(r) === targetKey);
              if (idx >= 0) {
                updated[idx] = entry.data;
              } else {
                updated.push(entry.data);
              }
            }
            break;
          }
          case "DELETE": {
            if (entry.data) {
              const targetKey = stableKey(entry.data);
              const idx = updated.findIndex((r) => stableKey(r) === targetKey);
              if (idx >= 0) {
                updated.splice(idx, 1);
              }
            }
            break;
          }
          case "UPDATE":
          case "aggregation": {
            if (entry.before) {
              const beforeKey = stableKey(entry.before);
              const idx = updated.findIndex((r) => stableKey(r) === beforeKey);
              if (idx >= 0 && entry.after) {
                updated[idx] = entry.after;
              } else if (entry.after) {
                const afterKey = stableKey(entry.after);
                const afterIdx = updated.findIndex((r) => stableKey(r) === afterKey);
                if (afterIdx >= 0) {
                  updated[afterIdx] = entry.after;
                } else {
                  updated.push(entry.after);
                }
              }
            } else if (entry.after) {
              const afterKey = stableKey(entry.after);
              const afterIdx = updated.findIndex((r) => stableKey(r) === afterKey);
              if (afterIdx >= 0) {
                updated[afterIdx] = entry.after;
              } else {
                updated.push(entry.after);
              }
            }
            break;
          }
          case "noop":
            break;
        }
      }

      return updated;
    });
  }, []);

  useEffect(() => {
    if (!queryId) {
      setResults([]);
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
        // Deduplicate
        const seen = new Set<string>();
        const deduped: QueryResultRow[] = [];
        for (const row of data) {
          const key = stableKey(row);
          if (!seen.has(key)) {
            seen.add(key);
            deduped.push(row);
          }
        }
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
