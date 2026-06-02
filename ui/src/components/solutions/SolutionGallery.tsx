import { useState, useEffect } from "react";
import { Package, Upload, ChevronRight, Loader2 } from "lucide-react";
import * as api from "@/api/client";
import type { SolutionTemplateSummary } from "@/api/types";

interface SolutionGalleryProps {
  onSelect: (templateId: string) => void;
  onUpload: (yaml: string) => void;
  onClose: () => void;
}

export default function SolutionGallery({
  onSelect,
  onUpload,
  onClose,
}: SolutionGalleryProps) {
  const [templates, setTemplates] = useState<SolutionTemplateSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        const list = await api.listSolutions();
        setTemplates(list);
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load solutions");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = () => {
      const content = reader.result as string;
      onUpload(content);
    };
    reader.readAsText(file);
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-drasi-surface border border-drasi-border rounded-lg shadow-xl w-[600px] max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex-shrink-0 border-b border-drasi-border p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-drasi-accent/10 flex items-center justify-center">
                <Package size={20} className="text-drasi-accent" />
              </div>
              <div>
                <h2 className="text-lg font-bold text-drasi-text-primary">
                  Solution Templates
                </h2>
                <p className="text-sm text-drasi-text-secondary">
                  Deploy pre-configured component sets
                </p>
              </div>
            </div>
            <button
              onClick={onClose}
              className="text-drasi-text-secondary hover:text-drasi-text-primary"
            >
              ✕
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-4">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 size={24} className="animate-spin text-drasi-accent" />
            </div>
          ) : error ? (
            <div className="text-center py-12 text-drasi-text-secondary">
              {error}
            </div>
          ) : templates.length === 0 ? (
            <div className="text-center py-12 text-drasi-text-secondary">
              <Package size={48} className="mx-auto mb-4 opacity-30" />
              <p>No solution templates found</p>
              <p className="text-sm mt-2">
                Add .yaml files to the solutions directory
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              {templates.map((t) => (
                <button
                  key={t.id}
                  onClick={() => onSelect(t.id)}
                  className="w-full text-left p-4 rounded-lg border border-drasi-border hover:border-drasi-accent hover:bg-drasi-card transition-colors group"
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <h3 className="font-medium text-drasi-text-primary group-hover:text-drasi-accent">
                        {t.name}
                      </h3>
                      {t.description && (
                        <p className="text-sm text-drasi-text-secondary mt-1">
                          {t.description}
                        </p>
                      )}
                      <div className="flex items-center gap-4 mt-2 text-xs text-drasi-text-secondary">
                        {t.version && <span>v{t.version}</span>}
                        {t.author && <span>by {t.author}</span>}
                        <span>
                          {t.sourceCount} source{t.sourceCount !== 1 ? "s" : ""} •{" "}
                          {t.queryCount} quer{t.queryCount !== 1 ? "ies" : "y"} •{" "}
                          {t.reactionCount} reaction{t.reactionCount !== 1 ? "s" : ""}
                        </span>
                      </div>
                    </div>
                    <ChevronRight
                      size={20}
                      className="text-drasi-text-secondary group-hover:text-drasi-accent"
                    />
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex-shrink-0 border-t border-drasi-border p-4 flex items-center justify-between">
          <button onClick={onClose} className="action-btn-ghost">
            Cancel
          </button>
          <label className="action-btn-secondary flex items-center gap-2 cursor-pointer">
            <Upload size={16} />
            Upload Template
            <input
              type="file"
              accept=".yaml,.yml"
              onChange={handleFileUpload}
              className="hidden"
            />
          </label>
        </div>
      </div>
    </div>
  );
}
