import { useState, useEffect } from "react";
import { Loader2, Check, AlertCircle, Package, Database, Search, Zap } from "lucide-react";
import FormField from "@/components/create/FormField";
import * as api from "@/api/client";
import type {
  SourceStatusResponse,
  QueryConfigResponse,
  ReactionStatusResponse,
} from "@/api/types";

interface CreateSolutionTemplateDialogProps {
  instanceId: string;
  sources: SourceStatusResponse[];
  queries: QueryConfigResponse[];
  reactions: ReactionStatusResponse[];
  onSuccess: (templateId: string) => void;
  onCancel: () => void;
}

type DialogState = "form" | "creating" | "success" | "error";

export default function CreateSolutionTemplateDialog({
  instanceId,
  sources,
  queries,
  reactions,
  onSuccess,
  onCancel,
}: CreateSolutionTemplateDialogProps) {
  // Metadata fields
  const [templateId, setTemplateId] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [author, setAuthor] = useState("");
  const [license, setLicense] = useState("Apache-2.0");
  
  // Selected components
  const [selectedSources, setSelectedSources] = useState<Set<string>>(new Set());
  const [selectedQueries, setSelectedQueries] = useState<Set<string>>(new Set());
  const [selectedReactions, setSelectedReactions] = useState<Set<string>>(new Set());
  
  // Dialog state
  const [dialogState, setDialogState] = useState<DialogState>("form");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [createError, setCreateError] = useState<string | null>(null);

  // Initialize selections with all components
  useEffect(() => {
    setSelectedSources(new Set(sources.map(s => s.id)));
    setSelectedQueries(new Set(queries.map(q => q.id)));
    setSelectedReactions(new Set(reactions.map(r => r.id)));
  }, [sources, queries, reactions]);

  const totalSelected = selectedSources.size + selectedQueries.size + selectedReactions.size;
  const totalAvailable = sources.length + queries.length + reactions.length;

  const toggleSource = (id: string) => {
    setSelectedSources(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const toggleQuery = (id: string) => {
    setSelectedQueries(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const toggleReaction = (id: string) => {
    setSelectedReactions(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const selectAll = () => {
    setSelectedSources(new Set(sources.map(s => s.id)));
    setSelectedQueries(new Set(queries.map(q => q.id)));
    setSelectedReactions(new Set(reactions.map(r => r.id)));
  };

  const deselectAll = () => {
    setSelectedSources(new Set());
    setSelectedQueries(new Set());
    setSelectedReactions(new Set());
  };

  const validate = (): boolean => {
    const errs: Record<string, string> = {};
    
    if (!templateId.trim()) {
      errs.id = "Template ID is required";
    } else if (!/^[a-z0-9-]+$/.test(templateId.trim())) {
      errs.id = "ID must be lowercase letters, numbers, and hyphens only";
    }
    
    if (!name.trim()) {
      errs.name = "Name is required";
    }
    
    if (totalSelected === 0) {
      errs.components = "At least one component must be selected";
    }
    
    setErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const handleCreate = async () => {
    if (!validate()) return;

    setDialogState("creating");
    setCreateError(null);

    try {
      const result = await api.createSolutionTemplate(instanceId, {
        id: templateId.trim(),
        name: name.trim(),
        description: description.trim() || undefined,
        version: version.trim() || undefined,
        author: author.trim() || undefined,
        license: license.trim() || undefined,
        sourceIds: Array.from(selectedSources),
        queryIds: Array.from(selectedQueries),
        reactionIds: Array.from(selectedReactions),
      });

      if (result.success && result.templateId) {
        setDialogState("success");
        setTimeout(() => {
          onSuccess(result.templateId!);
        }, 1500);
      } else {
        setDialogState("error");
        setCreateError(result.error || "Failed to create template");
      }
    } catch (err) {
      setDialogState("error");
      setCreateError(err instanceof Error ? err.message : "Failed to create template");
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-2xl w-full mx-4 shadow-2xl max-h-[90vh] flex flex-col">
        {dialogState === "form" && (
          <>
            <div className="flex items-center gap-3 mb-4">
              <div className="w-10 h-10 rounded-xl bg-drasi-accent/20 flex items-center justify-center">
                <Package size={20} className="text-drasi-accent" />
              </div>
              <div>
                <h2 className="text-lg font-bold text-drasi-text-primary">
                  Create Solution Template
                </h2>
                <p className="text-xs text-drasi-text-secondary">
                  Export components from <span className="font-mono">{instanceId}</span>
                </p>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto space-y-4 pr-2">
              {/* Metadata Fields */}
              <div className="grid grid-cols-2 gap-3">
                <FormField
                  label="Template ID"
                  field="id"
                  value={templateId}
                  onChange={(_, v) => {
                    setTemplateId(String(v));
                    setErrors(e => ({ ...e, id: "" }));
                  }}
                  error={errors.id}
                  required
                  placeholder="my-solution"
                />
                <FormField
                  label="Name"
                  field="name"
                  value={name}
                  onChange={(_, v) => {
                    setName(String(v));
                    setErrors(e => ({ ...e, name: "" }));
                  }}
                  error={errors.name}
                  required
                  placeholder="My Solution Template"
                />
              </div>

              <FormField
                label="Description"
                field="description"
                value={description}
                onChange={(_, v) => setDescription(String(v))}
                placeholder="A brief description of the solution"
              />

              <div className="grid grid-cols-3 gap-3">
                <FormField
                  label="Version"
                  field="version"
                  value={version}
                  onChange={(_, v) => setVersion(String(v))}
                  placeholder="1.0.0"
                />
                <FormField
                  label="Author"
                  field="author"
                  value={author}
                  onChange={(_, v) => setAuthor(String(v))}
                  placeholder="Your name"
                />
                <FormField
                  label="License"
                  field="license"
                  value={license}
                  onChange={(_, v) => setLicense(String(v))}
                  placeholder="Apache-2.0"
                />
              </div>

              {/* Component Selection */}
              <div className="border-t border-drasi-border pt-4">
                <div className="flex items-center justify-between mb-3">
                  <div>
                    <h3 className="text-sm font-medium text-drasi-text-primary">
                      Components to Include
                    </h3>
                    <p className="text-xs text-drasi-text-secondary">
                      {totalSelected} of {totalAvailable} selected
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <button
                      onClick={selectAll}
                      className="text-xs text-drasi-accent hover:text-drasi-accent/80 transition-colors"
                    >
                      Select All
                    </button>
                    <span className="text-drasi-text-secondary">|</span>
                    <button
                      onClick={deselectAll}
                      className="text-xs text-drasi-text-secondary hover:text-drasi-text-primary transition-colors"
                    >
                      Deselect All
                    </button>
                  </div>
                </div>

                {errors.components && (
                  <p className="text-xs text-drasi-error mb-3">{errors.components}</p>
                )}

                {/* Sources */}
                {sources.length > 0 && (
                  <div className="mb-3">
                    <div className="flex items-center gap-2 mb-2">
                      <Database size={14} className="text-drasi-source" />
                      <span className="text-xs font-medium text-drasi-text-primary">
                        Sources ({selectedSources.size}/{sources.length})
                      </span>
                    </div>
                    <div className="grid grid-cols-2 gap-1.5">
                      {sources.map(s => (
                        <label
                          key={s.id}
                          className="flex items-center gap-2 px-2 py-1.5 rounded-md bg-drasi-card border border-drasi-border cursor-pointer hover:border-drasi-source/50 transition-colors"
                        >
                          <input
                            type="checkbox"
                            checked={selectedSources.has(s.id)}
                            onChange={() => toggleSource(s.id)}
                            className="w-3.5 h-3.5 rounded border-drasi-border text-drasi-source focus:ring-drasi-source"
                          />
                          <span className="text-xs text-drasi-text-primary truncate font-mono">
                            {s.id}
                          </span>
                          <span className="text-[10px] text-drasi-text-secondary ml-auto">
                            {s.kind}
                          </span>
                        </label>
                      ))}
                    </div>
                  </div>
                )}

                {/* Queries */}
                {queries.length > 0 && (
                  <div className="mb-3">
                    <div className="flex items-center gap-2 mb-2">
                      <Search size={14} className="text-drasi-query" />
                      <span className="text-xs font-medium text-drasi-text-primary">
                        Queries ({selectedQueries.size}/{queries.length})
                      </span>
                    </div>
                    <div className="grid grid-cols-2 gap-1.5">
                      {queries.map(q => (
                        <label
                          key={q.id}
                          className="flex items-center gap-2 px-2 py-1.5 rounded-md bg-drasi-card border border-drasi-border cursor-pointer hover:border-drasi-query/50 transition-colors"
                        >
                          <input
                            type="checkbox"
                            checked={selectedQueries.has(q.id)}
                            onChange={() => toggleQuery(q.id)}
                            className="w-3.5 h-3.5 rounded border-drasi-border text-drasi-query focus:ring-drasi-query"
                          />
                          <span className="text-xs text-drasi-text-primary truncate font-mono">
                            {q.id}
                          </span>
                          <span className="text-[10px] text-drasi-text-secondary ml-auto">
                            {q.queryLanguage ?? "Cypher"}
                          </span>
                        </label>
                      ))}
                    </div>
                  </div>
                )}

                {/* Reactions */}
                {reactions.length > 0 && (
                  <div className="mb-3">
                    <div className="flex items-center gap-2 mb-2">
                      <Zap size={14} className="text-drasi-reaction" />
                      <span className="text-xs font-medium text-drasi-text-primary">
                        Reactions ({selectedReactions.size}/{reactions.length})
                      </span>
                    </div>
                    <div className="grid grid-cols-2 gap-1.5">
                      {reactions.map(r => (
                        <label
                          key={r.id}
                          className="flex items-center gap-2 px-2 py-1.5 rounded-md bg-drasi-card border border-drasi-border cursor-pointer hover:border-drasi-reaction/50 transition-colors"
                        >
                          <input
                            type="checkbox"
                            checked={selectedReactions.has(r.id)}
                            onChange={() => toggleReaction(r.id)}
                            className="w-3.5 h-3.5 rounded border-drasi-border text-drasi-reaction focus:ring-drasi-reaction"
                          />
                          <span className="text-xs text-drasi-text-primary truncate font-mono">
                            {r.id}
                          </span>
                          <span className="text-[10px] text-drasi-text-secondary ml-auto">
                            {r.kind}
                          </span>
                        </label>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>

            <div className="flex justify-end gap-2 mt-4 pt-4 border-t border-drasi-border">
              <button onClick={onCancel} className="action-btn-ghost">
                Cancel
              </button>
              <button onClick={handleCreate} className="action-btn-primary">
                Create Template
              </button>
            </div>
          </>
        )}

        {dialogState === "creating" && (
          <div className="text-center py-8">
            <Loader2 size={40} className="animate-spin text-drasi-accent mx-auto mb-4" />
            <p className="text-sm font-medium text-drasi-text-primary">
              Creating solution template...
            </p>
          </div>
        )}

        {dialogState === "success" && (
          <div className="text-center py-8">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-drasi-running/10 flex items-center justify-center">
              <Check size={32} className="text-drasi-running" />
            </div>
            <p className="text-lg font-medium text-drasi-text-primary">
              Template Created!
            </p>
            <p className="text-sm text-drasi-text-secondary mt-1">
              <span className="font-mono">{templateId}</span>
            </p>
          </div>
        )}

        {dialogState === "error" && (
          <div className="text-center py-8">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-drasi-error/10 flex items-center justify-center">
              <AlertCircle size={32} className="text-drasi-error" />
            </div>
            <p className="text-lg font-medium text-drasi-text-primary mb-2">
              Creation Failed
            </p>
            <p className="text-sm text-drasi-error mb-4">
              {createError}
            </p>
            <div className="flex justify-center gap-2">
              <button onClick={onCancel} className="action-btn-ghost">
                Close
              </button>
              <button 
                onClick={() => {
                  setDialogState("form");
                  setCreateError(null);
                }} 
                className="action-btn-primary"
              >
                Try Again
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
