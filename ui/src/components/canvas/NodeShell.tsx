import {
  Handle,
  Position,
  useReactFlow,
  useUpdateNodeInternals,
} from "@xyflow/react";
import { Maximize2, Minimize2, Pin, Play, Square } from "lucide-react";
import { motion } from "framer-motion";
import { useCallback, useRef, type ReactNode } from "react";
import { getStatusGlowClass } from "@/utils/colors";
import type { ComponentStatus } from "@/utils/colors";
import { useCanvasLocked } from "./CanvasLockedContext";

export interface NodeShellProps {
  /** React Flow node id */
  nodeId: string;
  /** CSS class for the card (e.g. node-card-source) */
  cardClass: string;
  /** Color class applied to the expand/collapse icon */
  accentClass: string;
  /** Width in px when collapsed */
  collapsedWidth: number;
  /** Width in px when expanded */
  expandedWidth: number;
  /** Minimum height in px when collapsed (ensures uniform sizing) */
  collapsedMinHeight?: number;
  /** Component status for glow styling */
  status: ComponentStatus;
  /** Whether the node is currently expanded */
  expanded: boolean;
  /** Whether the expand/collapse toggle should be shown */
  canToggle?: boolean;
  /** Toggle button tooltip */
  toggleTitle?: string;
  /** Handles to render — "source" (right), "target" (left), or "both" */
  handles: "source" | "target" | "both";
  /** Handle color class */
  handleClass: string;
  /** Content inside the header flex row (icon + title) */
  header: ReactNode;
  /** Content below the header, above expand section (StatusBadge, etc.) */
  children?: ReactNode;
  /** Content shown only when expanded (inside the CSS grid section) */
  expandContent?: ReactNode;
  /** Whether this node is individually locked */
  locked?: boolean;
  /** Callback when start/stop is clicked */
  onStartStop?: () => void;
}

