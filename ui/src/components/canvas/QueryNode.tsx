import { type NodeProps } from "@xyflow/react";
import { Search } from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import NodeShell from "./NodeShell";
import type { ComponentStatus } from "@/utils/colors";

interface QueryNodeData {
  id: string;
  status: string;
  resultCount?: number;
  query?: string;
  queryLanguage?: string;
  sourceIds?: string[];
  expanded?: boolean;
  locked?: boolean;
  canvasLocked?: boolean;
  error?: string;
  [key: string]: unknown;
}

export default function QueryNode({ data, id: nodeId }: NodeProps) {
  const d = data as unknown as QueryNodeData;
  const expanded = !!d.expanded;

  return (
    <NodeShell
      nodeId={nodeId}
      cardClass="node-card-query"
      accentClass="text-drasi-query"
      collapsedWidth={180}
      expandedWidth={360}
      status={d.status as ComponentStatus}
      expanded={expanded}
      locked={!!d.locked}
      canvasLocked={!!d.canvasLocked}
      toggleTitle={expanded ? "Collapse" : "View query"}
      handles="both"
      handleClass="!bg-drasi-query"
      header={
        <>
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
        </>
      }
      expandContent={
        <div className="mt-3 pt-3 border-t border-drasi-border space-y-2">
          {/* Language badge */}
          <div className="flex items-center gap-2">
            <span
              className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider
                         bg-drasi-query/20 text-drasi-query border border-drasi-query/30"
            >
              {d.queryLanguage ?? "Cypher"}
            </span>
            {d.sourceIds && d.sourceIds.length > 0 && (
              <span className="text-[10px] text-drasi-text-secondary">
                {d.sourceIds.length} source
                {d.sourceIds.length > 1 ? "s" : ""}
              </span>
            )}
          </div>

          {/* Query text */}
          {d.query && (
            <pre
              className="nowheel bg-drasi-bg rounded-lg p-2 text-[10px] font-mono text-drasi-text-primary
                         overflow-auto max-h-24 border border-drasi-border whitespace-pre-wrap break-words"
            >
              {d.query}
            </pre>
          )}

          {/* Source list */}
          {d.sourceIds && d.sourceIds.length > 0 && (
            <div className="space-y-1">
              <div className="text-[9px] uppercase tracking-wider text-drasi-text-secondary font-semibold">
                Sources
              </div>
              {d.sourceIds.map((sid) => (
                <div
                  key={sid}
                  className="text-[10px] text-drasi-source font-mono pl-2 border-l-2 border-drasi-source/30"
                >
                  {sid}
                </div>
              ))}
            </div>
          )}
        </div>
      }
    >
      <div className="flex items-center justify-between">
        <StatusBadge status={d.status as ComponentStatus} error={d.error} />
        {d.resultCount !== undefined && (
          <span className="text-[10px] font-mono text-drasi-text-secondary">
            {d.resultCount} rows
          </span>
        )}
      </div>
    </NodeShell>
  );
}
