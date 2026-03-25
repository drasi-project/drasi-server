import { useCallback, useLayoutEffect, useRef } from "react";
import { useStore, useReactFlow, type Node } from "@xyflow/react";

const NODE_MARGIN = 20;

// Fixed target dimensions per node type — used for layout displacement
// calculations so the final size is known instantly (no measurement needed).
// Values must match the collapsedWidth/expandedWidth/expandedHeight props
// passed to NodeShell in each node component.
const TARGET_DIMS: Record<
  string,
  { collapsedW: number; expandedW: number; collapsedH: number; expandedH: number }
> = {
  sourceNode:   { collapsedW: 180, expandedW: 320, collapsedH: 92, expandedH: 250 },
  queryNode:    { collapsedW: 180, expandedW: 420, collapsedH: 92, expandedH: 280 },
  reactionNode: { collapsedW: 180, expandedW: 300, collapsedH: 92, expandedH: 180 },
};

function getTargetWidth(node: Node): number {
  const spec = TARGET_DIMS[node.type ?? ""];
  if (!spec) return 180;
  return node.data?.expanded ? spec.expandedW : spec.collapsedW;
}

function getTargetHeight(node: Node): number {
  const spec = TARGET_DIMS[node.type ?? ""];
  if (!spec) return 92;
  return node.data?.expanded ? spec.expandedH : spec.collapsedH;
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
 * Stable fingerprint: expanded flags from nodes array.
 * Only re-renders when a node's expanded state changes.
 */
function dimensionSelector(
  s: {
    nodes: Array<{ id: string; type?: string; data?: Record<string, unknown> }>;
  },
): string {
  const parts: string[] = [];
  for (const node of s.nodes) {
    const exp = node.data?.expanded ? 1 : 0;
    parts.push(`${node.id}:${exp}`);
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

  const dimFingerprint = useStore(dimensionSelector);
  const prevFingerprint = useRef<string>("");
  const prevDims = useRef<Map<string, Dimensions>>(new Map());

  useLayoutEffect(() => {
    if (!dimFingerprint) return;
    if (dimFingerprint === prevFingerprint.current) return;
    prevFingerprint.current = dimFingerprint;

    const nodes = getNodes();
    if (nodes.length === 0) return;

    const currentDims = new Map<string, Dimensions>();
    for (const node of nodes) {
      currentDims.set(node.id, {
        width: getTargetWidth(node),
        height: getTargetHeight(node),
      });
    }

    if (prevDims.current.size === 0) {
      prevDims.current = currentDims;
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
      const deltaH = curr.height - prev.height;

      if (Math.abs(deltaW) > 1 || Math.abs(deltaH) > 1) {
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

    if (shifts.length === 0) return;

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

      return result;
    });
  }, [dimFingerprint, getNodes, setNodes]);

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
      const nodes = getNodes();

      const draggedNode = nodes.find((n) => n.id === draggedId);
      if (!draggedNode) return { x: proposedX, y: proposedY };

      const movingRect: Rect = {
        x: proposedX,
        y: proposedY,
        w: getTargetWidth(draggedNode),
        h: getTargetHeight(draggedNode),
      };

      const obstacles: Rect[] = [];
      for (const n of nodes) {
        if (n.id === draggedId) continue;
        obstacles.push({
          x: n.position.x,
          y: n.position.y,
          w: getTargetWidth(n),
          h: getTargetHeight(n),
        });
      }

      return clampAgainstObstacles(movingRect, obstacles);
    },
    [getNodes],
  );

  return { clampDragPosition };
}
