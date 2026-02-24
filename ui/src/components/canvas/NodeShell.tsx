import {
  Handle,
  Position,
  useReactFlow,
  useUpdateNodeInternals,
} from "@xyflow/react";
import { Maximize2, Minimize2, Lock, Unlock } from "lucide-react";
import { motion } from "framer-motion";
import { useCallback, type ReactNode } from "react";
import { getStatusGlowClass } from "@/utils/colors";
import type { ComponentStatus } from "@/utils/colors";

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
  /** Whether the entire canvas is locked */
  canvasLocked?: boolean;
}

export default function NodeShell({
  nodeId,
  cardClass,
  accentClass,
  collapsedWidth,
  expandedWidth,
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
  canvasLocked = false,
}: NodeShellProps) {
  const glowClass = getStatusGlowClass(status);
  const isLocked = locked || canvasLocked;
  const { setNodes } = useReactFlow();
  const updateNodeInternals = useUpdateNodeInternals();

  const handleToggle = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (isLocked) return;
      setNodes((nodes) =>
        nodes.map((n) =>
          n.id === nodeId
            ? { ...n, data: { ...n.data, expanded: !n.data.expanded } }
            : n,
        ),
      );
      setTimeout(() => updateNodeInternals(nodeId), 400);
    },
    [nodeId, setNodes, updateNodeInternals, isLocked],
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

  return (
    <motion.div
      className={`${cardClass} ${glowClass}`}
      animate={{ width: expanded ? expandedWidth : collapsedWidth }}
      transition={{ type: "tween", duration: 0.4, ease: "easeInOut" }}
    >
      <div className="flex items-center gap-2 mb-2">
        {header}
        {!canvasLocked && (
          <motion.button
            onClick={handleLockToggle}
            className="nodrag p-1 rounded-md transition-colors hover:bg-white/10"
            whileHover={{ scale: 1.2 }}
            whileTap={{ scale: 0.9 }}
            title={locked ? "Unlock node" : "Lock node"}
          >
            {locked ? (
              <Lock size={10} className="text-drasi-warning" />
            ) : (
              <Unlock size={10} className="text-drasi-text-secondary/40" />
            )}
          </motion.button>
        )}
        {canvasLocked && locked && (
          <Lock size={10} className="text-drasi-warning/60 shrink-0" />
        )}
        {canToggle && !isLocked && (
          <motion.button
            onClick={handleToggle}
            className="nodrag p-1 rounded-md transition-colors hover:bg-white/10"
            whileHover={{ scale: 1.2 }}
            whileTap={{ scale: 0.9 }}
            title={toggleTitle ?? (expanded ? "Collapse" : "Expand")}
          >
            {expanded ? (
              <Minimize2 size={12} className={accentClass} />
            ) : (
              <Maximize2 size={12} className={accentClass} />
            )}
          </motion.button>
        )}
      </div>

      {children}

      {expandContent && (
        <div
          className="grid transition-[grid-template-rows,opacity] duration-400 ease-in-out"
          style={{
            gridTemplateRows: expanded ? "1fr" : "0fr",
            opacity: expanded ? 1 : 0,
          }}
        >
          <div className="overflow-hidden">{expandContent}</div>
        </div>
      )}

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
