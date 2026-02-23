import { Handle, Position, type NodeProps } from "@xyflow/react";
import { Database, Globe, Radio, FlaskConical, Server } from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import type { ComponentStatus } from "@/utils/colors";
import { getStatusGlowClass } from "@/utils/colors";

const ICON_MAP: Record<string, React.ElementType> = {
  postgres: Database,
  http: Globe,
  grpc: Radio,
  mock: FlaskConical,
  platform: Server,
};

interface SourceNodeData {
  id: string;
  kind: string;
  status: string;
  [key: string]: unknown;
}

export default function SourceNode({ data }: NodeProps) {
  const d = data as unknown as SourceNodeData;
  const Icon = ICON_MAP[d.kind] || Database;
  const glowClass = getStatusGlowClass(d.status as ComponentStatus);

  return (
    <div className={`node-card-source ${glowClass} min-w-[180px]`}>
      <div className="flex items-center gap-2 mb-2">
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
      </div>
      <StatusBadge status={d.status as ComponentStatus} />
      <Handle
        type="source"
        position={Position.Right}
        className="!bg-drasi-source !border-drasi-card !w-3 !h-3"
      />
    </div>
  );
}
