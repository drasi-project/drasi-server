import { type NodeProps } from "@xyflow/react";
import {
  Zap,
  Globe,
  Radio,
  FileText,
  Rss,
  Server,
  Gauge,
  Activity,
} from "lucide-react";
import NodeShell from "./NodeShell";
import type { ComponentStatus } from "@/utils/colors";
import { useApi } from "@/hooks/useApi";

const ICON_MAP: Record<string, React.ElementType> = {
  log: FileText,
  http: Globe,
  "http-adaptive": Globe,
  grpc: Radio,
  "grpc-adaptive": Radio,
  sse: Rss,
  platform: Server,
  profiler: Gauge,
};

interface ReactionNodeData {
  id: string;
  kind: string;
  status: string;
  expanded?: boolean;
  locked?: boolean;
  canvasLocked?: boolean;
  queryIds?: string[];
  properties?: Record<string, unknown>;
  error?: string;
  instanceId?: string;
  [key: string]: unknown;
}

export default function ReactionNode({ data, id: nodeId }: NodeProps) {
  const d = data as unknown as ReactionNodeData;
  const Icon = ICON_MAP[d.kind] || Zap;
  const expanded = !!d.expanded;
  const { startReaction, stopReaction } = useApi();

  const handleStartStop = () => {
    if (d.status === "Running") {
      stopReaction(d.id, d.instanceId);
    } else if (d.status === "Stopped" || d.status === "Error") {
      startReaction(d.id, d.instanceId);
    }
  };

  return (
    <NodeShell
      nodeId={nodeId}
      cardClass="node-card-reaction"
      accentClass="text-drasi-reaction"
      collapsedWidth={180}
      expandedWidth={300}
      collapsedMinHeight={72}
      status={d.status as ComponentStatus}
      expanded={expanded}
      canToggle={false}
      toggleTitle={expanded ? "Collapse" : "View activity"}
      locked={!!d.locked}
      canvasLocked={!!d.canvasLocked}
      handles="target"
      handleClass="!bg-drasi-reaction"
      onStartStop={handleStartStop}
      header={
        <>
          <div className="p-1.5 rounded-lg bg-drasi-reaction/20">
            <Icon size={16} className="text-drasi-reaction" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-xs font-semibold text-drasi-text-primary truncate">
              {d.id}
            </div>
            <div className="text-[10px] text-drasi-text-secondary uppercase tracking-wider">
              {d.kind}
            </div>
          </div>
        </>
      }
      expandContent={
        <div className="mt-3 pt-3 border-t border-drasi-border">
          {/* Runtime activity - placeholder for future event stream */}
          {d.status === "Running" ? (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <div className="text-[9px] uppercase tracking-wider text-drasi-text-secondary font-semibold">
                  Activity
                </div>
                <Activity size={10} className="text-drasi-reaction animate-pulse" />
              </div>
              <div className="text-[10px] text-drasi-text-secondary italic text-center py-4 bg-drasi-bg rounded border border-drasi-border">
                Event stream coming soon
              </div>
            </div>
          ) : (
            <div className="text-[10px] text-drasi-text-secondary italic text-center py-4">
              Start reaction to see activity
            </div>
          )}
        </div>
      }
    />
  );
}
