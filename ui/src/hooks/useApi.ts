import { useState, useEffect, useCallback } from "react";
import axios from "axios";
import * as api from "@/api/client";
import { removeNodeFromPersistedState } from "@/hooks/useCanvasPersistence";
import type {
  SourceStatusResponse,
  QueryConfigResponse,
  ReactionStatusResponse,
  ComponentEvent,
  ComponentStatus,
} from "@/api/types";

// Internal components (introspection source, attach reactions) should be hidden from the UI
const INTERNAL_PREFIX = "__";
function isInternal(id: string): boolean {
  return id.startsWith(INTERNAL_PREFIX);
}

// ---------------------------------------------------------------------------
// Shared EventSource singleton per instance
//
// Browsers limit the number of concurrent HTTP/1.1 connections per domain
// (typically 6). Each EventSource holds one connection open permanently.
// Previously, useSources, useQueries, and useReactions each opened their own
// EventSource to the same /events endpoint — consuming 3 connections just for
// SSE.  This left only 3 connections for API calls, and if any additional SSE
// streams were open (inspector panels), new requests would queue indefinitely.
//
// This module shares a single EventSource per instance across all subscribers.
// ---------------------------------------------------------------------------

type Listener = (event: ComponentEvent) => void;

export type ConnectionState = "connected" | "connecting" | "disconnected";
type ConnectionListener = (state: ConnectionState) => void;

interface SharedES {
  es: EventSource;
  listeners: Set<Listener>;
  connectionListeners: Set<ConnectionListener>;
  connectionState: ConnectionState;
}

const sharedSources = new Map<string, SharedES>();

function getSharedKey(instanceId?: string): string {
  return instanceId ?? "__default__";
}

function notifyConnectionState(shared: SharedES, state: ConnectionState) {
  shared.connectionState = state;
  for (const listener of shared.connectionListeners) {
    listener(state);
  }
}

/**
 * Subscribe to the global component events SSE stream.
 * Multiple subscribers share the same underlying EventSource connection.
 * Returns a cleanup function to unsubscribe.
 */
function subscribeComponentEvents(
  onEvent: Listener,
  instanceId?: string,
): () => void {
  const key = getSharedKey(instanceId);

  let shared = sharedSources.get(key);
  if (!shared) {
    const path = instanceId
      ? `/api/v1/instances/${instanceId}/events`
      : `/api/v1/events`;

    const es = new EventSource(path);
    shared = { es, listeners: new Set(), connectionListeners: new Set(), connectionState: "connecting" };
    const ref = shared;

    es.onopen = () => {
      notifyConnectionState(ref, "connected");
    };

    es.onmessage = (msg) => {
      // Any message (including heartbeats) confirms we're connected
      if (ref.connectionState !== "connected") {
        notifyConnectionState(ref, "connected");
      }
      try {
        const event: ComponentEvent = JSON.parse(msg.data);
        for (const listener of ref.listeners) {
          listener(event);
        }
      } catch {
        // Skip heartbeats and malformed events
      }
    };

    es.onerror = () => {
      // EventSource auto-reconnects; readyState tells us which state we're in
      const state = es.readyState === EventSource.CONNECTING ? "connecting" : "disconnected";
      notifyConnectionState(ref, state);
    };

    sharedSources.set(key, shared);
  }

  shared.listeners.add(onEvent);

  return () => {
    const s = sharedSources.get(key);
    if (!s) return;
    s.listeners.delete(onEvent);
    if (s.listeners.size === 0 && s.connectionListeners.size === 0) {
      s.es.close();
      sharedSources.delete(key);
    }
  };
}

/**
 * Subscribe to SSE connection state changes.
 * Returns the current state immediately via callback, then notifies on changes.
 * Returns a cleanup function to unsubscribe.
 */
