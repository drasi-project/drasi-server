import {
  Info,
  LayoutGrid,
  FileStack,
  Package,
  Layers,
  ScrollText,
} from "lucide-react";
import type { SidebarTab } from "./IconRail";
import PanelHeader from "./PanelHeader";

const TAB_META: Record<SidebarTab, { title: string; icon: React.ReactNode }> = {
  component: { title: "Component", icon: <Info size={14} className="text-[var(--drasi-text-secondary)]" /> },
  catalog: { title: "Components", icon: <LayoutGrid size={14} className="text-[var(--drasi-text-secondary)]" /> },
  solutions: { title: "Solutions", icon: <FileStack size={14} className="text-[var(--drasi-text-secondary)]" /> },
  plugins: { title: "Plugins", icon: <Package size={14} className="text-[var(--drasi-text-secondary)]" /> },
  instances: { title: "Instances", icon: <Layers size={14} className="text-[var(--drasi-text-secondary)]" /> },
  logs: { title: "Logs", icon: <ScrollText size={14} className="text-[var(--drasi-text-secondary)]" /> },
};

interface LeftPanelProps {
  activeTab: SidebarTab | null;
  pinned: boolean;
  onTogglePin: () => void;
  children: React.ReactNode;
  headerActions?: React.ReactNode;
}

export default function LeftPanel({
  activeTab,
  pinned,
  onTogglePin,
  children,
  headerActions,
}: LeftPanelProps) {
  const meta = activeTab ? TAB_META[activeTab] : null;

  if (!activeTab || !meta) return null;

  return (
    <div
      className="h-full flex flex-col overflow-hidden bg-[var(--drasi-surface)]"
      role="tabpanel"
      aria-labelledby={`sidebar-tab-${activeTab}`}
    >
      <PanelHeader
        title={meta.title}
        icon={meta.icon}
        pinned={pinned}
        onTogglePin={onTogglePin}
        actions={headerActions}
      />
      <div className="flex-1 overflow-y-auto">{children}</div>
    </div>
  );
}
