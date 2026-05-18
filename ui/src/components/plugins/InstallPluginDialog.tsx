import { useState, useEffect, useCallback } from "react";
import { Download, Loader2, RefreshCw } from "lucide-react";
import * as api from "@/api/client";
import type { RegistryPlugin } from "@/api/types";

interface InstallPluginDialogProps {
  onClose: () => void;
  onInstalled: () => void;
}

export default function InstallPluginDialog({
  onClose,
  onInstalled,
}: InstallPluginDialogProps) {
  const [registry, setRegistry] = useState("");
  const [plugins, setPlugins] = useState<RegistryPlugin[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState("");
  const [installStatus, setInstallStatus] = useState<Record<string, "pending" | "installing" | "done" | "error">>({});

  const fetchPlugins = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const results = await api.searchRegistry(
        "*",
        registry.trim() || undefined,
      );
      setPlugins(results.sort((a, b) => a.reference.localeCompare(b.reference)));
      setSelected(new Set());
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to search registry",
      );
      setPlugins([]);
    } finally {
      setLoading(false);
    }
  }, [registry]);

  useEffect(() => {
    fetchPlugins();
  }, [fetchPlugins]);

  const togglePlugin = (ref: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(ref)) next.delete(ref);
      else next.add(ref);
      return next;
    });
  };

  const toggleAll = () => {
    if (selected.size === plugins.length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(plugins.map((p) => p.reference)));
    }
  };

  const handleInstall = async () => {
    if (selected.size === 0) return;
    setInstalling(true);
    setError("");

    const status: Record<string, "pending" | "installing" | "done" | "error"> = {};
    for (const ref of selected) status[ref] = "pending";
    setInstallStatus(status);

    let anySuccess = false;
    for (const ref of selected) {
      setInstallStatus((prev) => ({ ...prev, [ref]: "installing" }));
      try {
        await api.installPlugin(ref, registry.trim() || undefined);
        setInstallStatus((prev) => ({ ...prev, [ref]: "done" }));
        anySuccess = true;
      } catch (err) {
        setInstallStatus((prev) => ({ ...prev, [ref]: "error" }));
        setError(
          `Failed to install ${ref}: ${err instanceof Error ? err.message : "Unknown error"}`,
        );
      }
    }

    setInstalling(false);
    if (anySuccess) onInstalled();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-lg w-full mx-4 shadow-2xl max-h-[80vh] flex flex-col">
        <h2 className="text-lg font-bold text-drasi-text-primary mb-4">
          Install Plugins
        </h2>

        {/* Registry URL */}
        <div className="mb-3">
          <label className="block text-xs font-medium text-[var(--drasi-text-secondary)] mb-1">
            Plugin Registry
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              placeholder="Default registry (or enter URL / path)"
              value={registry}
              onChange={(e) => setRegistry(e.target.value)}
              className="flex-1 px-3 py-1.5 text-xs rounded-lg bg-[var(--drasi-card)] border border-[var(--drasi-border)] text-[var(--drasi-text-primary)] placeholder:text-[var(--drasi-text-secondary)] focus:outline-none focus:border-[var(--drasi-text-secondary)]"
              disabled={installing}
            />
            <button
              onClick={fetchPlugins}
              disabled={loading || installing}
              className="p-1.5 rounded-lg border border-[var(--drasi-border)] text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] hover:bg-[var(--drasi-card)] transition-colors disabled:opacity-50"
              title="Refresh"
            >
              <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
            </button>
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="mb-3 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/30 text-xs text-red-400">
            {error}
          </div>
        )}

        {/* Plugin list */}
        <div className="flex-1 overflow-y-auto min-h-0 border border-[var(--drasi-border)] rounded-lg">
          {loading ? (
            <div className="flex items-center justify-center h-32 text-xs text-[var(--drasi-text-secondary)]">
              <Loader2 size={16} className="animate-spin mr-2" />
              Searching registry…
            </div>
          ) : plugins.length === 0 ? (
            <div className="flex items-center justify-center h-32 text-xs text-[var(--drasi-text-secondary)]">
              No plugins found
            </div>
          ) : (
            <>
              {/* Select all header */}
              <div className="sticky top-0 bg-[var(--drasi-surface)] border-b border-[var(--drasi-border)] px-3 py-2 flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={selected.size === plugins.length && plugins.length > 0}
                  onChange={toggleAll}
                  disabled={installing}
                  className="rounded"
                />
                <span className="text-[10px] font-medium text-[var(--drasi-text-secondary)] uppercase tracking-wider">
                  {selected.size > 0
                    ? `${selected.size} of ${plugins.length} selected`
                    : `${plugins.length} available`}
                </span>
              </div>

              {/* Plugin rows */}
              {plugins.map((p) => {
                const status = installStatus[p.reference];
                return (
                  <label
                    key={p.reference}
                    className={`flex items-center gap-3 px-3 py-2.5 border-b border-[var(--drasi-border)]/50 cursor-pointer hover:bg-[var(--drasi-card)]/50 transition-colors ${
                      installing ? "opacity-75" : ""
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={selected.has(p.reference)}
                      onChange={() => togglePlugin(p.reference)}
                      disabled={installing}
                      className="rounded flex-shrink-0"
                    />
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium text-[var(--drasi-text-primary)] truncate">
                          {p.reference}
                        </span>
                        {p.version && (
                          <span className="text-[10px] text-[var(--drasi-text-secondary)] flex-shrink-0">
                            v{p.version}
                          </span>
                        )}
                      </div>
                      <div className="text-[10px] text-[var(--drasi-text-secondary)] truncate">
                        {p.source === "local" ? p.filename : p.fullReference}
                      </div>
                    </div>
                    {/* Install status indicator */}
                    {status === "installing" && (
                      <Loader2
                        size={14}
                        className="animate-spin text-blue-400 flex-shrink-0"
                      />
                    )}
                    {status === "done" && (
                      <span className="text-[10px] text-green-400 flex-shrink-0">
                        ✓
                      </span>
                    )}
                    {status === "error" && (
                      <span className="text-[10px] text-red-400 flex-shrink-0">
                        ✗
                      </span>
                    )}
                  </label>
                );
              })}
            </>
          )}
        </div>

        {/* Actions */}
        <div className="flex justify-end gap-2 mt-4">
          <button
            onClick={onClose}
            disabled={installing}
            className="px-4 py-1.5 text-xs rounded-lg border border-[var(--drasi-border)] text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] hover:bg-[var(--drasi-card)] transition-colors disabled:opacity-50"
          >
            {installing ? "Close" : "Cancel"}
          </button>
          <button
            onClick={handleInstall}
            disabled={selected.size === 0 || installing || loading}
            className="px-4 py-1.5 text-xs rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors disabled:opacity-50 flex items-center gap-1.5"
          >
            {installing ? (
              <>
                <Loader2 size={12} className="animate-spin" />
                Installing…
              </>
            ) : (
              <>
                <Download size={12} />
                Install ({selected.size})
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
