import { useCallback } from "react";
import { Info, LayoutGrid, FileStack, Package, Layers, ScrollText } from "lucide-react";

export type SidebarTab = "component" | "catalog" | "solutions" | "plugins" | "instances" | "logs";

interface IconRailProps {
  activeTab: SidebarTab | null;
  onTabClick: (tab: SidebarTab) => void;
  badges?: Partial<Record<SidebarTab, number>>;
  componentSelected?: boolean;
}

const mainTabs: { id: SidebarTab; icon: React.ElementType; label: string }[] = [
  { id: "catalog", icon: LayoutGrid, label: "Components" },
  { id: "solutions", icon: FileStack, label: "Solutions" },
  { id: "plugins", icon: Package, label: "Plugins" },
  { id: "instances", icon: Layers, label: "Instances" },
  { id: "logs", icon: ScrollText, label: "Logs" },
];

const componentTab = { id: "component" as SidebarTab, icon: Info, label: "Selected Component" };

const ALL_TAB_IDS: SidebarTab[] = [...mainTabs.map((t) => t.id), componentTab.id];

export default function IconRail({ activeTab, onTabClick, badges, componentSelected = false }: IconRailProps) {
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const currentIndex = activeTab ? ALL_TAB_IDS.indexOf(activeTab) : -1;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        const next = (currentIndex + 1) % ALL_TAB_IDS.length;
        onTabClick(ALL_TAB_IDS[next]);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        const prev = (currentIndex - 1 + ALL_TAB_IDS.length) % ALL_TAB_IDS.length;
        onTabClick(ALL_TAB_IDS[prev]);
      }
    },
    [activeTab, onTabClick],
  );

  const renderButton = (id: SidebarTab, Icon: React.ElementType, label: string, disabled = false) => {
    const isActive = activeTab === id;
    const badge = badges?.[id];
    return (
      <button
        key={id}
        id={`sidebar-tab-${id}`}
        onClick={() => !disabled && onTabClick(id)}
        role="tab"
        aria-selected={isActive}
        aria-label={label}
        aria-disabled={disabled}
        className={`relative w-10 h-10 flex items-center justify-center rounded-lg transition-colors ${
          disabled
            ? "text-[var(--drasi-text-secondary)] opacity-30 cursor-default"
            : isActive
              ? "bg-[var(--drasi-card)] text-[var(--drasi-text-primary)]"
              : "text-[var(--drasi-text-secondary)] hover:bg-[var(--drasi-card)] hover:text-[var(--drasi-text-primary)] cursor-pointer"
        }`}
        title={disabled ? `${label} (select a component on the canvas)` : label}
      >
        <Icon size={20} />
        {badge != null && badge > 0 && (
          <span className="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-red-500 text-white text-[9px] font-bold flex items-center justify-center leading-none">
            {badge > 99 ? "99+" : badge}
          </span>
        )}
      </button>
    );
  };

  return (
    <div
      className="w-12 flex-shrink-0 flex flex-col items-center py-2 bg-[var(--drasi-surface)] border-r border-[var(--drasi-border)] h-full"
      role="tablist"
      aria-label="Sidebar navigation"
      tabIndex={0}
      onKeyDown={handleKeyDown}
    >
      {/* Main tabs at top */}
      <div className="flex flex-col items-center gap-1">
        {mainTabs.map(({ id, icon, label }) => renderButton(id, icon, label))}
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Component tab at bottom — only active when a component is selected */}
      <div className="flex flex-col items-center gap-1 pb-1">
        {renderButton(componentTab.id, componentTab.icon, componentTab.label, !componentSelected)}
      </div>
    </div>
  );
}
