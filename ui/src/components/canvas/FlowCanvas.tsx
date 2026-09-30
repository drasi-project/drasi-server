import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  ControlButton,
  MiniMap,
  Panel,
  useNodesState,
  useEdgesState,
  useReactFlow,
  type Node,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Trash2, Pin, Lock, LockOpen, LayoutGrid } from "lucide-react";

import SourceNode from "./SourceNode";
import QueryNode from "./QueryNode";
import ReactionNode from "./ReactionNode";
import ComputationNode from "./ComputationNode";
import AnimatedEdge from "./AnimatedEdge";
import { CanvasLockedContext } from "./CanvasLockedContext";
import { buildFlowGraph, layoutFlowNodes, type PipelineData, type CanvasComponentType } from "@/utils/graph";
import { computationColor } from "@/utils/computation";
import { useAutoLayout } from "@/hooks/useAutoLayout";
import { useCanvasPersistence, loadPersistedState } from "@/hooks/useCanvasPersistence";

const nodeTypes = {
  sourceNode: SourceNode,
  queryNode: QueryNode,
  reactionNode: ReactionNode,
  computationNode: ComputationNode,
};

const edgeTypes = {
  animatedEdge: AnimatedEdge,
};

interface FlowCanvasProps {
  data: PipelineData;
  instanceId?: string;
  showInternal: boolean;
  onNodeClick?: (nodeId: string, type: CanvasComponentType) => void;
  onPaneClick?: () => void;
  onDeleteNodes?: (nodeIds: Array<{ id: string; type: string }>) => void;
}

interface CollisionApi {
  clampDragPosition: (id: string, x: number, y: number) => { x: number; y: number };
}

interface CanvasApi {
  collision: CollisionApi | null;
  fitView: () => void;
}

const CANVAS_LOCK_KEY = "drasi-canvas-locked";

function AutoLayout({
  onApiRef,
  instanceId,
}: {
  onApiRef: React.MutableRefObject<CanvasApi | null>;
  instanceId?: string;
}) {
  const layoutApi = useAutoLayout();
  const { fitView } = useReactFlow();
  onApiRef.current = {
    collision: layoutApi,
    fitView: () => fitView({ padding: 0.2, duration: 300 }),
  };
  
  useCanvasPersistence(instanceId);
  return null;
}