export function subscribeConnectionState(
  onState: ConnectionListener,
  instanceId?: string,
): () => void {
  const key = getSharedKey(instanceId);

  let shared = sharedSources.get(key);
  if (!shared) {
    // Create the EventSource if it doesn't exist yet
    // (subscribeComponentEvents creates it, but connection monitoring may start first)
    const path = instanceId
      ? `/api/v1/instances/${instanceId}/events`
      : `/api/v1/events`;

    const es = new EventSource(path);
    shared = { es, listeners: new Set(), connectionListeners: new Set(), connectionState: "connecting" };
    const ref = shared;

    es.onopen = () => {
      notifyConnectionState(ref, "connected");
    };

    es.onmessage = (msg) => {
      if (ref.connectionState !== "connected") {
        notifyConnectionState(ref, "connected");
      }
      try {
        const event: ComponentEvent = JSON.parse(msg.data);
        for (const listener of ref.listeners) {
          listener(event);
        }
      } catch {
        // Skip heartbeats and malformed events
      }
    };

    es.onerror = () => {
      const state = es.readyState === EventSource.CONNECTING ? "connecting" : "disconnected";
      notifyConnectionState(ref, state);
    };

    sharedSources.set(key, shared);
  }

  shared.connectionListeners.add(onState);

  // Immediately notify current state
  onState(shared.connectionState);

  return () => {
    const s = sharedSources.get(key);
    if (!s) return;
    s.connectionListeners.delete(onState);
    if (s.listeners.size === 0 && s.connectionListeners.size === 0) {
      s.es.close();
      sharedSources.delete(key);
    }
  };
}

export function useSources(instanceId?: string) {
  const [sources, setSources] = useState<SourceStatusResponse[]>([]);
  const [loading, setLoading] = useState(true);

  // One-time fetch for initial state (includes full config details)
  const refresh = useCallback(async () => {
    try {
      const data = await api.listSources(instanceId);
      setSources(data.filter((s) => !isInternal(s.id)));
    } catch (e) {
      // Only clear on 404 (instance not found) - preserve data on transient errors
      if (axios.isAxiosError(e) && e.response?.status === 404) {
        setSources([]);
      }
      // Otherwise keep existing data
    } finally {
      setLoading(false);
    }
  }, [instanceId]);

  useEffect(() => {
    setLoading(true);
    refresh();
  }, [refresh]);

  // SSE: apply status changes reactively
  useEffect(() => {
    const unsub = subscribeComponentEvents((event) => {
      if (event.componentType !== "Source") return;
      if (isInternal(event.componentId)) return;

      if (event.status === "Removed") {
        setSources((prev) => prev.filter((s) => s.id !== event.componentId));
        return;
      }

      if (event.status === "Added") {
        // New source added — fetch full details then append
        api.getSource(event.componentId, instanceId).then((full) => {
          setSources((prev) => {
            if (prev.some((s) => s.id === full.id)) return prev;
            return [...prev, full];
          });
        }).catch(() => {
          // Append with minimal info; will be refined on next detail fetch
          setSources((prev) => {
            if (prev.some((s) => s.id === event.componentId)) return prev;
            return [...prev, {
              id: event.componentId,
              kind: "mock",
              status: "Stopped" as ComponentStatus,
              autoStart: false,
            }];
          });
        });
        return;
      }

      // Status update - include error message if present
      setSources((prev) =>
        prev.map((s) =>
          s.id === event.componentId
            ? { ...s, status: event.status, error: event.message }
            : s,
        ),
      );
    }, instanceId);

    return unsub;
  }, [instanceId]);

  return {
    sources,
    loading,
    refresh,
    create: async (req: Parameters<typeof api.createSource>[0]) => {
      await api.createSource(req, instanceId);
      // SSE will deliver the Added event — no need to refresh
    },
    remove: async (id: string) => {
      await api.deleteSource(id, instanceId);
      // Clean up persisted canvas state for this node
      if (instanceId) {
        removeNodeFromPersistedState(instanceId, id);
      }
      // SSE will deliver the Removed event
    },
    start: async (id: string) => {
      await api.startSource(id, instanceId);
      // SSE will deliver the Starting/Running event
    },
    stop: async (id: string) => {
      await api.stopSource(id, instanceId);
      // SSE will deliver the Stopping/Stopped event
    },
  };
}

