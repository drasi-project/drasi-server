import { Handle, Position, type NodeProps } from "@xyflow/react";
import { Search } from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import type { ComponentStatus } from "@/utils/colors";
import { getStatusGlowClass } from "@/utils/colors";

interface QueryNodeData {
  id: string;
  status: string;
  resultCount?: number;
  [key: string]: unknown;
}

export default function QueryNode({ data }: NodeProps) {
  const d = data as unknown as QueryNodeData;
  const glowClass = getStatusGlowClass(d.status as ComponentStatus);

  return (
    <div className={`node-card-query ${glowClass} min-w-[180px]`}>
      <div className="flex items-center gap-2 mb-2">
        <div className="p-1.5 rounded-lg bg-drasi-query/20">
          <Search size={16} className="text-drasi-query" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-xs font-semibold text-drasi-text-primary truncate">
            {d.id}
          </div>
          <div className="text-[10px] text-drasi-text-secondary">
            CONTINUOUS QUERY
          </div>
        </div>
      </div>
      <div className="flex items-center justify-between">
        <StatusBadge status={d.status as ComponentStatus} />
        {d.resultCount !== undefined && (
          <span className="text-[10px] font-mono text-drasi-text-secondary">
            {d.resultCount} rows
          </span>
        )}
      </div>
      <Handle
        type="target"
        position={Position.Left}
        className="!bg-drasi-query !border-drasi-card !w-3 !h-3"
      />
      <Handle
        type="source"
        position={Position.Right}
        className="!bg-drasi-query !border-drasi-card !w-3 !h-3"
      />
    </div>
  );
}
