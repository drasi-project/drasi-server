import { useState, useEffect, useMemo } from "react";
import {
  Search,
  X,
  Package,
  Upload,
  Plus,
  Loader2,
  Trash2,
  Rocket,
  Database,
  Zap,
} from "lucide-react";
import * as api from "@/api/client";
import type { SolutionTemplateSummary } from "@/api/types";

interface SolutionTemplatesPanelProps {
  instanceId: string;
  sources: { id: string }[];
  queries: { id: string }[];
  reactions: { id: string }[];
  onDeployTemplate: (templateId: string) => void;
  onUploadYaml: (yaml: string) => void;
  onCreateTemplate: () => void;
}

export default function SolutionTemplatesPanel({
  instanceId,
  sources,
  queries,
  reactions,
  onDeployTemplate,
  onUploadYaml,
  onCreateTemplate,
}: SolutionTemplatesPanelProps) {
  const [templates, setTemplates] = useState<SolutionTemplateSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchText, setSearchText] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  // Suppress unused-var warnings for future use
  void instanceId;
  void sources;
  void queries;
  void reactions;

  const fetchTemplates = () => {
    setLoading(true);
    setError(null);
    api
      .listSolutions()
      .then(setTemplates)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load"))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    fetchTemplates();
  }, []);

  const filtered = useMemo(() => {
    if (!searchText.trim()) return templates;
    const q = searchText.toLowerCase();
    return templates.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        (t.description ?? "").toLowerCase().includes(q) ||
        t.id.toLowerCase().includes(q) ||
        (t.author ?? "").toLowerCase().includes(q),
    );
  }, [templates, searchText]);

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      onUploadYaml(reader.result as string);
    };
    reader.readAsText(file);
    // Reset so the same file can be re-selected
    e.target.value = "";
  };

  const handleDelete = async (templateId: string) => {
    setDeleting(true);
    try {
      await api.deleteSolution(templateId);
      setTemplates((prev) => prev.filter((t) => t.id !== templateId));
      setConfirmDeleteId(null);
    } catch {
      // Silently fail — template may have been removed on disk
      setConfirmDeleteId(null);
      fetchTemplates();
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Search */}
      <div className="px-3 pt-3 pb-2 flex-shrink-0">
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--drasi-text-secondary)]"
          />
          <input
            type="text"
            placeholder="Search templates…"
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
      </div>

      {/* Action buttons */}
      <div className="px-3 pb-2 flex gap-1.5 flex-shrink-0">
        <button
          onClick={onCreateTemplate}
          className="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-lg text-[10px] font-medium text-[var(--drasi-text-secondary)] hover:bg-[var(--drasi-card)] hover:text-[var(--drasi-text-primary)] transition-colors border border-[var(--drasi-border)]"
        >
          <Plus size={12} />
          Create
        </button>
        <label className="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-lg text-[10px] font-medium text-[var(--drasi-text-secondary)] hover:bg-[var(--drasi-card)] hover:text-[var(--drasi-text-primary)] transition-colors border border-[var(--drasi-border)] cursor-pointer">
          <Upload size={12} />
          Upload YAML
          <input
            type="file"
            accept=".yaml,.yml"
            onChange={handleFileUpload}
            className="hidden"
          />
        </label>
      </div>

      {/* Template list */}
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {loading ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Loader2 size={24} className="animate-spin text-drasi-query mb-3" />
            <span className="text-sm text-[var(--drasi-text-secondary)]">
              Loading templates…
            </span>
          </div>
        ) : error ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Package size={32} className="text-drasi-error opacity-50 mb-3" />
            <p className="text-sm text-drasi-error">Error: {error}</p>
            <button
              onClick={fetchTemplates}
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
              {templates.length === 0
                ? "No templates available"
                : "No matching templates"}
            </p>
            {templates.length === 0 && (
              <p className="text-xs text-[var(--drasi-text-secondary)] mt-1 opacity-70">
                Upload a YAML or create from existing components
              </p>
            )}
          </div>
        ) : (
          <div className="space-y-2">
            {filtered.map((t) => (
              <div
                key={t.id}
                className="border border-[var(--drasi-border)] rounded-xl p-3 hover:border-[var(--drasi-text-secondary)] transition-colors"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-semibold text-[var(--drasi-text-primary)] truncate">
                      {t.name}
                    </div>
                    {t.description && (
                      <p className="text-[10px] text-[var(--drasi-text-secondary)] mt-0.5 line-clamp-2">
                        {t.description}
                      </p>
                    )}
                    <div className="flex items-center gap-3 mt-1.5 flex-wrap">
                      {t.version && (
                        <span className="text-[10px] text-[var(--drasi-text-secondary)] px-1.5 py-0.5 rounded-full bg-[var(--drasi-card)]">
                          v{t.version}
                        </span>
                      )}
                      {t.author && (
                        <span className="text-[10px] text-[var(--drasi-text-secondary)]">
                          by {t.author}
                        </span>
                      )}
                      <div className="flex items-center gap-2 text-[10px] text-[var(--drasi-text-secondary)]">
                        <span className="flex items-center gap-0.5">
                          <Database size={10} className="text-drasi-source" />
                          {t.sourceCount}
                        </span>
                        <span className="flex items-center gap-0.5">
                          <Search size={10} className="text-drasi-query" />
                          {t.queryCount}
                        </span>
                        <span className="flex items-center gap-0.5">
                          <Zap size={10} className="text-drasi-reaction" />
                          {t.reactionCount}
                        </span>
                      </div>
                    </div>
                  </div>
                </div>

                {/* Actions */}
                <div className="flex items-center gap-1.5 mt-2 pt-2 border-t border-[var(--drasi-border)]">
                  <button
                    onClick={() => onDeployTemplate(t.id)}
                    className="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-lg text-[10px] font-medium bg-[var(--drasi-accent)]/10 text-[var(--drasi-accent)] hover:bg-[var(--drasi-accent)]/20 transition-colors"
                  >
                    <Rocket size={12} />
                    Deploy
                  </button>
                  {confirmDeleteId === t.id ? (
                    <div className="flex items-center gap-1">
                      <button
                        onClick={() => handleDelete(t.id)}
                        disabled={deleting}
                        className="px-2 py-1.5 rounded-lg text-[10px] font-medium bg-drasi-error/10 text-drasi-error hover:bg-drasi-error/20 transition-colors disabled:opacity-50"
                      >
                        {deleting ? "…" : "Confirm"}
                      </button>
                      <button
                        onClick={() => setConfirmDeleteId(null)}
                        className="px-2 py-1.5 rounded-lg text-[10px] text-[var(--drasi-text-secondary)] hover:text-[var(--drasi-text-primary)] transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                  ) : (
                    <button
                      onClick={() => setConfirmDeleteId(t.id)}
                      className="p-1.5 rounded-lg text-[var(--drasi-text-secondary)] hover:text-drasi-error hover:bg-drasi-error/10 transition-colors"
                      title="Delete template"
                    >
                      <Trash2 size={12} />
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
