import { useState, useEffect } from "react";
import {
  Loader2,
  Check,
  AlertCircle,
  Package,
  Database,
  Search,
  Zap,
  ChevronDown,
  Sparkles,
  RefreshCw,
  Plus,
} from "lucide-react";
import * as api from "@/api/client";
import type {
  SolutionTemplateDetail,
  SolutionDeployResponse,
  InstanceInfo,
} from "@/api/types";

interface SolutionDeployDialogProps {
  templateId?: string;
  uploadedYaml?: string;
  onClose: () => void;
  onSuccess: (deployedToInstanceId: string) => void;
}

type DeployState = "form" | "deploying" | "success" | "error";

export default function SolutionDeployDialog({
  templateId,
  uploadedYaml,
  onClose,
  onSuccess,
}: SolutionDeployDialogProps) {
  const [template, setTemplate] = useState<SolutionTemplateDetail | null>(null);
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [selectedInstance, setSelectedInstance] = useState<string>("");
  const [showNewInstance, setShowNewInstance] = useState(false);
  const [newInstanceId, setNewInstanceId] = useState("");
  const [creatingInstance, setCreatingInstance] = useState(false);
  const [variables, setVariables] = useState<Record<string, string>>({});
  const [deployState, setDeployState] = useState<DeployState>("form");
  const [deployResult, setDeployResult] =
    useState<SolutionDeployResponse | null>(null);

  useEffect(() => {
    async function load() {
      try {
        // Load instances
        const instanceList = await api.listInstances();
        setInstances(instanceList);
        if (instanceList.length > 0) {
          setSelectedInstance(instanceList[0].id);
        }

        // Load template details if templateId provided
        if (templateId) {
          const detail = await api.getSolution(templateId);
          setTemplate(detail);

          // Initialize variables with defaults
          const initial: Record<string, string> = {};
          detail.variables.forEach((v) => {
            initial[v.name] = v.default ?? "";
          });
          setVariables(initial);
        } else if (uploadedYaml) {
          // For uploaded YAML, we need to extract variables client-side
          // For now, just use an empty template representation
          setTemplate({
            id: "uploaded",
            name: "Uploaded Template",
            variables: extractVariablesFromYaml(uploadedYaml),
            sourceIds: [],
            queryIds: [],
            reactionIds: [],
          });
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, [templateId, uploadedYaml]);

  // Simple client-side variable extraction
  function extractVariablesFromYaml(yaml: string) {
    const re = /\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}/g;
    const seen = new Set<string>();
    const vars: { name: string; default?: string; required: boolean }[] = [];

    let match;
    while ((match = re.exec(yaml)) !== null) {
      const name = match[1];
      if (seen.has(name)) continue;
      seen.add(name);
      vars.push({
        name,
        default: match[2],
        required: !match[2],
      });
    }
    return vars;
  }

  const handleDeploy = async () => {
    if (!selectedInstance) return;

    setDeployState("deploying");
    try {
      const result = await api.deploySolution(selectedInstance, {
        templateId: templateId,
        yaml: uploadedYaml,
        variables,
      });

      setDeployResult(result);
      setDeployState(result.success ? "success" : "error");

      if (result.success) {
        setTimeout(() => {
          onSuccess(selectedInstance);
        }, 2000);
      }
    } catch (e) {
      setDeployResult({
        success: false,
        sourcesCreated: [],
        queriesCreated: [],
        reactionsCreated: [],
        componentsStarted: [],
        errors: [
          {
            phase: "validation",
            message: e instanceof Error ? e.message : "Deployment failed",
          },
        ],
      });
      setDeployState("error");
    }
  };

  const handleCreateInstance = async () => {
    if (!newInstanceId.trim()) return;

    setCreatingInstance(true);
    try {
      await api.createInstance({ id: newInstanceId.trim() });
      // Refresh instances list
      const instanceList = await api.listInstances();
      setInstances(instanceList);
      setSelectedInstance(newInstanceId.trim());
      setShowNewInstance(false);
      setNewInstanceId("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create instance");
    } finally {
      setCreatingInstance(false);
    }
  };

  const missingRequired =
    template?.variables
      .filter((v) => v.required)
      .some((v) => !variables[v.name]?.trim()) ?? false;

  if (loading) {
    return (
      <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in">
        <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-8 shadow-2xl">
          <Loader2
            size={32}
            className="animate-spin text-drasi-accent mx-auto"
          />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in">
        <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-8 max-w-md shadow-2xl">
          <AlertCircle size={32} className="text-drasi-error mx-auto mb-4" />
          <p className="text-center text-drasi-text-primary">{error}</p>
          <button
            onClick={onClose}
            className="w-full mt-6 py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    );
  }

  const totalComponents =
    (template?.sourceIds.length ?? 0) +
    (template?.queryIds.length ?? 0) +
    (template?.reactionIds.length ?? 0);

  return (
    <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in">
      <div
        className="bg-drasi-surface border border-drasi-border rounded-2xl shadow-2xl w-full max-w-lg mx-4 overflow-hidden flex flex-col"
        style={{ maxHeight: "85vh" }}
      >
        {/* Header */}
        <div className="shrink-0 p-6 pb-4">
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-drasi-accent/20 to-purple-500/20 flex items-center justify-center shrink-0">
              <Package size={24} className="text-drasi-accent" />
            </div>
            <div className="flex-1 min-w-0">
              <h2 className="text-xl font-bold text-drasi-text-primary">
                {template?.name ?? "Deploy Solution"}
              </h2>
              {template?.description && (
                <p className="text-sm text-drasi-text-secondary mt-1 line-clamp-2">
                  {template.description}
                </p>
              )}
              {template?.version && (
                <span className="inline-block mt-2 text-xs px-2 py-0.5 rounded-full bg-drasi-card text-drasi-text-secondary">
                  v{template.version}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-5">
          {deployState === "form" && (
            <>
              {/* What will be created - Visual summary */}
              {template && totalComponents > 0 && (
                <div className="rounded-xl border border-drasi-border overflow-hidden">
                  <div className="px-4 py-3 bg-drasi-bg/50 border-b border-drasi-border">
                    <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider">
                      Components in Solution Template
                    </h3>
                  </div>
                  <div className="p-4 space-y-3">
                    {template.sourceIds.length > 0 && (
                      <div className="flex items-start gap-3">
                        <div className="w-8 h-8 rounded-lg bg-blue-500/10 flex items-center justify-center shrink-0">
                          <Database size={16} className="text-blue-400" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-drasi-text-primary">
                            {template.sourceIds.length} Source
                            {template.sourceIds.length !== 1 ? "s" : ""}
                          </div>
                          <div className="text-xs text-drasi-text-secondary mt-0.5 flex flex-wrap gap-1.5">
                            {template.sourceIds.map((id) => (
                              <span
                                key={id}
                                className="px-2 py-0.5 rounded bg-drasi-card"
                              >
                                {id}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    )}
                    {template.queryIds.length > 0 && (
                      <div className="flex items-start gap-3">
                        <div className="w-8 h-8 rounded-lg bg-purple-500/10 flex items-center justify-center shrink-0">
                          <Search size={16} className="text-purple-400" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-drasi-text-primary">
                            {template.queryIds.length} Quer
                            {template.queryIds.length !== 1 ? "ies" : "y"}
                          </div>
                          <div className="text-xs text-drasi-text-secondary mt-0.5 flex flex-wrap gap-1.5">
                            {template.queryIds.map((id) => (
                              <span
                                key={id}
                                className="px-2 py-0.5 rounded bg-drasi-card"
                              >
                                {id}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    )}
                    {template.reactionIds.length > 0 && (
                      <div className="flex items-start gap-3">
                        <div className="w-8 h-8 rounded-lg bg-cyan-500/10 flex items-center justify-center shrink-0">
                          <Zap size={16} className="text-cyan-400" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-drasi-text-primary">
                            {template.reactionIds.length} Reaction
                            {template.reactionIds.length !== 1 ? "s" : ""}
                          </div>
                          <div className="text-xs text-drasi-text-secondary mt-0.5 flex flex-wrap gap-1.5">
                            {template.reactionIds.map((id) => (
                              <span
                                key={id}
                                className="px-2 py-0.5 rounded bg-drasi-card"
                              >
                                {id}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* Configuration Section */}
              <div className="space-y-4">
                {/* Target Instance */}
                <div>
                  <label className="block text-sm font-medium text-drasi-text-primary mb-2">
                    Deploy to Instance
                  </label>
                  {showNewInstance ? (
                    <div className="space-y-2">
                      <input
                        type="text"
                        value={newInstanceId}
                        onChange={(e) => setNewInstanceId(e.target.value)}
                        placeholder="Enter new instance ID..."
                        className="w-full px-4 py-3 bg-drasi-card border border-drasi-border rounded-xl text-drasi-text-primary placeholder-drasi-text-secondary/50 focus:border-drasi-accent focus:outline-none"
                        autoFocus
                      />
                      <div className="flex gap-2">
                        <button
                          onClick={handleCreateInstance}
                          disabled={!newInstanceId.trim() || creatingInstance}
                          className="flex-1 flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-drasi-accent text-white text-sm font-medium disabled:opacity-40 disabled:cursor-not-allowed"
                        >
                          {creatingInstance ? (
                            <Loader2 size={14} className="animate-spin" />
                          ) : (
                            <Plus size={14} />
                          )}
                          Create
                        </button>
                        <button
                          onClick={() => {
                            setShowNewInstance(false);
                            setNewInstanceId("");
                          }}
                          className="px-4 py-2 rounded-lg text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex gap-2">
                      <div className="relative flex-1">
                        <select
                          value={selectedInstance}
                          onChange={(e) => setSelectedInstance(e.target.value)}
                          className="w-full px-4 py-3 bg-drasi-card border border-drasi-border rounded-xl text-drasi-text-primary focus:border-drasi-accent focus:outline-none appearance-none cursor-pointer"
                        >
                          {instances.map((i) => (
                            <option key={i.id} value={i.id}>
                              {i.id}
                            </option>
                          ))}
                        </select>
                        <ChevronDown
                          size={16}
                          className="absolute right-4 top-1/2 -translate-y-1/2 text-drasi-text-secondary pointer-events-none"
                        />
                      </div>
                      <button
                        onClick={() => setShowNewInstance(true)}
                        className="px-3 py-3 rounded-xl border border-drasi-border text-drasi-text-secondary hover:text-drasi-accent hover:border-drasi-accent transition-colors"
                        title="Create new instance"
                      >
                        <Plus size={16} />
                      </button>
                    </div>
                  )}
                </div>

                {/* Variables */}
                {template?.variables && template.variables.length > 0 && (
                  <div className="rounded-xl border border-drasi-border overflow-hidden">
                    <div className="px-4 py-3 bg-drasi-bg/50 border-b border-drasi-border">
                      <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider">
                        Configuration Variables
                      </h3>
                    </div>
                    <div className="p-4 space-y-5">
                      {template.variables.map((v) => (
                        <div key={v.name}>
                          <div className="flex items-start justify-between gap-2 mb-2">
                            <div>
                              <label className="flex items-center gap-2 text-sm font-medium text-drasi-text-primary">
                                <span className="font-mono text-drasi-accent">
                                  {v.name}
                                </span>
                                {v.required ? (
                                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-400 font-normal">
                                    Required
                                  </span>
                                ) : (
                                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-drasi-card text-drasi-text-secondary font-normal">
                                    Optional
                                  </span>
                                )}
                              </label>
                              {v.description && (
                                <p className="text-xs text-drasi-text-secondary mt-1">
                                  {v.description}
                                </p>
                              )}
                            </div>
                          </div>
                          {v.usedBy && v.usedBy.length > 0 && (
                            <div className="flex items-center gap-1.5 mb-2 text-[10px] text-drasi-text-secondary">
                              <span>Used by:</span>
                              {v.usedBy.map((id) => (
                                <span
                                  key={id}
                                  className="px-1.5 py-0.5 rounded bg-drasi-card"
                                >
                                  {id}
                                </span>
                              ))}
                            </div>
                          )}
                          <input
                            type="text"
                            value={variables[v.name] ?? ""}
                            onChange={(e) =>
                              setVariables((prev) => ({
                                ...prev,
                                [v.name]: e.target.value,
                              }))
                            }
                            placeholder={v.default ? `Default: ${v.default}` : "Enter value..."}
                            className="w-full px-4 py-3 bg-drasi-card border border-drasi-border rounded-xl text-drasi-text-primary placeholder-drasi-text-secondary/50 focus:border-drasi-accent focus:outline-none font-mono text-sm"
                          />
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </>
          )}

          {deployState === "deploying" && (
            <div className="py-12 text-center">
              <div className="relative w-16 h-16 mx-auto mb-6">
                <div className="absolute inset-0 rounded-full border-4 border-drasi-border" />
                <div className="absolute inset-0 rounded-full border-4 border-drasi-accent border-t-transparent animate-spin" />
                <Sparkles
                  size={24}
                  className="absolute inset-0 m-auto text-drasi-accent"
                />
              </div>
              <p className="text-lg font-medium text-drasi-text-primary">
                Deploying Solution
              </p>
              <p className="text-sm text-drasi-text-secondary mt-1">
                Creating {totalComponents} component
                {totalComponents !== 1 ? "s" : ""}...
              </p>
            </div>
          )}

          {deployState === "success" && (
            <div className="py-12 text-center">
              <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-green-500/10 flex items-center justify-center">
                <Check size={32} className="text-green-400" />
              </div>
              <p className="text-lg font-medium text-drasi-text-primary">
                Solution Deployed!
              </p>
              {deployResult && (
                <div className="mt-4 flex items-center justify-center gap-4 text-sm text-drasi-text-secondary">
                  {deployResult.sourcesCreated.length > 0 && (
                    <span className="flex items-center gap-1.5">
                      <Database size={14} className="text-blue-400" />
                      {deployResult.sourcesCreated.length}
                    </span>
                  )}
                  {deployResult.queriesCreated.length > 0 && (
                    <span className="flex items-center gap-1.5">
                      <Search size={14} className="text-purple-400" />
                      {deployResult.queriesCreated.length}
                    </span>
                  )}
                  {deployResult.reactionsCreated.length > 0 && (
                    <span className="flex items-center gap-1.5">
                      <Zap size={14} className="text-cyan-400" />
                      {deployResult.reactionsCreated.length}
                    </span>
                  )}
                </div>
              )}
            </div>
          )}

          {deployState === "error" && deployResult && (
            <div className="py-6">
              <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-red-500/10 flex items-center justify-center">
                <AlertCircle size={32} className="text-red-400" />
              </div>
              <p className="text-center text-lg font-medium text-drasi-text-primary mb-6">
                Deployment Failed
              </p>
              <div className="space-y-3">
                {deployResult.errors.map((e, i) => (
                  <div
                    key={i}
                    className="p-4 bg-red-500/5 border border-red-500/20 rounded-xl"
                  >
                    <div className="flex items-center gap-2 mb-2">
                      <span className="text-xs px-2 py-0.5 rounded bg-red-500/20 text-red-400 capitalize">
                        {e.phase}
                      </span>
                      {e.componentType && e.componentId && (
                        <span className="text-xs text-drasi-text-secondary">
                          {e.componentType}: {e.componentId}
                        </span>
                      )}
                    </div>
                    <p className="text-sm text-drasi-text-primary">
                      {e.message}
                    </p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="shrink-0 border-t border-drasi-border p-4 flex items-center justify-between bg-drasi-bg/30">
          <button
            onClick={onClose}
            className="px-4 py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
          >
            {deployState === "success" ? "Close" : "Cancel"}
          </button>
          {deployState === "form" && (
            <button
              onClick={handleDeploy}
              disabled={!selectedInstance || missingRequired}
              className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-drasi-accent text-white font-medium text-sm hover:bg-drasi-accent/90 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              <Sparkles size={16} />
              Deploy Solution
            </button>
          )}
          {deployState === "error" && (
            <button
              onClick={() => setDeployState("form")}
              className="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-drasi-card border border-drasi-border text-drasi-text-primary text-sm hover:bg-drasi-surface transition-colors"
            >
              <RefreshCw size={14} />
              Try Again
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
