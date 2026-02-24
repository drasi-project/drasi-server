import { useCallback, useEffect, useRef } from "react";
import { useReactFlow, useStore, type Viewport } from "@xyflow/react";

const STORAGE_PREFIX = "drasi-canvas-";
const DEBOUNCE_MS = 500;

interface PersistedState {
  positions: Record<string, { x: number; y: number }>;
  expanded: Record<string, boolean>;
  locked: Record<string, boolean>;
  viewport?: Viewport;
}

function storageKey(instanceId: string): string {
  return `${STORAGE_PREFIX}${instanceId}`;
}

export function loadPersistedState(instanceId: string): PersistedState | null {
  try {
    const raw = localStorage.getItem(storageKey(instanceId));
    if (!raw) return null;
    return JSON.parse(raw) as PersistedState;
  } catch {
    return null;
  }
}

function save(instanceId: string, state: PersistedState): void {
  try {
    localStorage.setItem(storageKey(instanceId), JSON.stringify(state));
  } catch {
    // localStorage full or unavailable — silently ignore
  }
}

/**
 * Persist and restore canvas state (positions, expanded, viewport)
 * to localStorage, keyed by instance ID.
 *
 * Must be rendered inside <ReactFlow>.
 */
export function useCanvasPersistence(instanceId: string | undefined) {
  const { getNodes, setNodes, getViewport, setViewport } = useReactFlow();
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restoredRef = useRef<string | null>(null);

  // Stable fingerprint of node positions + expanded states for change detection.
  const fingerprint = useStore((s) => {
    const parts: string[] = [];
    for (const node of s.nodes) {
      const px = Math.round(node.position.x);
      const py = Math.round(node.position.y);
      const exp = node.data?.expanded ? 1 : 0;
      const lck = node.data?.locked ? 1 : 0;
      parts.push(`${node.id}:${px}:${py}:${exp}:${lck}`);
    }
    return parts.sort().join("|");
  });

  // Restore state when instance changes.
  useEffect(() => {
    if (!instanceId || restoredRef.current === instanceId) return;
    restoredRef.current = instanceId;

    const state = loadPersistedState(instanceId);
    if (!state) return;

    // Restore positions, expanded, and locked states.
    setNodes((nodes) =>
      nodes.map((n) => {
        const pos = state.positions[n.id];
        const exp = state.expanded[n.id];
        const lock = state.locked?.[n.id];
        return {
          ...n,
          position: pos ?? n.position,
          draggable: lock ? false : undefined,
          data: {
            ...n.data,
            expanded: exp ?? !!n.data?.expanded,
            locked: lock ?? false,
          },
        };
      }),
    );

    // Restore viewport.
    if (state.viewport) {
      // Defer to next frame so React Flow has measured nodes first.
      requestAnimationFrame(() => setViewport(state.viewport!));
    }
  }, [instanceId, setNodes, setViewport]);

  // Debounced save on change.
  const scheduleSave = useCallback(() => {
    if (!instanceId) return;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      const nodes = getNodes();
      const positions: Record<string, { x: number; y: number }> = {};
      const expanded: Record<string, boolean> = {};
      const locked: Record<string, boolean> = {};
      for (const n of nodes) {
        positions[n.id] = { x: n.position.x, y: n.position.y };
        expanded[n.id] = !!n.data?.expanded;
        locked[n.id] = !!n.data?.locked;
      }
      save(instanceId, { positions, expanded, locked, viewport: getViewport() });
    }, DEBOUNCE_MS);
  }, [instanceId, getNodes, getViewport]);

  // Trigger save whenever fingerprint changes.
  useEffect(() => {
    scheduleSave();
  }, [fingerprint, scheduleSave]);

  // Cleanup timer on unmount.
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);
}
