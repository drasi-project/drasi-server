import { useState, useEffect, useCallback, useRef } from "react";
import * as api from "@/api/client";
import type { InstanceInfo, CreateInstanceRequest } from "@/api/types";

const INSTANCE_KEY = "drasi-selected-instance";

/** Read the `?instance=` search param from the current URL. */
function getUrlInstanceParam(): string | undefined {
  try {
    const params = new URLSearchParams(window.location.search);
    return params.get("instance") ?? undefined;
  } catch {
    return undefined;
  }
}

/** Update the URL search param without a full page reload. */
function setUrlInstanceParam(id: string | undefined) {
  try {
    const url = new URL(window.location.href);
    if (id) {
      url.searchParams.set("instance", id);
    } else {
      url.searchParams.delete("instance");
    }
    window.history.replaceState({}, "", url.toString());
  } catch {
    /* ignore */
  }
}

export function useInstances() {
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  // The instance ID that was requested via URL (if any), preserved for "not found" UX
  const urlRequestedId = useRef<string | undefined>(getUrlInstanceParam());
  const [requestedNotFound, setRequestedNotFound] = useState<string | undefined>(undefined);

  const [selectedId, setSelectedIdState] = useState<string | undefined>(() => {
    // If URL param is specified, defer selection until refresh validates it
    const fromUrl = getUrlInstanceParam();
    if (fromUrl) return undefined;
    try { return localStorage.getItem(INSTANCE_KEY) ?? undefined; } catch { return undefined; }
  });
  const [loading, setLoading] = useState(true);
  const initialLoad = useRef(true);

  const setSelectedId = useCallback((id: string | undefined) => {
    setSelectedIdState(id);
    setRequestedNotFound(undefined);
    setUrlInstanceParam(id);
    try {
      if (id) localStorage.setItem(INSTANCE_KEY, id);
      else localStorage.removeItem(INSTANCE_KEY);
    } catch { /* ignore */ }
  }, []);

  const refresh = useCallback(async () => {
    try {
      const data = await api.listInstances();
      setInstances(data);

      if (initialLoad.current && data.length > 0) {
        const requested = urlRequestedId.current;

        if (requested) {
          // URL param was provided — check if the instance exists
          if (data.some((i) => i.id === requested)) {
            // Exists — select it
            setSelectedIdState(requested);
            setRequestedNotFound(undefined);
          } else {
            // Doesn't exist — show picker, don't select any instance
            setRequestedNotFound(requested);
            setSelectedIdState(undefined);
          }
        } else {
          // No URL param — use localStorage or first instance
          setSelectedIdState((prev) => {
            if (prev && data.some((i) => i.id === prev)) return prev;
            const first = data[0].id;
            setUrlInstanceParam(first);
            try { localStorage.setItem(INSTANCE_KEY, first); } catch { /* ignore */ }
            return first;
          });
        }
        initialLoad.current = false;
      }
    } catch {
      // Server may not be ready
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const create = useCallback(
    async (req: CreateInstanceRequest) => {
      await api.createInstance(req);
      await refresh();
      setSelectedId(req.id);
    },
    [refresh, setSelectedId],
  );

  /** Dismiss the "not found" banner */
  const dismissNotFound = useCallback(() => {
    setRequestedNotFound(undefined);
  }, []);

  const selected = instances.find((i) => i.id === selectedId) ?? null;

  return {
    instances,
    selected,
    selectedId,
    setSelectedId,
    loading,
    refresh,
    create,
    /** The instance ID from the URL that was not found (undefined if N/A or dismissed) */
    requestedNotFound,
    dismissNotFound,
  };
}
