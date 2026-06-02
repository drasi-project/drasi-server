import { Sun, Moon } from "lucide-react";
import DrasiLogo from "@/components/DrasiLogo";
import type { ConnectionState } from "@/hooks/useApi";

interface AppLayoutProps {
  children: React.ReactNode;
  connectionState?: ConnectionState;
  instanceSlot?: React.ReactNode;
  theme?: "light" | "dark";
  onToggleTheme?: () => void;
}

export default function AppLayout({
  children,
  connectionState = "disconnected",
  instanceSlot,
  theme = "dark",
  onToggleTheme,
}: AppLayoutProps) {
  const connectionDisplay = {
    connected: { dot: "bg-drasi-running animate-pulse-glow", label: "Live" },
    connecting: { dot: "bg-amber-400 animate-pulse", label: "Connecting..." },
    disconnected: { dot: "bg-drasi-error", label: "Disconnected" },
  }[connectionState];

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
          {/* Connection status — driven by SSE connection, no polling */}
          <div className="flex items-center gap-1.5">
            <span className={`w-2 h-2 rounded-full ${connectionDisplay.dot}`} />
            <span className="text-xs text-drasi-text-secondary">
              {connectionDisplay.label}
            </span>
          </div>

          {/* Theme toggle */}
          <button
            onClick={onToggleTheme}
            className="p-1.5 rounded-lg text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
            title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          >
            {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
          </button>
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1 overflow-hidden relative" style={{ minHeight: 0 }}>
        {children}
      </main>
    </div>
  );
}
