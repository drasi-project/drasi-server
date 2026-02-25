import { type NodeProps } from "@xyflow/react";
import {
  Zap,
  Globe,
  Radio,
  FileText,
  Rss,
  Server,
  Gauge,
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
      toggleTitle={expanded ? "Collapse" : "View details"}
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
        <div className="mt-3 pt-3 border-t border-drasi-border space-y-2">
          {/* Kind badge */}
          <span
            className="inline-block px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider
                       bg-drasi-reaction/20 text-drasi-reaction border border-drasi-reaction/30"
          >
            {d.kind}
          </span>

          {/* Connected queries */}
          {d.queryIds && d.queryIds.length > 0 && (
            <div className="space-y-1">
              <div className="text-[9px] uppercase tracking-wider text-drasi-text-secondary font-semibold">
                Listening to
              </div>
              {d.queryIds.map((qId) => (
                <div
                  key={qId}
                  className="text-[10px] text-drasi-query font-mono pl-2 border-l-2 border-drasi-query/30"
                >
                  {qId}
                </div>
              ))}
            </div>
          )}

          {/* Properties */}
          {d.properties && Object.keys(d.properties).length > 0 && (
            <div className="space-y-1">
              <div className="text-[9px] uppercase tracking-wider text-drasi-text-secondary font-semibold">
                Config
              </div>
              {Object.entries(d.properties).map(([key, value]) => (
                <div
                  key={key}
                  className="flex justify-between text-[10px] gap-2"
                >
                  <span className="text-drasi-text-secondary truncate">
                    {key}
                  </span>
                  <span className="text-drasi-text-primary font-mono truncate">
                    {String(value)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      }
    />
  );
}
