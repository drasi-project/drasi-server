import { type NodeProps } from "@xyflow/react";
import {
  Database,
  Globe,
  Radio,
  FlaskConical,
  Server,
} from "lucide-react";
import SourcePushPanel from "./SourcePushPanel";
import NodeShell from "./NodeShell";
import type { ComponentStatus } from "@/utils/colors";
import { useApi } from "@/hooks/useApi";

const ICON_MAP: Record<string, React.ElementType> = {
  postgres: Database,
  http: Globe,
  grpc: Radio,
  mock: FlaskConical,
  platform: Server,
};

const PUSHABLE_KINDS = new Set(["http", "grpc"]);

interface SourceNodeData {
  id: string;
  kind: string;
  status: string;
  expanded?: boolean;
  locked?: boolean;
  canvasLocked?: boolean;
  properties?: Record<string, unknown>;
  instanceId?: string;
  error?: string;
  [key: string]: unknown;
}

export default function SourceNode({ data, id: nodeId }: NodeProps) {
  const d = data as unknown as SourceNodeData;
  const Icon = ICON_MAP[d.kind] || Database;
  const canPush = PUSHABLE_KINDS.has(d.kind) && d.status === "Running";
  const expanded = !!d.expanded;
  const { startSource, stopSource } = useApi();

  const handleStartStop = () => {
    if (d.status === "Running") {
      stopSource(d.id, d.instanceId);
    } else if (d.status === "Stopped" || d.status === "Error") {
      startSource(d.id, d.instanceId);
    }
  };

  return (
    <NodeShell
      nodeId={nodeId}
      cardClass="node-card-source"
      accentClass="text-drasi-source"
      collapsedWidth={180}
      expandedWidth={320}
      collapsedMinHeight={72}
      status={d.status as ComponentStatus}
      expanded={expanded}
      canToggle={canPush}
      toggleTitle={expanded ? "Collapse" : "Push data"}
      locked={!!d.locked}
      canvasLocked={!!d.canvasLocked}
      handles="source"
      handleClass="!bg-drasi-source"
      onStartStop={handleStartStop}
      header={
        <>
          <div className="p-1.5 rounded-lg bg-drasi-source/20">
            <Icon size={16} className="text-drasi-source" />
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
        d.properties ? (
          <SourcePushPanel
            sourceId={d.id}
            instanceId={d.instanceId}
            host={String(d.properties?.host ?? "localhost")}
            port={Number(d.properties?.port ?? 8081)}
            endpoint={d.properties?.endpoint as string | undefined}
          />
        ) : undefined
      }
    />
  );
}
