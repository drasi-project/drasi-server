import { useCallback, useLayoutEffect, useRef } from "react";
import { useStore, useStoreApi, useReactFlow, type Node } from "@xyflow/react";

const NODE_MARGIN = 20;

// Known target widths from framer-motion animate props in each node component.
const TARGET_WIDTHS: Record<string, { collapsed: number; expanded: number }> = {
  sourceNode: { collapsed: 180, expanded: 320 },
  queryNode: { collapsed: 180, expanded: 360 },
  reactionNode: { collapsed: 180, expanded: 300 },
};

function getTargetWidth(node: Node): number {
  const spec = TARGET_WIDTHS[node.type ?? ""];
  if (!spec) return 180;
  return node.data?.expanded ? spec.expanded : spec.collapsed;
}

function getMeasuredHeight(
  nodeLookup: Map<string, { measured?: { height?: number } }>,
  id: string,
): number {
  const entry = nodeLookup.get(id);
  return (entry as { measured?: { height?: number } })?.measured?.height ?? 0;
}

/**
 * Return the effective height for layout calculations.
 *
 * When a node's expanded state just changed, we compute the target (final)
 * height immediately using the pre-measured expandContentHeight and lock it in
 * `heightTargets`.  While the CSS transition is in progress the measured height
 * gradually approaches the target; we keep returning the locked target so that
 * intermediate measurements don't create reverse deltas (bouncing).  Once the
 * measured height converges we release the lock.
 */