export default function NodeShell({
  nodeId,
  cardClass,
  accentClass,
  collapsedWidth,
  expandedWidth,
  collapsedMinHeight,
  status,
  expanded,
  canToggle = true,
  toggleTitle,
  handles,
  handleClass,
  header,
  children,
  expandContent,
  locked = false,
  onStartStop,
}: NodeShellProps) {
  const canvasLocked = useCanvasLocked();
  const glowClass = getStatusGlowClass(status);
  const isLocked = locked || canvasLocked;
  const isRunning = status === "Running";
  const isTransitioning = status === "Starting" || status === "Stopping" || status === "Reconfiguring";
  const { setNodes } = useReactFlow();
  const updateNodeInternals = useUpdateNodeInternals();
  const expandContentRef = useRef<HTMLDivElement>(null);

  const handleToggle = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (isLocked) return;
      // Measure the expand content's natural height before toggling so we
      // can apply vertical displacement in the SAME state update — ensuring
      // both the expansion and neighbour movement start in the same paint frame.
      const expandContentHeight = expandContentRef.current?.scrollHeight ?? 0;
      setNodes((nodes) => {
        const self = nodes.find((n) => n.id === nodeId);
        if (!self) return nodes;

        const willExpand = !self.data?.expanded;
        const deltaH = willExpand ? expandContentHeight : -expandContentHeight;
        // Pre-toggle width determines the right-edge boundary
        const preWidth = willExpand ? collapsedWidth : expandedWidth;
        const rightEdge = self.position.x + preWidth;

        return nodes.map((n) => {
          if (n.id === nodeId) {
            return {
              ...n,
              data: {
                ...n.data,
                expanded: willExpand,
                expandContentHeight,
                heightShiftApplied: true,
              },
            };
          }

          // Apply vertical displacement to nodes below & within horizontal span
          const a = n.position.x;
          const b = n.position.y;
          if (Math.abs(deltaH) > 1 && a < rightEdge && b >= self.position.y) {
            return {
              ...n,
              position: { ...n.position, y: n.position.y + deltaH },
            };
          }

          return n;
        });
      });
      setTimeout(() => updateNodeInternals(nodeId), 405);
    },
    [nodeId, setNodes, updateNodeInternals, isLocked, collapsedWidth, expandedWidth],
  );

  const handleLockToggle = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (canvasLocked) return;
      setNodes((nodes) =>
        nodes.map((n) =>
          n.id === nodeId
            ? {
                ...n,
                draggable: n.data?.locked ? true : false,
                data: { ...n.data, locked: !n.data?.locked },
              }
            : n,
        ),
      );
    },
    [nodeId, setNodes, canvasLocked],
  );

  const targetWidth = expanded ? expandedWidth : collapsedWidth;
  const minHeight = !expanded && collapsedMinHeight ? collapsedMinHeight : undefined;

  const handleStartStop = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      onStartStop?.();
    },
    [onStartStop],
  );

  // Toolbar buttons component to avoid duplication
  const toolbarButtons = (
    <>
      {/* Start/Stop button */}
      {onStartStop && (
        <motion.button
          onClick={handleStartStop}
          disabled={isTransitioning}
          className={`nodrag p-1.5 rounded-md transition-colors ${
            isTransitioning
              ? "opacity-50 cursor-not-allowed"
              : isRunning
                ? "hover:bg-drasi-error/10 text-drasi-error/70 hover:text-drasi-error"
                : "hover:bg-drasi-running/10 text-drasi-running/70 hover:text-drasi-running"
          }`}
          whileHover={isTransitioning ? {} : { scale: 1.1 }}
          whileTap={isTransitioning ? {} : { scale: 0.9 }}
          title={isRunning ? "Stop" : "Start"}
        >
          {isRunning ? (
            <Square size={14} fill="currentColor" />
          ) : (
            <Play size={14} fill="currentColor" />
          )}
        </motion.button>
      )}

      {/* Expand button - always visible, disabled when not expandable or canvas locked */}
      <motion.button
        onClick={canToggle && !isLocked ? handleToggle : undefined}
        disabled={!canToggle || isLocked}
        className={`nodrag p-1.5 rounded-md transition-colors ${
          canToggle && !isLocked
            ? "hover:bg-drasi-text-secondary/10"
            : "opacity-30 cursor-not-allowed"
        }`}
        whileHover={canToggle && !isLocked ? { scale: 1.1 } : {}}
        whileTap={canToggle && !isLocked ? { scale: 0.9 } : {}}
        title={canvasLocked ? "Canvas is locked" : (!canToggle ? "Cannot expand" : (toggleTitle ?? (expanded ? "Collapse" : "Expand")))}
      >
        {expanded ? (
          <Minimize2 size={14} className={canToggle && !isLocked ? accentClass : "text-drasi-text-secondary"} />
        ) : (
          <Maximize2 size={14} className={canToggle && !isLocked ? accentClass : "text-drasi-text-secondary"} />
        )}
      </motion.button>

      {/* Pin button - always visible, disabled when canvas is locked */}
      <motion.button
        onClick={!canvasLocked ? handleLockToggle : undefined}
        disabled={canvasLocked}
        className={`nodrag p-1.5 rounded-md transition-colors ${
          canvasLocked
            ? "opacity-30 cursor-not-allowed"
            : "hover:bg-drasi-text-secondary/10"
        }`}
        whileHover={!canvasLocked ? { scale: 1.1 } : {}}
        whileTap={!canvasLocked ? { scale: 0.9 } : {}}
        title={canvasLocked ? "Canvas is locked" : (locked ? "Unpin node" : "Pin node")}
      >
        <Pin 
          size={14} 
          className={locked ? "text-drasi-warning" : "text-drasi-text-secondary/50 -rotate-45"} 
        />
      </motion.button>
    </>
  );

  return (
    <motion.div
      className={`${cardClass} ${glowClass} relative`}
      style={{ minHeight }}
      initial={{ width: targetWidth }}
      animate={{ width: targetWidth }}
      transition={{ type: "tween", duration: 0.4, ease: "easeInOut" }}
    >
      {/* Top-right toolbar (shown when expanded) */}
      <div 
        className="absolute top-1.5 right-2 flex items-center justify-end gap-1 transition-opacity duration-300"
        style={{ opacity: expanded ? 1 : 0, pointerEvents: expanded ? "auto" : "none" }}
      >
        {toolbarButtons}
      </div>

      {/* Header - just icon and title */}
      <div className="flex items-center gap-2 mb-1">
        {header}
      </div>

      {children}

      {expandContent && (
        <div
          className="grid transition-[grid-template-rows,opacity] duration-[405ms]"
          style={{
            gridTemplateRows: expanded ? "1fr" : "0fr",
            opacity: expanded ? 1 : 0,
            transitionTimingFunction: "ease-in-out",
          }}
        >
          <div ref={expandContentRef} className="overflow-hidden">{expandContent}</div>
        </div>
      )}

      {/* Bottom toolbar (shown when collapsed) */}
      <div 
        className="flex items-center justify-end gap-1 mt-2 pt-2 border-t border-drasi-border/50 transition-opacity duration-300"
        style={{ opacity: expanded ? 0 : 1, pointerEvents: expanded ? "none" : "auto" }}
      >
        {toolbarButtons}
      </div>

      {(handles === "target" || handles === "both") && (
        <Handle
          type="target"
          position={Position.Left}
          className={`!border-drasi-card !w-3 !h-3 ${handleClass}`}
        />
      )}
      {(handles === "source" || handles === "both") && (
        <Handle
          type="source"
          position={Position.Right}
          className={`!border-drasi-card !w-3 !h-3 ${handleClass}`}
        />
      )}
    </motion.div>
  );
}
