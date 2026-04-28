import { Pin, PinOff } from "lucide-react";

interface PanelHeaderProps {
  title: string;
  icon?: React.ReactNode;
  pinned: boolean;
  onTogglePin: () => void;
  actions?: React.ReactNode;
}

export default function PanelHeader({
  title,
  icon,
  pinned,
  onTogglePin,
  actions,
}: PanelHeaderProps) {
  return (
    <div className="flex items-center justify-between px-3 py-2.5 border-b border-[var(--drasi-border)] bg-[var(--drasi-surface)] flex-shrink-0">
      <div className="flex items-center gap-2 min-w-0">
        {icon}
        <span className="text-sm font-semibold text-[var(--drasi-text-primary)] truncate">
          {title}
        </span>
      </div>
      <div className="flex items-center gap-1 flex-shrink-0">
        {actions}
        <button
          onClick={onTogglePin}
          className="p-1 rounded hover:bg-[var(--drasi-card)] text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] transition-colors"
          title={pinned ? "Unpin panel" : "Pin panel open"}
          aria-label={pinned ? "Unpin sidebar" : "Pin sidebar open"}
          aria-pressed={pinned}
        >
          {pinned ? <PinOff size={14} /> : <Pin size={14} />}
        </button>
      </div>
    </div>
  );
}