export function useQueries(instanceId?: string) {
  const [queries, setQueries] = useState<QueryConfigResponse[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const data = await api.listQueries(instanceId);
      setQueries(data.filter((q) => !isInternal(q.id)));
    } catch (e) {
      // Only clear on 404 (instance not found) - preserve data on transient errors
      if (axios.isAxiosError(e) && e.response?.status === 404) {
        setQueries([]);
      }
    } finally {
      setLoading(false);
    }
  }, [instanceId]);

  useEffect(() => {
    setLoading(true);
    refresh();
  }, [refresh]);

  useEffect(() => {
    const unsub = subscribeComponentEvents((event) => {
      if (event.componentType !== "Query") return;
      if (isInternal(event.componentId)) return;

      if (event.status === "Removed") {
        setQueries((prev) => prev.filter((q) => q.id !== event.componentId));
        return;
      }

      if (event.status === "Added") {
        api.getQuery(event.componentId, instanceId).then((full) => {
          setQueries((prev) => {
            if (prev.some((q) => q.id === full.id)) return prev;
            return [...prev, full];
          });
        }).catch(() => {
          setQueries((prev) => {
            if (prev.some((q) => q.id === event.componentId)) return prev;
            return [...prev, {
              id: event.componentId,
              query: "",
              sources: [],
              status: "Stopped" as ComponentStatus,
            }];
          });
        });
        return;
      }

      setQueries((prev) =>
        prev.map((q) =>
          q.id === event.componentId
            ? { ...q, status: event.status, error: event.message }
            : q,
        ),
      );
    }, instanceId);

    return unsub;
  }, [instanceId]);

  return {
    queries,
    loading,
    refresh,
    create: async (req: Parameters<typeof api.createQuery>[0]) => {
      await api.createQuery(req, instanceId);
    },
    remove: async (id: string) => {
      await api.deleteQuery(id, instanceId);
      // Clean up persisted canvas state for this node
      if (instanceId) {
        removeNodeFromPersistedState(instanceId, id);
      }
    },
    start: async (id: string) => {
      await api.startQuery(id, instanceId);
    },
    stop: async (id: string) => {
      await api.stopQuery(id, instanceId);
    },
  };
}

export function useReactions(instanceId?: string) {
  const [reactions, setReactions] = useState<ReactionStatusResponse[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const data = await api.listReactions(instanceId);
      setReactions(data.filter((r) => !isInternal(r.id)));
    } catch (e) {
      // Only clear on 404 (instance not found) - preserve data on transient errors
      if (axios.isAxiosError(e) && e.response?.status === 404) {
        setReactions([]);
      }
    } finally {
      setLoading(false);
    }
  }, [instanceId]);

  useEffect(() => {
    setLoading(true);
    refresh();
  }, [refresh]);

  useEffect(() => {
    const unsub = subscribeComponentEvents((event) => {
      if (event.componentType !== "Reaction") return;
      if (isInternal(event.componentId)) return;

      if (event.status === "Removed") {
        setReactions((prev) => prev.filter((r) => r.id !== event.componentId));
        return;
      }

      if (event.status === "Added") {
        api.getReaction(event.componentId, instanceId).then((full) => {
          setReactions((prev) => {
            if (prev.some((r) => r.id === full.id)) return prev;
            return [...prev, full];
          });
        }).catch(() => {
          setReactions((prev) => {
            if (prev.some((r) => r.id === event.componentId)) return prev;
            return [...prev, {
              id: event.componentId,
              kind: "log",
              status: "Stopped" as ComponentStatus,
              queries: [],
              autoStart: false,
            }];
          });
        });
        return;
      }

      setReactions((prev) =>
        prev.map((r) =>
          r.id === event.componentId
            ? { ...r, status: event.status, error: event.message }
            : r,
        ),
      );
    }, instanceId);

    return unsub;
  }, [instanceId]);

  return {
    reactions,
    loading,
    refresh,
    create: async (req: Parameters<typeof api.createReaction>[0]) => {
      await api.createReaction(req, instanceId);
    },
    remove: async (id: string) => {
      await api.deleteReaction(id, instanceId);
      // Clean up persisted canvas state for this node
      if (instanceId) {
        removeNodeFromPersistedState(instanceId, id);
      }
    },
    start: async (id: string) => {
      await api.startReaction(id, instanceId);
    },
    stop: async (id: string) => {
      await api.stopReaction(id, instanceId);
    },
  };
}

/**
 * Direct API access hook for components that need to call start/stop
 * without managing local state. SSE events will update the UI reactively.
 */
export function useApi() {
  return {
    startSource: api.startSource,
    stopSource: api.stopSource,
    startQuery: api.startQuery,
    stopQuery: api.stopQuery,
    startReaction: api.startReaction,
    stopReaction: api.stopReaction,
  };
}
