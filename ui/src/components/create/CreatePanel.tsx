import { X, Save, Loader2 } from "lucide-react";
import type { DraftState } from "@/hooks/useDraft";

interface CreatePanelProps {
  draft: DraftState;
  title: string;
  subtitle: string;
  accentColor: string;
  onSave: () => void;
  onCancel: () => void;
  children: React.ReactNode;
}

export default function CreatePanel({
  draft,
  title,
  subtitle,
  accentColor,
  onSave,
  onCancel,
  children,
}: CreatePanelProps) {
  const hasErrors = Object.keys(draft.errors).length > 0;

  return (
    <div className="fixed right-0 top-0 h-full w-[460px] bg-drasi-surface border-l border-drasi-border z-50 flex flex-col animate-slide-in-right">
      {/* Header */}
      <div
        className="flex-shrink-0 border-b border-drasi-border p-4"
        style={{ borderTopColor: accentColor, borderTopWidth: 2 }}
      >
        <div className="flex items-center justify-between mb-1">
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
            onClick={onCancel}
            className="p-1.5 rounded-lg hover:bg-drasi-card text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
          >
            <X size={18} />
          </button>
        </div>
      </div>

      {/* Form body */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">{children}</div>

      {/* Footer */}
      <div className="flex-shrink-0 border-t border-drasi-border p-4 flex items-center justify-between">
        <button onClick={onCancel} className="action-btn-ghost">
          Cancel
        </button>
        <button
          onClick={onSave}
          disabled={draft.saving || hasErrors}
          className="action-btn-primary flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {draft.saving ? (
            <Loader2 size={16} className="animate-spin" />
          ) : (
            <Save size={16} />
          )}
          {draft.saving ? "Saving..." : "Save"}
        </button>
      </div>
    </div>
  );
}
