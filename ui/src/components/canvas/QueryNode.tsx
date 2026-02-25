import { type NodeProps } from "@xyflow/react";
import { Search, Loader2, Radio, WifiOff } from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import NodeShell from "./NodeShell";
import type { ComponentStatus } from "@/utils/colors";
import { useQueryResults } from "@/hooks/useQueryResults";

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
  instanceId?: string;
  [key: string]: unknown;
}

export default function QueryNode({ data, id: nodeId }: NodeProps) {
  const d = data as unknown as QueryNodeData;
  const expanded = !!d.expanded;

  // Only fetch/stream results when expanded and query is running
  const shouldFetchResults = expanded && d.status === "Running";
  const { results, loading, error: resultsError, streaming } = useQueryResults(
    shouldFetchResults ? d.id : null,
    d.instanceId,
  );

  // Format a result row for display (truncate long values)
  const formatValue = (val: unknown): string => {
    if (val === null || val === undefined) return "null";
    if (typeof val === "object") {
      const str = JSON.stringify(val);
      return str.length > 50 ? str.slice(0, 47) + "..." : str;
    }
    const str = String(val);
    return str.length > 50 ? str.slice(0, 47) + "..." : str;
  };

  // Get column headers from first result
  const columns = results.length > 0 ? Object.keys(results[0]) : [];

  return (
    <NodeShell
      nodeId={nodeId}
      cardClass="node-card-query"
      accentClass="text-drasi-query"
      collapsedWidth={180}
      expandedWidth={420}
      collapsedMinHeight={85}
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

          {/* Query Results Section */}
          {d.status === "Running" && (
            <div className="space-y-1 mt-2 pt-2 border-t border-drasi-border">
              <div className="flex items-center gap-2">
                <div className="text-[9px] uppercase tracking-wider text-drasi-text-secondary font-semibold">
                  Results
                </div>
                {loading && (
                  <Loader2 size={10} className="animate-spin text-drasi-query" />
                )}
                {!loading && streaming && (
                  <span title="Streaming updates">
                    <Radio size={10} className="text-drasi-success animate-pulse" />
                  </span>
                )}
                {!loading && !streaming && results.length > 0 && (
                  <span title="Not streaming">
                    <WifiOff size={10} className="text-drasi-text-secondary" />
                  </span>
                )}
                <span className="text-[9px] text-drasi-text-secondary ml-auto">
                  {results.length} row{results.length !== 1 ? "s" : ""}
                </span>
              </div>

              {resultsError && (
                <div className="text-[10px] text-drasi-error bg-drasi-error/10 rounded p-1.5">
                  {resultsError}
                </div>
              )}

              {!resultsError && results.length > 0 && (
                <div className="nowheel overflow-auto max-h-32 rounded border border-drasi-border bg-drasi-bg">
                  <table className="w-full text-[9px] font-mono">
                    <thead className="bg-drasi-surface sticky top-0">
                      <tr>
                        {columns.map((col) => (
                          <th
                            key={col}
                            className="px-1.5 py-1 text-left text-drasi-text-secondary font-semibold border-b border-drasi-border whitespace-nowrap"
                          >
                            {col}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {results.slice(0, 50).map((row, i) => (
                        <tr key={i} className="hover:bg-drasi-surface/50">
                          {columns.map((col) => (
                            <td
                              key={col}
                              className="px-1.5 py-0.5 text-drasi-text-primary border-b border-drasi-border/50 whitespace-nowrap"
                              title={typeof row[col] === "object" ? JSON.stringify(row[col], null, 2) : String(row[col])}
                            >
                              {formatValue(row[col])}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {results.length > 50 && (
                    <div className="text-center py-1 text-[9px] text-drasi-text-secondary bg-drasi-surface border-t border-drasi-border">
                      +{results.length - 50} more rows
                    </div>
                  )}
                </div>
              )}

              {!resultsError && !loading && results.length === 0 && (
                <div className="text-[10px] text-drasi-text-secondary italic text-center py-2">
                  No results
                </div>
              )}
            </div>
          )}

          {/* Show message when query is not running */}
          {d.status !== "Running" && (
            <div className="mt-2 pt-2 border-t border-drasi-border">
              <div className="text-[10px] text-drasi-text-secondary italic text-center py-2">
                Start query to see results
              </div>
            </div>
          )}
        </div>
      }
    >
      <div className="flex items-center justify-between">
        <StatusBadge status={d.status as ComponentStatus} error={d.error} />
        {d.status === "Running" && expanded && results.length > 0 && (
          <span className="text-[10px] font-mono text-drasi-text-secondary">
            {results.length} rows
          </span>
        )}
        {d.resultCount !== undefined && !expanded && (
          <span className="text-[10px] font-mono text-drasi-text-secondary">
            {d.resultCount} rows
          </span>
        )}
      </div>
    </NodeShell>
  );
}