function getEffectiveHeight(
  node: Node,
  nodeLookup: Map<string, { measured?: { height?: number } }>,
  prevExpandedMap: Map<string, boolean>,
  heightTargets: Map<string, number>,
): number {
  const measured = getMeasuredHeight(nodeLookup, node.id);
  const isExpanded = !!node.data?.expanded;
  const wasExpanded = prevExpandedMap.get(node.id);

  // Expanded state just changed — compute & lock NEW target height.
  // Must check BEFORE the lock so that collapse overwrites the expand lock.
  if (wasExpanded !== undefined && isExpanded !== wasExpanded) {
    const contentH = Number(node.data?.expandContentHeight ?? 0);
    if (contentH > 0) {
      const target = isExpanded
        ? measured + contentH
        : Math.max(0, measured - contentH);
      heightTargets.set(node.id, target);
      return target;
    }
  }

  // Still transitioning — keep using locked target to prevent
  // small measurement fluctuations from restarting CSS transitions.
  const locked = heightTargets.get(node.id);
  if (locked !== undefined) {
    return locked;
  }

  return measured;
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Clamp a moving rect so it doesn't overlap any obstacle rect.
 * The moving rect is pushed back to the nearest edge + margin.
 * Obstacles stay fixed.
 */
function clampAgainstObstacles(
  moving: Rect,
  obstacles: Rect[],
): { x: number; y: number } {
  let { x, y } = moving;

  for (let iter = 0; iter < 3; iter++) {
    for (const obs of obstacles) {
      const overlapX =
        x < obs.x + obs.w + NODE_MARGIN && x + moving.w + NODE_MARGIN > obs.x;
      const overlapY =
        y < obs.y + obs.h + NODE_MARGIN && y + moving.h + NODE_MARGIN > obs.y;

      if (!overlapX || !overlapY) continue;

      const pushRight = obs.x + obs.w + NODE_MARGIN - x;
      const pushLeft = x + moving.w + NODE_MARGIN - obs.x;
      const pushDown = obs.y + obs.h + NODE_MARGIN - y;
      const pushUp = y + moving.h + NODE_MARGIN - obs.y;

      const min = Math.min(pushRight, pushLeft, pushDown, pushUp);

      if (min === pushRight) x = obs.x + obs.w + NODE_MARGIN;
      else if (min === pushLeft) x = obs.x - moving.w - NODE_MARGIN;
      else if (min === pushDown) y = obs.y + obs.h + NODE_MARGIN;
      else y = obs.y - moving.h - NODE_MARGIN;
    }
  }

  return { x, y };
}

/**
 * Stable fingerprint: expanded flags (from nodes array) + measured heights.
 * Only re-renders when something actually changes.
 */
function dimensionSelector(
  s: {
    nodeLookup: Map<string, { measured?: { width?: number; height?: number } }>;
    nodes: Array<{ id: string; data?: Record<string, unknown> }>;
  },
): string {
  const expandedMap = new Map<string, boolean>();
  for (const node of s.nodes) {
    expandedMap.set(node.id, !!node.data?.expanded);
  }
  const parts: string[] = [];
  for (const [id, node] of s.nodeLookup) {
    const h = Math.round(node.measured?.height ?? 0);
    const exp = expandedMap.get(id) ? 1 : 0;
    if (h > 0) parts.push(`${id}:${h}:${exp}`);
  }
  return parts.sort().join("|");
}

interface Dimensions {
  width: number;
  height: number;
}

/**
 * Delta-displacement layout using bottom-right boundary rules:
 *
 * When a node resizes, let (x1,y1) = its top-left, (x2,y2) = its bottom-right.
 * For each OTHER node with anchor (a,b):
 *   1. If a >= x2 → shift right/left by deltaW
 *   2. If a < x2 AND b <= y2 → shift down/up by deltaH
 *   3. Otherwise → stationary
 *
 * After displacement, collision clamping prevents overlaps.
 */
export function useAutoLayout() {
  const { setNodes, getNodes } = useReactFlow();
  const store = useStoreApi();

  const dimFingerprint = useStore(dimensionSelector);
  const prevFingerprint = useRef<string>("");
  const prevDims = useRef<Map<string, Dimensions>>(new Map());
  const prevExpanded = useRef<Map<string, boolean>>(new Map());
  const heightTargets = useRef<Map<string, number>>(new Map());

  useLayoutEffect(() => {
    if (!dimFingerprint || dimFingerprint === prevFingerprint.current) return;
    prevFingerprint.current = dimFingerprint;

    const { nodeLookup } = store.getState();
    const nodes = getNodes();
    if (nodes.length === 0) return;

    const currentDims = new Map<string, Dimensions>();
    for (const node of nodes) {
      currentDims.set(node.id, {
        width: getTargetWidth(node),
        height: getEffectiveHeight(
          node,
          nodeLookup,
          prevExpanded.current,
          heightTargets.current,
        ),
      });
    }

    if (prevDims.current.size === 0) {
      prevDims.current = currentDims;
      // Seed expanded state tracking
      for (const node of nodes) {
        prevExpanded.current.set(node.id, !!node.data?.expanded);
      }
      return;
    }

    // Collect nodes whose dimensions changed.
    const shifts: Array<{
      id: string;
      x: number; y: number;
      width: number; height: number;
      deltaW: number; deltaH: number;
    }> = [];

    for (const node of nodes) {
      const prev = prevDims.current.get(node.id);
      const curr = currentDims.get(node.id)!;
      if (!prev) continue;

      const deltaW = curr.width - prev.width;
      const deltaH = Math.round(curr.height) - Math.round(prev.height);

      // Skip height-based shifts for nodes whose vertical displacement was
      // already applied in handleToggle (same setNodes call as expansion).
      const skipHeight = !!node.data?.heightShiftApplied;
      const effectiveDeltaH = skipHeight ? 0 : deltaH;

      if (Math.abs(deltaW) > 1 || Math.abs(effectiveDeltaH) > 1) {
        shifts.push({
          id: node.id,
          x: node.position.x,
          y: node.position.y,
          width: curr.width,
          height: curr.height,
          deltaW,
          deltaH: effectiveDeltaH,
        });
      }
    }

    prevDims.current = currentDims;

    // Update expanded state tracking for next comparison
    const expandedChanged = new Set<string>();
    for (const node of nodes) {
      const isExp = !!node.data?.expanded;
      if (prevExpanded.current.get(node.id) !== isExp) {
        expandedChanged.add(node.id);
      }
      prevExpanded.current.set(node.id, isExp);
    }

    if (shifts.length === 0) {
      // Even with no shifts, clean up flags from nodes that toggled
      if (expandedChanged.size > 0) {
        setNodes((prev) =>
          prev.map((n) =>
            expandedChanged.has(n.id) && (n.data?.expandContentHeight != null || n.data?.heightShiftApplied)
              ? { ...n, data: { ...n.data, expandContentHeight: undefined, heightShiftApplied: undefined } }
              : n,
          ),
        );
      }
      return;
    }

    const shiftIds = new Set(shifts.map((s) => s.id));

    setNodes((prev) => {
      // Apply spec displacement rules using bottom-right boundary.
      const displaced = prev.map((n) => {
        let dx = 0;
        let dy = 0;

        for (const shift of shifts) {
          if (n.id === shift.id) continue;

          // Use pre-expansion right edge to determine which nodes were
          // "to the right" before this resize. This prevents the expanding
          // node from growing PAST a neighbor and switching it to rule 2.
          const prevWidth = shift.width - shift.deltaW;
          const prevX2 = shift.x + prevWidth;
          const a = n.position.x;
          const b = n.position.y;

          if (a >= prevX2) {
            // Rule 1: node was to the right of the old right edge → shift horizontally
            dx += shift.deltaW;
          } else if (b >= shift.y) {
            // Rule 2: node is within horizontal span and at/below top → shift vertically
            dy += shift.deltaH;
          }
          // Rule 3: otherwise stationary
        }

        if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return n;

        return {
          ...n,
          position: { x: n.position.x + dx, y: n.position.y + dy },
        };
      });

      // Collision clamping: displaced nodes must not overlap stationary ones.
      const obstacles: Rect[] = [];
      const movedNodes: Array<{ index: number; rect: Rect }> = [];

      for (let i = 0; i < displaced.length; i++) {
        const n = displaced[i];
        const dims = currentDims.get(n.id);
        const rect: Rect = {
          x: n.position.x,
          y: n.position.y,
          w: dims?.width ?? 180,
          h: dims?.height ?? 80,
        };
        const orig = prev[i];
        const didMove =
          Math.abs(n.position.x - orig.position.x) > 1 ||
          Math.abs(n.position.y - orig.position.y) > 1;

        if (!didMove || shiftIds.has(n.id)) {
          obstacles.push(rect);
        } else {
          movedNodes.push({ index: i, rect });
        }
      }

      const result = [...displaced];
      for (const { index, rect } of movedNodes) {
        const clamped = clampAgainstObstacles(rect, obstacles);
        if (
          Math.abs(clamped.x - result[index].position.x) > 0.5 ||
          Math.abs(clamped.y - result[index].position.y) > 0.5
        ) {
          result[index] = { ...result[index], position: clamped };
        }
        obstacles.push({ ...rect, x: clamped.x, y: clamped.y });
      }

      // Clear expandContentHeight and heightShiftApplied from nodes that just toggled.
      const final = result.map((n) =>
        expandedChanged.has(n.id) && (n.data?.expandContentHeight != null || n.data?.heightShiftApplied)
          ? { ...n, data: { ...n.data, expandContentHeight: undefined, heightShiftApplied: undefined } }
          : n,
      );

      return final;
    });
  }, [dimFingerprint, store, getNodes, setNodes]);

  /**
   * Synchronously clamp a dragged node's proposed position against all
   * other nodes. Called BEFORE React Flow applies the position change.
   */
  const clampDragPosition = useCallback(
    (
      draggedId: string,
      proposedX: number,
      proposedY: number,
    ): { x: number; y: number } => {
      const { nodeLookup } = store.getState();
      const nodes = getNodes();

      const draggedNode = nodes.find((n) => n.id === draggedId);
      if (!draggedNode) return { x: proposedX, y: proposedY };

      const entry = nodeLookup.get(draggedId) as
        | { measured?: { width?: number; height?: number } }
        | undefined;
      const movingRect: Rect = {
        x: proposedX,
        y: proposedY,
        w: entry?.measured?.width ?? getTargetWidth(draggedNode),
        h: entry?.measured?.height ?? 80,
      };

      const obstacles: Rect[] = [];
      for (const n of nodes) {
        if (n.id === draggedId) continue;
        const e = nodeLookup.get(n.id) as
          | { measured?: { width?: number; height?: number } }
          | undefined;
        obstacles.push({
          x: n.position.x,
          y: n.position.y,
          w: e?.measured?.width ?? getTargetWidth(n),
          h: e?.measured?.height ?? 80,
        });
      }

      return clampAgainstObstacles(movingRect, obstacles);
    },
    [store, getNodes],
  );

  return { clampDragPosition };
}
