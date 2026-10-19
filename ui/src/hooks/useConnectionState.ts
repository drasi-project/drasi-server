import { useState, useEffect } from "react";
import { subscribeConnectionState, type ConnectionState } from "./useApi";

/**
 * Hook that tracks the selected instance's SSE connection state.
 *
 * Returns "connected" | "connecting" | "disconnected" based on the
 * EventSource connection that the UI already maintains for component
 * events. No polling — purely reactive.
 */
export function useConnectionState(instanceId?: string): ConnectionState {
  const [state, setState] = useState<ConnectionState>("connecting");

  useEffect(() => {
    if (!instanceId) {
      setState("unselected");
      return;
    }
    const unsubscribe = subscribeConnectionState(setState, instanceId);
    return unsubscribe;
  }, [instanceId]);

  return state;
}
