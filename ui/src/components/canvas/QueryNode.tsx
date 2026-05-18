import { memo, useMemo } from "react";
import { type NodeProps } from "@xyflow/react";
import { Search, Loader2, Radio, WifiOff } from "lucide-react";
import NodeShell from "./NodeShell";
import type { ComponentStatus } from "@/utils/colors";
import { useQueryResults } from "@/hooks/useQueryResults";
import { useApi } from "@/hooks/useApi";

interface QueryNodeData {
  id: string;
  status: string;
  resultCount?: number;
  query?: string;
  queryLanguage?: string;
  sourceIds?: string[];
  expanded?: boolean;
  locked?: boolean;
  error?: string;
  instanceId?: string;
  [key: string]: unknown;
}

function formatValue(val: unknown): string {
  if (val === null || val === undefined) return "null";
  if (typeof val === "object") {
    const str = JSON.stringify(val);
    return str.length > 50 ? str.slice(0, 47) + "..." : str;
  }
  const str = String(val);
  return str.length > 50 ? str.slice(0, 47) + "..." : str;
}

const MAX_DISPLAY_ROWS = 50;

export default memo(function QueryNode({ data, id: nodeId }: NodeProps) {
  const d = data as unknown as QueryNodeData;
  const expanded = !!d.expanded;
  
  // Format query language label
  const languageLabel = d.queryLanguage?.toLowerCase() === "gql" 
    ? "GQL Query" 
    : "Cypher Query";
  const { startQuery, stopQuery } = useApi();

  // Only fetch/stream results when expanded and query is running
  const shouldFetchResults = expanded && d.status === "Running";
  const { results, loading, error: resultsError, streaming } = useQueryResults(
    shouldFetchResults ? d.id : null,
    d.instanceId,
  );

  const columns = useMemo(
    () => (results.length > 0 ? Object.keys(results[0]) : []),
    [results],
  );
  const displayRows = useMemo(
    () => results.slice(0, MAX_DISPLAY_ROWS),
    [results],
  );

  const handleStartStop = () => {
    if (d.status === "Running") {
      stopQuery(d.id, d.instanceId);
    } else if (d.status === "Stopped" || d.status === "Error") {
      startQuery(d.id, d.instanceId);
    }
  };

  return (
    <NodeShell
      nodeId={nodeId}
      cardClass="node-card-query"
      accentClass="text-drasi-query"
      collapsedWidth={180}
      expandedWidth={420}
      collapsedHeight={92}
      expandedHeight={280}
      status={d.status as ComponentStatus}
      expanded={expanded}
      locked={!!d.locked}
      toggleTitle={expanded ? "Collapse" : "View results"}
      handles="both"
      handleClass="!bg-drasi-query"
      onStartStop={handleStartStop}
      header={
        <>
          <div className="p-1.5 rounded-lg bg-drasi-query/20">
            <Search size={16} className="text-drasi-query" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-xs font-semibold text-drasi-text-primary truncate">
              {d.id}
            </div>
            <div className="text-[10px] text-drasi-text-secondary uppercase tracking-wider">
              {languageLabel}
            </div>
          </div>
        </>
      }
      expandContent={
        <div className="mt-3 pt-3 border-t border-drasi-border">
          {/* Query Results - Runtime only */}
          {d.status === "Running" ? (
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <div className="text-[9px] uppercase tracking-wider text-drasi-text-secondary font-semibold">
                  Live Results
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
                <div className="nowheel overflow-auto max-h-40 rounded border border-drasi-border bg-drasi-bg">
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
                      {displayRows.map((row, i) => (
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
                  {results.length > MAX_DISPLAY_ROWS && (
                    <div className="text-center py-1 text-[9px] text-drasi-text-secondary bg-drasi-surface border-t border-drasi-border">
                      +{results.length - MAX_DISPLAY_ROWS} more rows
                    </div>
                  )}
                </div>
              )}

              {!resultsError && !loading && results.length === 0 && (
                <div className="text-[10px] text-drasi-text-secondary italic text-center py-4">
                  No results yet
                </div>
              )}
            </div>
          ) : (
            <div className="text-[10px] text-drasi-text-secondary italic text-center py-4">
              Start query to see live results
            </div>
          )}
        </div>
      }
    />
  );
})
