import { Plus, Copy, Package, Layers, ArrowRight } from "lucide-react";
import type { InstanceInfo } from "@/api/types";

interface InstancesPanelProps {
  instances: InstanceInfo[];
  selectedId?: string;
  onSelect: (id: string) => void;
  onCreateNew: () => void;
  onCreateFromTemplate?: () => void;
  onClone?: () => void;
  onCreateTemplate?: () => void;
}

export default function InstancesPanel({
  instances,
  selectedId,
  onSelect,
  onCreateNew,
  onCreateFromTemplate,
  onClone,
  onCreateTemplate,
}: InstancesPanelProps) {
  return (
    <div className="flex flex-col h-full">
      {/* Action buttons */}
      <div className="px-3 pt-3 pb-2 flex-shrink-0 space-y-1">
        <button
          onClick={onCreateNew}
          className="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-xs text-[var(--drasi-text-secondary)] hover:bg-[var(--drasi-card)] hover:text-[var(--drasi-text-primary)] transition-colors border border-[var(--drasi-border)]"
        >
          <Plus size={14} />
          Create New Instance
        </button>
        {onCreateFromTemplate && (
          <button
            onClick={onCreateFromTemplate}
            className="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-xs text-[var(--drasi-text-secondary)] hover:bg-[var(--drasi-card)] hover:text-[var(--drasi-text-primary)] transition-colors border border-[var(--drasi-border)]"
          >
            <Package size={14} />
            Create from Template
          </button>
        )}
      </div>

      {/* Instance list */}
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {instances.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Layers
              size={32}
              className="text-[var(--drasi-text-secondary)] opacity-30 mb-3"
            />
            <p className="text-sm text-[var(--drasi-text-secondary)] font-medium">
              No instances
            </p>
          </div>
        ) : (
          <div className="space-y-2 mt-1">
            {instances.map((inst) => {
              const isActive = inst.id === selectedId;
              return (
                <div
                  key={inst.id}
                  className={`rounded-xl border p-3 transition-colors min-w-0 ${
                    isActive
                      ? "border-drasi-query bg-drasi-query/5"
                      : "border-[var(--drasi-border)] hover:border-[var(--drasi-text-secondary)]"
                  }`}
                >
                  <div className="flex items-center justify-between mb-1.5">
                    <div className="flex items-center gap-2 min-w-0">
                      {isActive && (
                        <span className="w-2 h-2 rounded-full bg-drasi-running flex-shrink-0" />
                      )}
                      <span className="text-sm font-semibold text-[var(--drasi-text-primary)] truncate font-mono">
                        {inst.id}
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center gap-3 text-[10px] text-[var(--drasi-text-secondary)] mb-2">
                    <span>{inst.source_count} Sources</span>
                    <span>{inst.query_count} Queries</span>
                    <span>{inst.reaction_count} Reactions</span>
                  </div>

                  <div className="flex items-center gap-1">
                    {!isActive && (
                      <button
                        onClick={() => onSelect(inst.id)}
                        className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium text-drasi-query hover:bg-drasi-query/10 transition-colors"
                      >
                        <ArrowRight size={10} />
                        Switch to
                      </button>
                    )}
                    {isActive && onClone && (
                      <button
                        onClick={onClone}
                        className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] hover:bg-[var(--drasi-card)] transition-colors"
                      >
                        <Copy size={10} />
                        Clone
                      </button>
                    )}
                    {isActive && onCreateTemplate && (
                      <button
                        onClick={onCreateTemplate}
                        className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] hover:bg-[var(--drasi-card)] transition-colors"
                      >
                        <Package size={10} />
                        Template
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
