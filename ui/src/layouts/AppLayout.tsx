import { Plus, Activity } from "lucide-react";
import DrasiLogo from "@/components/DrasiLogo";

interface AppLayoutProps {
  children: React.ReactNode;
  onAddComponent?: () => void;
  connected?: boolean;
  instanceSlot?: React.ReactNode;
}

export default function AppLayout({
  children,
  onAddComponent,
  connected = false,
  instanceSlot,
}: AppLayoutProps) {
  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden bg-drasi-bg">
      {/* Top Bar */}
      <header className="h-12 flex items-center justify-between px-4 border-b border-drasi-border bg-drasi-surface flex-shrink-0">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <DrasiLogo iconOnly size={22} />
            <span className="text-sm font-bold text-drasi-text-primary tracking-tight">
              DRASI SERVER
            </span>
          </div>
          {instanceSlot}
        </div>

        <div className="flex items-center gap-3">
          {/* Connection status */}
          <div className="flex items-center gap-1.5">
            <span
              className={`w-2 h-2 rounded-full ${connected ? "bg-drasi-running animate-pulse-glow" : "bg-drasi-error"}`}
            />
            <span className="text-xs text-drasi-text-secondary">
              {connected ? "Live" : "Disconnected"}
            </span>
          </div>

          {/* Add button */}
          {onAddComponent && (
            <button
              onClick={onAddComponent}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-drasi-card border border-drasi-border text-sm text-drasi-text-primary hover:bg-drasi-surface hover:border-drasi-text-secondary transition-all"
            >
              <Plus size={14} />
              <span>Add</span>
            </button>
          )}

          {/* Activity indicator */}
          <Activity size={16} className="text-drasi-text-secondary" />
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1 overflow-hidden relative" style={{ minHeight: 0 }}>
        {children}
      </main>
    </div>
  );
}