export default function FlowCanvas({ data, instanceId, showInternal, onNodeClick, onPaneClick, onDeleteNodes }: FlowCanvasProps) {
  // Build initial nodes and pre-apply any persisted state (positions, expanded,
  // locked) so that nodes mount at their correct size and position. This avoids
  // a visible shrink/grow animation when Framer Motion transitions from the
  // default collapsed width to the persisted expanded width.
  const { nodes: initialNodes, edges: initialEdges } = useMemo(() => {
    const graph = buildFlowGraph(data, showInternal);
    if (!instanceId) return graph;
    const persisted = loadPersistedState(instanceId);
    if (!persisted) return graph;
    return {
      edges: graph.edges,
      nodes: graph.nodes.map((n) => {
        const component = data.computation?.components.find((c) => c.id === n.data.id);
        const oldId = `${component?.role.toLowerCase()}-${n.data.id}`;
        const pos = persisted.positions[n.id] ?? persisted.positions[oldId];
        const exp = persisted.expanded[n.id] ?? persisted.expanded[oldId];
        const lock = persisted.locked?.[n.id] ?? persisted.locked?.[oldId];
        return {
          ...n,
          position: pos ?? n.position,
          draggable: lock ? false : undefined,
          data: {
            ...n.data,
            expanded: exp ?? false,
            locked: lock ?? false,
          },
        };
      }),
    };
  }, [data, instanceId, showInternal]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);
  const canvasApiRef = useRef<CanvasApi | null>(null);
  const [canvasLocked, setCanvasLocked] = useState(() => {
    try { return localStorage.getItem(CANVAS_LOCK_KEY) === "true"; } catch { return false; }
  });
  const [pendingDelete, setPendingDelete] = useState<Node[] | null>(null);

  const toggleCanvasLock = useCallback(() => {
    setCanvasLocked((prev) => {
      const next = !prev;
      try { localStorage.setItem(CANVAS_LOCK_KEY, String(next)); } catch { /* ignore */ }
      return next;
    });
  }, []);

  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      const clampedChanges = changes.map((c) => {
        if (c.type === "position" && c.position && canvasApiRef.current?.collision) {
          const clamped = canvasApiRef.current.collision.clampDragPosition(
            c.id,
            c.position.x,
            c.position.y,
          );
          return { ...c, position: clamped };
        }
        return c;
      });
      onNodesChange(clampedChanges);
    },
    [onNodesChange],
  );

  // Sync when pipeline data changes
  useEffect(() => {
    const { nodes: newNodes, edges: newEdges } = buildFlowGraph(data, showInternal);
    setNodes((prev) => {
      const previous = new Map(prev.map((node) => [node.id, node]));
      const visibilityChanged = newNodes.some((node) => previous.has(node.id) && previous.get(node.id)?.hidden !== node.hidden);
      const merged = newNodes.map((n) => {
        const existing = previous.get(n.id);
        if (existing) {
          return {
            ...n,
            position: existing.position,
            draggable: !canvasLocked && !existing.data?.locked,
            selected: !n.hidden && existing.selected,
            data: {
              ...n.data,
              expanded: existing.data.expanded,
              locked: existing.data.locked,
            },
          };
        }
        return n;
      });
      if (!visibilityChanged || canvasLocked) return merged;
      return layoutFlowNodes(merged, newEdges).map((node) =>
        node.data.locked ? { ...node, position: previous.get(node.id)?.position ?? node.position } : node);
    });
    setEdges(newEdges);
  }, [data, showInternal, canvasLocked, setNodes, setEdges]);

  // Update draggable state when canvas lock changes (without mutating node data)
  useEffect(() => {
    setNodes((prev) =>
      prev.map((n) => ({
        ...n,
        draggable: canvasLocked ? false : !n.data?.locked,
      })),
    );
  }, [canvasLocked, setNodes]);

  const handleNodeClick = useCallback(
    (event: React.MouseEvent, node: Node) => {
      // Don't open inspector when shift-clicking (multi-select)
      if (event.shiftKey) return;
      const type = node.data.componentType;
      if (typeof node.data.id === "string"
        && (type === "source" || type === "query" || type === "reaction" || type === "computation")) {
        onNodeClick?.(node.data.id, type);
      }
    },
    [onNodeClick],
  );

  // Delete selected nodes (respecting locks)
  const deleteSelectedNodes = useCallback(() => {
    if (canvasLocked || !onDeleteNodes) return;
    const selected = nodes.filter(
      (n) => n.selected && !n.hidden && !n.data?.locked && n.data.componentType !== "computation",
    );
    if (selected.length === 0) return;
    setPendingDelete(selected);
  }, [nodes, canvasLocked, onDeleteNodes]);

  // Toggle lock on all selected nodes
  const toggleSelectedNodesLock = useCallback(() => {
    if (canvasLocked) return;
    const selected = nodes.filter((n) => n.selected);
    if (selected.length === 0) return;
    // If any selected node is unlocked, lock all; otherwise unlock all
    const shouldLock = selected.some((n) => !n.data?.locked);
    setNodes((prev) =>
      prev.map((n) =>
        n.selected
          ? {
              ...n,
              draggable: !shouldLock,
              data: { ...n.data, locked: shouldLock },
            }
          : n,
      ),
    );
  }, [nodes, canvasLocked, setNodes]);

  // Use actual topology rather than fixed source/query/reaction columns.
  const autoLayoutNodes = useCallback(() => {
    setNodes((prev) => layoutFlowNodes(prev, edges));

    // Fit view after layout with a small delay for React to render new positions
    setTimeout(() => {
      canvasApiRef.current?.fitView();
    }, 50);
  }, [setNodes, edges]);

  const confirmDelete = useCallback(() => {
    if (!pendingDelete) return;
    if (onDeleteNodes) {
      onDeleteNodes(
        pendingDelete.map((n) => {
          return { id: String(n.data.id), type: String(n.data.componentType) };
        }),
      );
    }

    setPendingDelete(null);
  }, [pendingDelete, onDeleteNodes]);

  // Keyboard shortcut for delete
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Delete" || e.key === "Backspace") {
        // Don't intercept if user is typing in an input
        if (
          e.target instanceof HTMLInputElement ||
          e.target instanceof HTMLTextAreaElement
        ) return;
        deleteSelectedNodes();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [deleteSelectedNodes]);

  return (
    <CanvasLockedContext.Provider value={canvasLocked}>
    <div className="w-full h-full" style={{ minHeight: "100%" }}>
      <svg width="0" height="0">
        <defs>
          <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="3" result="blur" />
            <feComposite in="SourceGraphic" in2="blur" operator="over" />
          </filter>
        </defs>
      </svg>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onPaneClick={onPaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        nodesDraggable={!canvasLocked}
        nodesConnectable={false}
        multiSelectionKeyCode="Shift"
        selectionKeyCode="Shift"
        deleteKeyCode={null}
        fitView
        minZoom={0.05}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <AutoLayout onApiRef={canvasApiRef} instanceId={instanceId} />
        <Background color="var(--drasi-border)" gap={24} size={1} />
        <Controls showInteractive={false} className="!bg-drasi-card !border-drasi-border !rounded-lg [&>button]:!bg-drasi-card [&>button]:!border-drasi-border [&>button]:!text-drasi-text-secondary [&>button:hover]:!bg-drasi-surface [&>button]:!w-8 [&>button]:!h-8">
          <ControlButton
            onClick={autoLayoutNodes}
            title="Auto-layout nodes"
          >
            <LayoutGrid size={18} />
          </ControlButton>
          <ControlButton
            onClick={toggleCanvasLock}
            title={canvasLocked ? "Unlock canvas" : "Lock canvas"}
            className={canvasLocked ? "!text-drasi-warning !bg-drasi-warning/20" : ""}
          >
            {canvasLocked ? <Lock size={18} /> : <LockOpen size={18} />}
          </ControlButton>
        </Controls>

        {/* Canvas toolbar — visible when nodes are selected */}
        {!canvasLocked && nodes.some((n) => n.selected) && (
          <Panel position="top-right" className="flex gap-1.5">
            <button
              onClick={toggleSelectedNodesLock}
              className="p-2 rounded-lg border bg-drasi-card border-drasi-border text-drasi-text-secondary hover:text-drasi-warning hover:bg-drasi-warning/10 transition-colors"
              title={
                nodes.filter((n) => n.selected).some((n) => !n.data?.locked)
                  ? "Pin selected nodes"
                  : "Unpin selected nodes"
              }
            >
              {nodes.filter((n) => n.selected).some((n) => !n.data?.locked) ? (
                <Pin size={16} className="-rotate-45" />
              ) : (
                <Pin size={16} />
              )}
            </button>
            {onDeleteNodes && <button
              onClick={deleteSelectedNodes}
              className="p-2 rounded-lg border bg-drasi-card border-drasi-border text-drasi-error/70 hover:text-drasi-error hover:bg-drasi-error/10 transition-colors"
              title="Delete selected nodes"
            >
              <Trash2 size={16} />
            </button>}
          </Panel>
        )}

        <MiniMap
          nodeColor={(node) => {
            const component = data.computation?.components.find((c) => c.id === node.data.id);
            return computationColor(component?.role ?? "");
          }}
          maskColor="var(--drasi-minimap-mask)"
          className="!bg-drasi-surface !border-drasi-border !rounded-lg"
        />
      </ReactFlow>

      {/* Delete confirmation dialog */}
      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-drasi-card border border-drasi-border rounded-xl p-5 max-w-sm space-y-4">
            <h3 className="text-sm font-semibold text-drasi-text-primary">
              Delete {pendingDelete.length} component{pendingDelete.length > 1 ? "s" : ""}?
            </h3>
            <p className="text-xs text-drasi-text-secondary">
              This will remove the selected component{pendingDelete.length > 1 ? "s" : ""} from the server. This action cannot be undone.
            </p>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => setPendingDelete(null)}
                className="action-btn-ghost text-xs"
              >
                Cancel
              </button>
              <button
                onClick={confirmDelete}
                className="action-btn-danger text-xs"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
    </CanvasLockedContext.Provider>
  );
}
