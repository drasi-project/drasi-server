import { useState, useEffect, useCallback, useRef } from "react";
import * as api from "@/api/client";
import type { InstanceInfo, CreateInstanceRequest } from "@/api/types";

export function useInstances() {
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | undefined>(undefined);
  const [loading, setLoading] = useState(true);
  const initialLoad = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const data = await api.listInstances();
      setInstances(data);
      // Auto-select first instance on initial load
      if (initialLoad.current && data.length > 0) {
        setSelectedId(data[0].id);
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
    [refresh],
  );

  const selected = instances.find((i) => i.id === selectedId) ?? null;

  return {
    instances,
    selected,
    selectedId,
    setSelectedId,
    loading,
    refresh,
    create,
  };
}
