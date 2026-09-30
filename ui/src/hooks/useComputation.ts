import { useCallback, useEffect, useRef, useState } from "react";
import { inspectComputation } from "@/api/client";
import type { ComputationInspection } from "@/api/types";
import { subscribeComponentEvents } from "./useApi";

interface InspectionState {
  instanceId?: string;
  data?: ComputationInspection;
  error?: string;
  loading: boolean;
}

export function useComputation(instanceId?: string) {
  const [state, setState] = useState<InspectionState>({ loading: true });
  const refreshRef = useRef<(() => Promise<void>) | null>(null);
  const refresh = useCallback(() => { void refreshRef.current?.(); }, []);

  useEffect(() => {
    if (!instanceId) return;
    const controller = new AbortController();
    let fetching = false;
    const load = async () => {
      if (fetching || controller.signal.aborted) return;
      fetching = true;
      try {
        const data = await inspectComputation(instanceId, controller.signal);
        if (!controller.signal.aborted) {
          setState({ instanceId, data, loading: false });
        }
      } catch (error) {
        if (!controller.signal.aborted) {
          setState((previous) => ({
            instanceId,
            data: previous.instanceId === instanceId ? previous.data : undefined,
            loading: false,
            error: error instanceof Error ? error.message : "Unable to inspect computation graph",
          }));
        }
      } finally {
        fetching = false;
      }
    };
    refreshRef.current = load;
    void load();
    // Native graph changes are not all represented by the legacy component SSE stream.
    const timer = window.setInterval(() => { void load(); }, 5000);
    const unsubscribe = subscribeComponentEvents(() => { void load(); }, instanceId);
    return () => {
      controller.abort();
      window.clearInterval(timer);
      unsubscribe();
      refreshRef.current = null;
    };
  }, [instanceId]);

  return {
    data: state.instanceId === instanceId ? state.data : undefined,
    error: state.instanceId === instanceId ? state.error : undefined,
    loading: !!instanceId && (state.instanceId !== instanceId || state.loading),
    refresh,
  };
}
