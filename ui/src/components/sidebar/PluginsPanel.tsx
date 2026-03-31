import { useEffect, useState, useMemo } from "react";
import { Package, Loader2, RefreshCw, Search, X } from "lucide-react";

interface PluginKindInfo {
  category: string;
  kind: string;
  configVersion: string;
}

interface PluginInfo {
  id: string;
  status: string;
  pluginVersion: string;
  sdkVersion: string;
  filePath: string;
  fileHash: string;
  loadedAt: string;
  kinds: PluginKindInfo[];
  dependentCount: number;
  libraryGeneration: number;
}

type PluginFilter = "all" | "source" | "reaction" | "bootstrap";

const FILTERS: { id: PluginFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "source", label: "Source" },
  { id: "reaction", label: "Reaction" },
  { id: "bootstrap", label: "Bootstrap" },
];

interface PluginsPanelProps {
  onRefreshAction?: React.ReactNode;
}

export default function PluginsPanel({ onRefreshAction }: PluginsPanelProps) {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchText, setSearchText] = useState("");
  const [filter, setFilter] = useState<PluginFilter>("all");

  const fetchPlugins = () => {
    setLoading(true);
    setError(null);
    fetch("/api/v1/plugins")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then((data) => {
        setPlugins(data.plugins || []);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setLoading(false);
      });
  };

  useEffect(() => {
    fetchPlugins();
  }, []);

  const filtered = useMemo(() => {
    let result = plugins;
    if (filter !== "all") {
      result = result.filter((p) =>
        p.kinds.some((k) => k.category.toLowerCase() === filter),
      );
    }
    if (searchText.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(
        (p) =>
          p.id.toLowerCase().includes(q) ||
          p.kinds.some(
            (k) =>
              k.kind.toLowerCase().includes(q) ||
              k.category.toLowerCase().includes(q),
          ),
      );
    }
    return result.sort((a, b) => a.id.localeCompare(b.id));
  }, [plugins, filter, searchText]);

  // Expose refresh button for PanelHeader actions
  void onRefreshAction;

  return (
    <div className="flex flex-col h-full">
      {/* Search */}
      <div className="px-3 pt-3 pb-2 flex-shrink-0">
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search
              size={14}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--drasi-text-secondary)]"
            />
            <input
              type="text"
              placeholder="Search plugins…"
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
              className="w-full pl-8 pr-8 py-1.5 text-xs rounded-lg bg-[var(--drasi-card)] border border-[var(--drasi-border)] text-[var(--drasi-text-primary)] placeholder:text-[var(--drasi-text-secondary)] focus:outline-none focus:border-[var(--drasi-text-secondary)] transition-colors"
            />
            {searchText && (
              <button
                onClick={() => setSearchText("")}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)]"
              >
                <X size={12} />
              </button>
            )}
          </div>
          <button
            onClick={fetchPlugins}
            className="p-1.5 rounded-lg hover:bg-[var(--drasi-card)] text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] transition-colors flex-shrink-0"
            title="Refresh"
          >
            <RefreshCw size={14} />
          </button>
        </div>
      </div>

      {/* Filter chips */}
      <div className="px-3 pb-2 flex gap-1 flex-wrap flex-shrink-0">
        {FILTERS.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors ${
              filter === f.id
                ? "bg-[var(--drasi-card)] text-[var(--drasi-text-primary)] border border-[var(--drasi-text-secondary)]"
                : "text-[var(--drasi-text-secondary)] border border-[var(--drasi-border)] hover:border-[var(--drasi-text-secondary)]"
            }`}
          >
            {f.label}
          </button>
        ))}
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {loading ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Loader2 size={24} className="animate-spin text-drasi-query mb-3" />
            <span className="text-sm text-[var(--drasi-text-secondary)]">
              Loading plugins…
            </span>
          </div>
        ) : error ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Package size={32} className="text-drasi-error opacity-50 mb-3" />
            <p className="text-sm text-drasi-error">Error: {error}</p>
            <button
              onClick={fetchPlugins}
              className="mt-3 text-xs text-drasi-query hover:underline"
            >
              Retry
            </button>
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Package
              size={32}
              className="text-[var(--drasi-text-secondary)] opacity-30 mb-3"
            />
            <p className="text-sm text-[var(--drasi-text-secondary)] font-medium">
              {plugins.length === 0 ? "No plugins loaded" : "No matching plugins"}
            </p>
            {plugins.length === 0 && (
              <p className="text-xs text-[var(--drasi-text-secondary)] mt-1 opacity-70">
                Plugins are loaded from the plugins directory at startup
              </p>
            )}
          </div>
        ) : (
          <div className="space-y-2">
            {filtered.map((plugin) => (
              <PluginCard key={plugin.id} plugin={plugin} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

const STATUS_STYLES: Record<string, string> = {
  Loaded: "bg-blue-500/15 text-blue-400",
  Active: "bg-emerald-500/15 text-emerald-400",
  Draining: "bg-yellow-500/15 text-yellow-400",
  Retired: "bg-[var(--drasi-card)] text-[var(--drasi-text-secondary)]",
  Failed: "bg-red-500/15 text-red-400",
};

function PluginCard({ plugin }: { plugin: PluginInfo }) {
  const [expanded, setExpanded] = useState(false);
  const statusStyle =
    STATUS_STYLES[plugin.status] ??
    "bg-[var(--drasi-card)] text-[var(--drasi-text-secondary)]";

  return (
    <button
      onClick={() => setExpanded((p) => !p)}
      className="w-full text-left border border-[var(--drasi-border)] rounded-xl p-3 hover:border-[var(--drasi-text-secondary)] transition-colors min-w-0"
    >
      <div className="flex items-center justify-between">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-[var(--drasi-text-primary)] truncate">
            {plugin.id}
          </div>
          <div className="text-[10px] text-[var(--drasi-text-secondary)] mt-0.5 truncate">
            {plugin.kinds
              .map((k) => `${k.category}/${k.kind}`)
              .join(", ") || "No kinds"}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0 ml-2">
          <span className="text-[10px] text-[var(--drasi-text-secondary)]">
            {plugin.dependentCount} dep
            {plugin.dependentCount !== 1 ? "s" : ""}
          </span>
          <span
            className={`px-2 py-0.5 rounded-full text-[10px] font-medium ${statusStyle}`}
          >
            {plugin.status}
          </span>
        </div>
      </div>

      {expanded && (
        <div className="mt-3 pt-3 border-t border-[var(--drasi-border)] space-y-1.5 text-[11px] text-[var(--drasi-text-secondary)]">
          <div className="flex justify-between">
            <span>Version</span>
            <span className="text-[var(--drasi-text-primary)] font-mono">
              {plugin.pluginVersion || "—"}
            </span>
          </div>
          <div className="flex justify-between">
            <span>SDK</span>
            <span className="text-[var(--drasi-text-primary)] font-mono">
              {plugin.sdkVersion || "—"}
            </span>
          </div>
          <div className="flex justify-between">
            <span>Generation</span>
            <span className="text-[var(--drasi-text-primary)] font-mono">
              {plugin.libraryGeneration}
            </span>
          </div>
          {plugin.loadedAt && (
            <div className="flex justify-between">
              <span>Loaded</span>
              <span className="text-[var(--drasi-text-primary)]">
                {new Date(plugin.loadedAt).toLocaleString()}
              </span>
            </div>
          )}
          {plugin.filePath && (
            <div className="mt-1">
              <span>Path</span>
              <div className="text-[var(--drasi-text-primary)] font-mono text-[10px] mt-0.5 break-all">
                {plugin.filePath}
              </div>
            </div>
          )}
        </div>
      )}
    </button>
  );
}
