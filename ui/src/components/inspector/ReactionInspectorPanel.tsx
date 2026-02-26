import { X, AlertCircle, Zap, Search, GitBranch } from "lucide-react";
import { motion } from "framer-motion";
import StatusBadge from "@/components/shared/StatusBadge";
import ActionButtons from "@/components/shared/ActionButtons";
import type { ComponentStatus, ComponentType } from "@/utils/colors";
import { getTypeColor } from "@/utils/colors";

interface ConnectedComponent {
  id: string;
  type: ComponentType;
  status: ComponentStatus;
}

interface ReactionInspectorPanelProps {
  id: string;
  kind: string;
  status: ComponentStatus;
  error?: string;
  autoStart?: boolean;
  properties?: Record<string, unknown>;
  queries: ConnectedComponent[];
  onClose: () => void;
  onStart?: () => void;
  onStop?: () => void;
  onDelete?: () => void;
}

export default function ReactionInspectorPanel({
  id,
  kind,
  status,
  error,
  autoStart,
  properties,
  queries,
  onClose,
  onStart,
  onStop,
  onDelete,
}: ReactionInspectorPanelProps) {
  const accentColor = getTypeColor("reaction");
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
              <Zap size={20} style={{ color: accentColor }} />
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
                  {kind}
                </span>
                <span className="text-xs text-drasi-text-secondary">
                  Reaction
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

        <div>
          <div className="flex items-center gap-2 mb-2">
            <Search size={12} className="text-drasi-query" />
            <span className="text-xs font-medium text-drasi-query">
              Queries ({queries.length})
            </span>
            <div className="flex-1 h-px bg-drasi-border" />
            <span className="text-[10px] text-drasi-text-secondary">INPUT</span>
          </div>
          {queries.length > 0 ? (
            <div className="grid gap-2">
              {queries.map((q) => (
                <div
                  key={q.id}
                  className="flex items-center justify-between p-2.5 rounded-lg bg-drasi-card border border-drasi-border/50 hover:border-drasi-query/30 transition-colors"
                >
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full bg-drasi-query" />
                    <span className="text-sm font-medium text-drasi-text-primary">
                      {q.id}
                    </span>
                  </div>
                  <StatusBadge status={q.status} />
                </div>
              ))}
            </div>
          ) : (
            <div className="text-sm text-drasi-text-secondary italic p-2">
              No queries subscribed
            </div>
          )}
        </div>
      </div>

      {/* Configuration */}
      <div className="p-4">
        <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider mb-3">
          Configuration
        </h3>
        <div className="space-y-2">
          <div className="flex items-center justify-between p-2.5 rounded-lg bg-drasi-card border border-drasi-border/50">
            <span className="text-sm text-drasi-text-secondary">Auto Start</span>
            <span className="text-sm font-medium text-drasi-text-primary">
              {autoStart ? "Yes" : "No"}
            </span>
          </div>
          {properties && Object.entries(properties).map(([key, value]) => (
            <div
              key={key}
              className="flex items-center justify-between p-2.5 rounded-lg bg-drasi-card border border-drasi-border/50"
            >
              <span className="text-sm text-drasi-text-secondary">{key}</span>
              <span className="text-sm font-medium text-drasi-text-primary font-mono truncate max-w-[200px]">
                {typeof value === "object" ? JSON.stringify(value) : String(value)}
              </span>
            </div>
          ))}
        </div>
      </div>
    </motion.div>
  );
}
