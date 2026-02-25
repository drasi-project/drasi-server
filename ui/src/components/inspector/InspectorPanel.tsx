import { X, AlertCircle } from "lucide-react";
import StatusBadge from "@/components/shared/StatusBadge";
import ActionButtons from "@/components/shared/ActionButtons";
import type { ComponentStatus, ComponentType } from "@/utils/colors";
import { getTypeColor } from "@/utils/colors";

interface InspectorPanelProps {
  title: string;
  subtitle: string;
  componentType: ComponentType;
  status: ComponentStatus;
  error?: string;
  stats?: { label: string; value: string }[];
  config?: Record<string, unknown>;
  connections?: { id: string; type: ComponentType; status: ComponentStatus }[];
  onClose: () => void;
  onStart?: () => void;
  onStop?: () => void;
  onDelete?: () => void;
  children?: React.ReactNode;
}

export default function InspectorPanel({
  title,
  subtitle,
  componentType,
  status,
  error,
  stats = [],
  config,
  connections = [],
  onClose,
  onStart,
  onStop,
  onDelete,
  children,
}: InspectorPanelProps) {
  const accentColor = getTypeColor(componentType);
  const showError = status === "Error" && error;

  return (
    <div className="inspector-panel">
      {/* Header */}
      <div
        className="sticky top-0 z-10 bg-drasi-surface border-b border-drasi-border p-4"
        style={{ borderTopColor: accentColor, borderTopWidth: 2 }}
      >
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <div
              className="w-2 h-8 rounded-full"
              style={{ backgroundColor: accentColor }}
            />
            <div>
              <h2 className="text-lg font-bold text-drasi-text-primary">
                {title}
              </h2>
              <p className="text-xs text-drasi-text-secondary uppercase tracking-wider">
                {subtitle}
              </p>
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

      {/* Stats */}
      {stats.length > 0 && (
        <div className="p-4 border-b border-drasi-border">
          <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider mb-2">
            Stats
          </h3>
          <div className="space-y-1">
            {stats.map((s) => (
              <div key={s.label} className="stat-row">
                <span className="stat-label">{s.label}</span>
                <span className="stat-value">{s.value}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Connections */}
      {connections.length > 0 && (
        <div className="p-4 border-b border-drasi-border">
          <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider mb-2">
            Connected To
          </h3>
          <div className="space-y-1.5">
            {connections.map((c) => (
              <div
                key={c.id}
                className="flex items-center justify-between p-2 rounded-lg bg-drasi-card"
              >
                <span className="text-sm text-drasi-text-primary">
                  {c.id}
                </span>
                <StatusBadge status={c.status} />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Config */}
      {config && Object.keys(config).length > 0 && (
        <div className="p-4 border-b border-drasi-border">
          <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider mb-2">
            Configuration
          </h3>
          <div className="bg-drasi-card rounded-lg p-3 font-mono text-xs space-y-1">
            {Object.entries(config).map(([key, value]) => (
              <div key={key} className="flex gap-2">
                <span className="text-drasi-text-secondary">{key}:</span>
                <span className="text-drasi-text-primary break-all">
                  {typeof value === "object"
                    ? JSON.stringify(value)
                    : String(value)}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Custom content */}
      {children && <div className="p-4">{children}</div>}
    </div>
  );
}
