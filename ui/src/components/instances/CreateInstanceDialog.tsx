import { useState, useEffect } from "react";
import { Package, ChevronDown, Loader2 } from "lucide-react";
import FormField from "@/components/create/FormField";
import * as api from "@/api/client";
import type { SolutionTemplateSummary } from "@/api/types";

interface CreateInstanceDialogProps {
  onSave: (data: {
    id: string;
    persistIndex?: boolean;
    defaultPriorityQueueCapacity?: number;
    defaultDispatchBufferCapacity?: number;
    solutionTemplateId?: string;
  }) => Promise<void>;
  onCancel: () => void;
  /** Pre-fill the instance ID field (e.g. from a URL param that wasn't found) */
  initialId?: string;
}

export default function CreateInstanceDialog({
  onSave,
  onCancel,
  initialId,
}: CreateInstanceDialogProps) {
  const [id, setId] = useState(initialId ?? "");
  const [persistIndex, setPersistIndex] = useState(false);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  
  // Solution template selection
  const [templates, setTemplates] = useState<SolutionTemplateSummary[]>([]);
  const [loadingTemplates, setLoadingTemplates] = useState(true);
  const [selectedTemplate, setSelectedTemplate] = useState<string>("");
  const [templateDropdownOpen, setTemplateDropdownOpen] = useState(false);

  // Load available solution templates
  useEffect(() => {
    async function loadTemplates() {
      try {
        const list = await api.listSolutions();
        setTemplates(list);
      } catch {
        // Silently fail - templates are optional
      } finally {
        setLoadingTemplates(false);
      }
    }
    loadTemplates();
  }, []);

  const selectedTemplateInfo = templates.find((t) => t.id === selectedTemplate);

  const handleSave = async () => {
    if (!id.trim()) {
      setError("Required");
      return;
    }
    setSaving(true);
    try {
      await onSave({ 
        id: id.trim(), 
        persistIndex,
        solutionTemplateId: selectedTemplate || undefined,
      });
    } catch {
      setError("Failed to create instance");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-md w-full mx-4 shadow-2xl">
        <h2 className="text-lg font-bold text-drasi-text-primary mb-4">
          Create Instance
        </h2>
        <div className="space-y-4">
          <FormField
            label="Instance ID"
            field="id"
            value={id}
            onChange={(_, v) => {
              setId(String(v));
              setError("");
            }}
            error={error}
            required
            placeholder="my-instance"
          />
          <FormField
            label="Persist Index (RocksDB)"
            field="persistIndex"
            value={persistIndex}
            onChange={(_, v) => setPersistIndex(Boolean(v))}
            type="toggle"
            helpText="Use RocksDB for persistent query indexes"
          />

          {/* Solution Template Selector */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-drasi-text-secondary">
              Solution Template (Optional)
            </label>
            <div className="relative">
              <button
                type="button"
                onClick={() => setTemplateDropdownOpen(!templateDropdownOpen)}
                disabled={loadingTemplates}
                className="w-full flex items-center justify-between px-3 py-2 rounded-lg border border-drasi-border bg-drasi-card text-sm text-drasi-text-primary hover:border-drasi-text-secondary transition-colors disabled:opacity-50"
              >
                <span className="flex items-center gap-2">
                  {loadingTemplates ? (
                    <>
                      <Loader2 size={14} className="animate-spin text-drasi-text-secondary" />
                      <span className="text-drasi-text-secondary">Loading templates...</span>
                    </>
                  ) : selectedTemplateInfo ? (
                    <>
                      <Package size={14} className="text-drasi-accent" />
                      <span>{selectedTemplateInfo.name}</span>
                    </>
                  ) : (
                    <span className="text-drasi-text-secondary">None - empty instance</span>
                  )}
                </span>
                <ChevronDown
                  size={14}
                  className={`text-drasi-text-secondary transition-transform ${templateDropdownOpen ? "rotate-180" : ""}`}
                />
              </button>

              {templateDropdownOpen && !loadingTemplates && (
                <>
                  <div
                    className="fixed inset-0 z-40"
                    onClick={() => setTemplateDropdownOpen(false)}
                  />
                  <div className="absolute top-full left-0 right-0 mt-1 bg-drasi-surface border border-drasi-border rounded-lg shadow-xl z-50 overflow-hidden">
                    <div className="max-h-48 overflow-y-auto">
                      {/* None option */}
                      <button
                        onClick={() => {
                          setSelectedTemplate("");
                          setTemplateDropdownOpen(false);
                        }}
                        className={`w-full flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors ${
                          !selectedTemplate
                            ? "bg-drasi-card text-drasi-text-primary"
                            : "text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary"
                        }`}
                      >
                        <span className="text-drasi-text-secondary">None - empty instance</span>
                      </button>
                      
                      {templates.map((t) => (
                        <button
                          key={t.id}
                          onClick={() => {
                            setSelectedTemplate(t.id);
                            setTemplateDropdownOpen(false);
                          }}
                          className={`w-full flex items-center justify-between px-3 py-2 text-left text-sm transition-colors ${
                            selectedTemplate === t.id
                              ? "bg-drasi-card text-drasi-text-primary"
                              : "text-drasi-text-secondary hover:bg-drasi-card hover:text-drasi-text-primary"
                          }`}
                        >
                          <span className="flex items-center gap-2">
                            <Package size={14} className="text-drasi-accent" />
                            <span className="text-drasi-text-primary">{t.name}</span>
                          </span>
                          <span className="text-[10px] text-drasi-text-secondary">
                            {t.sourceCount}S {t.queryCount}Q {t.reactionCount}R
                          </span>
                        </button>
                      ))}
                      
                      {templates.length === 0 && (
                        <div className="px-3 py-2 text-sm text-drasi-text-secondary text-center">
                          No solution templates available
                        </div>
                      )}
                    </div>
                  </div>
                </>
              )}
            </div>
            <p className="text-[10px] text-drasi-text-secondary">
              Optionally load a solution template into the new instance
            </p>
          </div>
        </div>

        <div className="flex justify-end gap-2 mt-6">
          <button onClick={onCancel} className="action-btn-ghost" disabled={saving}>
            Cancel
          </button>
          <button onClick={handleSave} className="action-btn-primary" disabled={saving}>
            {saving ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
