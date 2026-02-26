import { X, AlertCircle, Database, Zap, Search, Code, GitBranch } from "lucide-react";
import { motion } from "framer-motion";
import StatusBadge from "@/components/shared/StatusBadge";
import ActionButtons from "@/components/shared/ActionButtons";
import type { ComponentStatus, ComponentType } from "@/utils/colors";
import { getTypeColor } from "@/utils/colors";

interface ConnectedComponent {
  id: string;
  type: ComponentType;
  status: ComponentStatus;
  kind?: string;
}

interface QueryInspectorPanelProps {
  id: string;
  status: ComponentStatus;
  error?: string;
  query: string;
  queryLanguage: string;
  sources: ConnectedComponent[];
  reactions: ConnectedComponent[];
  onClose: () => void;
  onStart?: () => void;
  onStop?: () => void;
  onDelete?: () => void;
}

export default function QueryInspectorPanel({
  id,
  status,
  error,
  query,
  queryLanguage,
  sources,
  reactions,
  onClose,
  onStart,
  onStop,
  onDelete,
}: QueryInspectorPanelProps) {
  const accentColor = getTypeColor("query");
  const showError = status === "Error" && error;

  return (
    <motion.div
      className="inspector-panel"
      initial={{ x: "100%" }}
      animate={{ x: 0 }}
      exit={{ x: "100%" }}
      transition={{ type: "tween", duration: 0.25, ease: "easeInOut" }}
    >
      {/* Header */}
      <div
        className="sticky top-0 z-10 bg-drasi-surface border-b border-drasi-border p-4"
        style={{ borderTopColor: accentColor, borderTopWidth: 3 }}
      >
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-3">
            <div
              className="w-10 h-10 rounded-xl flex items-center justify-center"
              style={{ backgroundColor: `${accentColor}20` }}
            >
              <Search size={20} style={{ color: accentColor }} />
            </div>
            <div>
              <h2 className="text-lg font-bold text-drasi-text-primary">
                {id}
              </h2>
              <div className="flex items-center gap-2">
                <span
                  className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider"
                  style={{ 
                    backgroundColor: `${accentColor}20`,
                    color: accentColor,
                    border: `1px solid ${accentColor}40`
                  }}
                >
                  {queryLanguage}
                </span>
                <span className="text-xs text-drasi-text-secondary">
                  Continuous Query
                </span>
              </div>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg hover:bg-drasi-card text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex items-center justify-between">
          <StatusBadge status={status} size="md" />
          <ActionButtons
            status={status}
            componentName={id}
            onStart={onStart}
            onStop={onStop}
            onDelete={onDelete}
            deleteDisabled={reactions.length > 0}
            deleteDisabledReason={reactions.length > 0 ? `Cannot delete: ${reactions.length} reaction(s) depend on this query` : undefined}
            compact
          />
        </div>
      </div>

      {/* Error Message */}
      {showError && (
        <div className="p-4 border-b border-drasi-border">
          <div className="flex items-start gap-3 p-3 bg-red-500/10 rounded-lg border border-red-500/20">
            <AlertCircle size={18} className="text-red-500 shrink-0 mt-0.5" />
            <div>
              <h3 className="text-xs font-semibold text-red-400 uppercase tracking-wider mb-1">
                Error Details
              </h3>
              <p className="text-sm text-red-300 break-words leading-relaxed">
                {error}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Data Flow Section */}
      <div className="p-4 border-b border-drasi-border">
        <div className="flex items-center gap-2 mb-3">
          <GitBranch size={14} className="text-drasi-text-secondary" />
          <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider">
            Data Flow
          </h3>
        </div>

        <div className="space-y-4">
          {/* Sources - Input */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <Database size={12} className="text-drasi-source" />
              <span className="text-xs font-medium text-drasi-source">
                Sources ({sources.length})
              </span>
              <div className="flex-1 h-px bg-drasi-border" />
              <span className="text-[10px] text-drasi-text-secondary">INPUT</span>
            </div>
            {sources.length > 0 ? (
              <div className="grid gap-2">
                {sources.map((s) => (
                  <div
                    key={s.id}
                    className="flex items-center justify-between p-2.5 rounded-lg bg-drasi-card border border-drasi-border/50 hover:border-drasi-source/30 transition-colors"
                  >
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 rounded-full bg-drasi-source" />
                      <span className="text-sm font-medium text-drasi-text-primary">
                        {s.id}
                      </span>
                      {s.kind && (
                        <span className="text-[10px] text-drasi-text-secondary px-1.5 py-0.5 rounded bg-drasi-bg">
                          {s.kind}
                        </span>
                      )}
                    </div>
                    <StatusBadge status={s.status} />
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-sm text-drasi-text-secondary italic p-2">
                No sources connected
              </div>
            )}
          </div>

          {/* Reactions - Output */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <Zap size={12} className="text-drasi-reaction" />
              <span className="text-xs font-medium text-drasi-reaction">
                Reactions ({reactions.length})
              </span>
              <div className="flex-1 h-px bg-drasi-border" />
              <span className="text-[10px] text-drasi-text-secondary">OUTPUT</span>
            </div>
            {reactions.length > 0 ? (
              <div className="grid gap-2">
                {reactions.map((r) => (
                  <div
                    key={r.id}
                    className="flex items-center justify-between p-2.5 rounded-lg bg-drasi-card border border-drasi-border/50 hover:border-drasi-reaction/30 transition-colors"
                  >
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 rounded-full bg-drasi-reaction" />
                      <span className="text-sm font-medium text-drasi-text-primary">
                        {r.id}
                      </span>
                      {r.kind && (
                        <span className="text-[10px] text-drasi-text-secondary px-1.5 py-0.5 rounded bg-drasi-bg">
                          {r.kind}
                        </span>
                      )}
                    </div>
                    <StatusBadge status={r.status} />
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-sm text-drasi-text-secondary italic p-2">
                No reactions subscribed
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Query Definition */}
      <div className="p-4">
        <div className="flex items-center gap-2 mb-3">
          <Code size={14} className="text-drasi-query" />
          <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider">
            Query Definition
          </h3>
        </div>
        <pre
          className="bg-drasi-bg rounded-xl p-4 text-sm font-mono text-drasi-text-primary
                     overflow-y-auto max-h-96 border border-drasi-border whitespace-pre-wrap break-words
                     leading-relaxed"
        >
          {query}
        </pre>
      </div>
    </motion.div>
  );
}
