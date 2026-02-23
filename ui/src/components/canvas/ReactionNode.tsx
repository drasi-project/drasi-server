import { Handle, Position, type NodeProps } from "@xyflow/react";
import {
  Zap,
  Globe,
  Radio,
  FileText,
  Rss,
  Server,
  Gauge,
} from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import type { ComponentStatus } from "@/utils/colors";
import { getStatusGlowClass } from "@/utils/colors";

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
  [key: string]: unknown;
}

export default function ReactionNode({ data }: NodeProps) {
  const d = data as unknown as ReactionNodeData;
  const Icon = ICON_MAP[d.kind] || Zap;
  const glowClass = getStatusGlowClass(d.status as ComponentStatus);

  return (
    <div className={`node-card-reaction ${glowClass} min-w-[180px]`}>
      <div className="flex items-center gap-2 mb-2">
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
      </div>
      <StatusBadge status={d.status as ComponentStatus} />
      <Handle
        type="target"
        position={Position.Left}
        className="!bg-drasi-reaction !border-drasi-card !w-3 !h-3"
      />
    </div>
  );
}
