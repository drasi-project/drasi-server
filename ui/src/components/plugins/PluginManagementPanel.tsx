import { useEffect, useState } from "react";
import { X, Package, Loader2, RefreshCw } from "lucide-react";

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

interface PluginManagementPanelProps {
  onClose: () => void;
}

export function PluginManagementPanel({ onClose }: PluginManagementPanelProps) {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="fixed right-0 top-0 h-full w-[460px] bg-drasi-surface border-l border-drasi-border z-50 flex flex-col animate-slide-in-right">
      {/* Header */}
      <div className="flex-shrink-0 border-b border-drasi-border p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Package size={18} className="text-drasi-accent" />
            <h2 className="text-lg font-bold text-drasi-text-primary">
              Plugins
            </h2>
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={fetchPlugins}
              className="p-1.5 rounded-lg hover:bg-drasi-card text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
              title="Refresh"
            >
              <RefreshCw size={16} />
            </button>
            <button
              onClick={onClose}
              className="p-1.5 rounded-lg hover:bg-drasi-card text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
            >
              <X size={18} />
            </button>
          </div>
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto p-4">
        {loading ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Loader2 size={24} className="animate-spin text-drasi-accent mb-3" />
            <span className="text-sm text-drasi-text-secondary">
              Loading plugins...
            </span>
          </div>
        ) : error ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Package size={32} className="text-drasi-error opacity-50 mb-3" />
            <p className="text-sm text-drasi-error">Error: {error}</p>
            <button
              onClick={fetchPlugins}
              className="mt-3 text-xs text-drasi-accent hover:underline"
            >
              Retry
            </button>
          </div>
        ) : plugins.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Package
              size={32}
              className="text-drasi-text-secondary opacity-30 mb-3"
            />
            <p className="text-sm text-drasi-text-secondary font-medium">
              No plugins loaded
            </p>
            <p className="text-xs text-drasi-text-secondary mt-1 opacity-70">
              Plugins are loaded from the plugins directory at startup
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {plugins.map((plugin) => (
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
  Retired: "bg-drasi-card text-drasi-text-secondary",
  Failed: "bg-red-500/15 text-red-400",
};

function PluginCard({ plugin }: { plugin: PluginInfo }) {
  const [expanded, setExpanded] = useState(false);
  const statusStyle =
    STATUS_STYLES[plugin.status] ?? "bg-drasi-card text-drasi-text-secondary";

  return (
    <button
      onClick={() => setExpanded((p) => !p)}
      className="w-full text-left border border-drasi-border rounded-xl p-3 hover:border-drasi-text-secondary transition-colors"
    >
      <div className="flex items-center justify-between">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-drasi-text-primary truncate">
            {plugin.id}
          </div>
          <div className="text-[10px] text-drasi-text-secondary mt-0.5">
            {plugin.kinds
              .map((k) => `${k.category}/${k.kind}`)
              .join(", ") || "No kinds"}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0 ml-2">
          <span className="text-[10px] text-drasi-text-secondary">
            {plugin.dependentCount} dep{plugin.dependentCount !== 1 ? "s" : ""}
          </span>
          <span
            className={`px-2 py-0.5 rounded-full text-[10px] font-medium ${statusStyle}`}
          >
            {plugin.status}
          </span>
        </div>
      </div>

      {expanded && (
        <div className="mt-3 pt-3 border-t border-drasi-border space-y-1.5 text-[11px] text-drasi-text-secondary">
          <div className="flex justify-between">
            <span>Version</span>
            <span className="text-drasi-text-primary font-mono">
              {plugin.pluginVersion || "—"}
            </span>
          </div>
          <div className="flex justify-between">
            <span>SDK</span>
            <span className="text-drasi-text-primary font-mono">
              {plugin.sdkVersion || "—"}
            </span>
          </div>
          <div className="flex justify-between">
            <span>Generation</span>
            <span className="text-drasi-text-primary font-mono">
              {plugin.libraryGeneration}
            </span>
          </div>
          {plugin.loadedAt && (
            <div className="flex justify-between">
              <span>Loaded</span>
              <span className="text-drasi-text-primary">
                {new Date(plugin.loadedAt).toLocaleString()}
              </span>
            </div>
          )}
          {plugin.filePath && (
            <div className="mt-1">
              <span>Path</span>
              <div className="text-drasi-text-primary font-mono text-[10px] mt-0.5 break-all">
                {plugin.filePath}
              </div>
            </div>
          )}
        </div>
      )}
    </button>
  );
}

export default PluginManagementPanel;
