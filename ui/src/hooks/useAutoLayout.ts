import { useCallback, useLayoutEffect, useRef, useState } from "react";
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
  lockUntil: Map<string, number>,
): number {
  const measured = getMeasuredHeight(nodeLookup, node.id);
  const isExpanded = !!node.data?.expanded;
  const wasExpanded = prevExpandedMap.get(node.id);

  // Expanded state just changed — compute & lock NEW target height.
  // The lock suppresses intermediate measurements during the CSS
  // grid-template-rows transition so neighbours get ONE displacement
  // that the CSS transform transition can animate smoothly.
  if (wasExpanded !== undefined && isExpanded !== wasExpanded) {
    // Lock at the CURRENT measured height for the duration of the CSS
    // grid-template-rows transition.  This suppresses all intermediate
    // deltas.  After the lock expires the real measured height is used,
    // producing one correct displacement.
    console.log(`[EFFECTIVE_H] TOGGLE DETECTED node=${node.id} isExpanded=${isExpanded} measured=${measured}`);
    heightTargets.set(node.id, measured);
    lockUntil.set(node.id, performance.now() + 450);
    console.log(`[EFFECTIVE_H] LOCKED node=${node.id} target=${measured} (freeze at current)`);
    return measured;
  }

  const locked = heightTargets.get(node.id);
  if (locked !== undefined) {
    const deadline = lockUntil.get(node.id) ?? 0;
    const now = performance.now();

    // Still within the CSS transition window — hold the lock regardless
    // of measured height.  This prevents the 20+ small incremental
    // DISPLACE cycles that cancel out the CSS transform animation.
    if (now < deadline) {
      return locked;
    }

    console.log(`[EFFECTIVE_H] LOCK EXPIRED node=${node.id} locked=${locked} measured=${measured} delta=${measured - locked}`);
    heightTargets.delete(node.id);
    lockUntil.delete(node.id);
    return measured;
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
  const lockUntil = useRef<Map<string, number>>(new Map());
  const pendingRecalcTimers = useRef<Set<string>>(new Set());
  const [recalcTick, setRecalcTick] = useState(0);
  const prevRecalcTick = useRef(0);

  useLayoutEffect(() => {
    const tickChanged = recalcTick !== prevRecalcTick.current;
    prevRecalcTick.current = recalcTick;

    if (!dimFingerprint) return;
    const fingerprintChanged = dimFingerprint !== prevFingerprint.current;
    if (!fingerprintChanged && !tickChanged) return;
    prevFingerprint.current = dimFingerprint;

    // If only the tick changed (lock expired) but the fingerprint is
    // stable, silently update prevDims to the real measured heights
    // WITHOUT generating any visible displacement.  This corrects the
    // internal bookkeeping so future interactions use accurate data,
    // but the user never sees a second animation phase.
    console.log(`[LAYOUT] entry fingerprintChanged=${fingerprintChanged} tickChanged=${tickChanged}`);

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
          lockUntil.current,
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

      if (Math.abs(deltaW) > 1 || Math.abs(deltaH) > 1) {
        console.log(`[LAYOUT] SHIFT node=${node.id} prevW=${prev.width} currW=${curr.width} deltaW=${deltaW} prevH=${Math.round(prev.height)} currH=${Math.round(curr.height)} deltaH=${deltaH}`);
        shifts.push({
          id: node.id,
          x: node.position.x,
          y: node.position.y,
          width: curr.width,
          height: curr.height,
          deltaW,
          deltaH,
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
            expandedChanged.has(n.id) && n.data?.expandContentHeight != null
              ? { ...n, data: { ...n.data, expandContentHeight: undefined } }
              : n,
          ),
        );
      }
      return;
    }

    const shiftIds = new Set(shifts.map((s) => s.id));

    setNodes((prev) => {
      // Apply displacement rules using full bounding-box relationships.
      //
      // N = expanding node (shift), C = other node (n)
      //   N: (x1,y1)-(x2,y2)  where x2 = pre-expansion right edge
      //   C: (a1,b1)-(a2,b2)
      //   dx = shift.deltaW, dy = shift.deltaH
      //
      // Rules:
      //   C fully above N  (b2 <= y1)       → stationary
      //   C fully left of N (a2 <= x1)      → stationary
      //   C fully right of N (a1 >= x2)     → shift right by dx
      //   C below N + horizontal overlap    → shift down by dy
      const displaced = prev.map((n) => {
        let dx = 0;
        let dy = 0;

        const cDims = currentDims.get(n.id);
        const cWidth = cDims?.width ?? 180;
        const cHeight = cDims?.height ?? 80;

        for (const shift of shifts) {
          if (n.id === shift.id) continue;

          // N's pre-expansion bounding box
          const prevWidth = shift.width - shift.deltaW;
          const prevHeight = shift.height - shift.deltaH;
          const x1 = shift.x;
          const y1 = shift.y;
          const x2 = shift.x + prevWidth;
          const y2 = shift.y + prevHeight;

          // C's bounding box
          const a1 = n.position.x + dx;
          const b1 = n.position.y + dy;
          const a2 = a1 + cWidth;
          const b2 = b1 + cHeight;

          // C fully above N → stationary
          if (b2 <= y1) continue;

          // C fully left of N → stationary
          if (a2 <= x1) continue;

          // C fully right of N → shift right by dx
          if (a1 >= x2) {
            dx += shift.deltaW;
            continue;
          }

          // C below N's bottom edge AND horizontally overlapping → shift down by dy
          const horizontalOverlap =
            (a1 < x1 && a2 > x2) ||    // C spans wider than N
            (a1 >= x1 && a1 <= x2) ||   // C's left edge within N
            (a2 >= x1 && a2 <= x2);     // C's right edge within N
          if (b1 >= y2 && horizontalOverlap) {
            dy += shift.deltaH;
          }
        }

        if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return n;

        console.log(`[DISPLACE] node=${n.id} dx=${dx} dy=${dy} from=(${n.position.x},${n.position.y}) to=(${n.position.x+dx},${n.position.y+dy})`);
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

      // Clear expandContentHeight from nodes that just toggled.
      const final = result.map((n) =>
        expandedChanged.has(n.id) && n.data?.expandContentHeight != null
          ? { ...n, data: { ...n.data, expandContentHeight: undefined } }
          : n,
      );

      return final;
    });

    // Schedule forced re-evaluation after any active locks expire so that
    // the catch-up displacement fires even if the fingerprint has stabilised.
    for (const [nodeId, deadline] of lockUntil.current) {
      if (!pendingRecalcTimers.current.has(nodeId)) {
        const delay = Math.max(0, deadline - performance.now()) + 20;
        pendingRecalcTimers.current.add(nodeId);
        setTimeout(() => {
          pendingRecalcTimers.current.delete(nodeId);
          setRecalcTick((c) => c + 1);
        }, delay);
      }
    }
  }, [dimFingerprint, recalcTick, store, getNodes, setNodes]);

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
