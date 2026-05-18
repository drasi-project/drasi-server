import { useState, useEffect } from "react";
import {
  Loader2,
  Check,
  AlertCircle,
  Package,
  Database,
  Search,
  Zap,
  ChevronRight,
  ChevronLeft,
  Upload,
  Sparkles,
  RefreshCw,
} from "lucide-react";
import * as api from "@/api/client";
import type {
  SolutionTemplateSummary,
  SolutionTemplateDetail,
  SolutionDeployResponse,
} from "@/api/types";

interface SolutionInstanceWizardProps {
  onClose: () => void;
  onSuccess: (instanceId: string) => void;
}

type WizardStep = "select" | "configure" | "creating" | "success" | "error";

export default function SolutionInstanceWizard({
  onClose,
  onSuccess,
}: SolutionInstanceWizardProps) {
  // Step state
  const [step, setStep] = useState<WizardStep>("select");
  
  // Template selection state
  const [templates, setTemplates] = useState<SolutionTemplateSummary[]>([]);
  const [loadingTemplates, setLoadingTemplates] = useState(true);
  const [templateError, setTemplateError] = useState<string | null>(null);
  
  // Selected template details
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null);
  const [uploadedYaml, setUploadedYaml] = useState<string | null>(null);
  const [templateDetail, setTemplateDetail] = useState<SolutionTemplateDetail | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);
  
  // Configuration state
  const [instanceId, setInstanceId] = useState("");
  const [instanceIdError, setInstanceIdError] = useState("");
  const [persistIndex, setPersistIndex] = useState(false);
  const [variables, setVariables] = useState<Record<string, string>>({});
  
  // Result state
  const [deployResult, setDeployResult] = useState<SolutionDeployResponse | null>(null);

  // Load templates on mount
  useEffect(() => {
    async function load() {
      try {
        const list = await api.listSolutions();
        setTemplates(list);
      } catch (e) {
        setTemplateError(e instanceof Error ? e.message : "Failed to load templates");
      } finally {
        setLoadingTemplates(false);
      }
    }
    load();
  }, []);

  // Load template details when selected
  useEffect(() => {
    if (!selectedTemplateId) return;
    
    async function loadDetail() {
      setLoadingDetail(true);
      try {
        const detail = await api.getSolution(selectedTemplateId!);
        setTemplateDetail(detail);
        
        // Initialize variables with defaults
        const initial: Record<string, string> = {};
        detail.variables.forEach((v) => {
          initial[v.name] = v.default ?? "";
        });
        setVariables(initial);
        
        setStep("configure");
      } catch (e) {
        setTemplateError(e instanceof Error ? e.message : "Failed to load template details");
      } finally {
        setLoadingDetail(false);
      }
    }
    loadDetail();
  }, [selectedTemplateId]);

  // Handle uploaded YAML
  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = () => {
      const content = reader.result as string;
      setUploadedYaml(content);
      
      // Extract variables from YAML
      const vars = extractVariablesFromYaml(content);
      setTemplateDetail({
        id: "uploaded",
        name: file.name.replace(/\.(yaml|yml)$/, ""),
        variables: vars,
        sourceIds: [],
        queryIds: [],
        reactionIds: [],
      });
      
      // Initialize variables with defaults
      const initial: Record<string, string> = {};
      vars.forEach((v) => {
        initial[v.name] = v.default ?? "";
      });
      setVariables(initial);
      
      setStep("configure");
    };
    reader.readAsText(file);
  };

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

  const handleCreate = async () => {
    // Validate instance ID
    if (!instanceId.trim()) {
      setInstanceIdError("Instance ID is required");
      return;
    }
    
    setInstanceIdError("");
    setStep("creating");
    
    try {
      // Step 1: Create the instance
      await api.createInstance({ 
        id: instanceId.trim(),
        persistIndex,
      });
      
      // Step 2: Deploy the template to the new instance
      const result = await api.deploySolution(instanceId.trim(), {
        templateId: selectedTemplateId ?? undefined,
        yaml: uploadedYaml ?? undefined,
        variables,
      });
      
      setDeployResult(result);
      setStep(result.success ? "success" : "error");
      
      if (result.success) {
        setTimeout(() => {
          onSuccess(instanceId.trim());
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
            phase: "creation",
            message: e instanceof Error ? e.message : "Failed to create instance",
          },
        ],
      });
      setStep("error");
    }
  };

  const missingRequired =
    templateDetail?.variables
      .filter((v) => v.required)
      .some((v) => !variables[v.name]?.trim()) ?? false;

  const totalComponents =
    (templateDetail?.sourceIds.length ?? 0) +
    (templateDetail?.queryIds.length ?? 0) +
    (templateDetail?.reactionIds.length ?? 0);

  return (
    <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in">
      <div
        className="bg-drasi-surface border border-drasi-border rounded-2xl shadow-2xl w-full max-w-xl mx-4 overflow-hidden flex flex-col"
        style={{ maxHeight: "85vh" }}
      >
        {/* Header */}
        <div className="shrink-0 p-6 pb-4 border-b border-drasi-border">
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-drasi-accent/20 to-drasi-reaction/20 flex items-center justify-center shrink-0">
              <Package size={24} className="text-drasi-accent" />
            </div>
            <div className="flex-1 min-w-0">
              <h2 className="text-xl font-bold text-drasi-text-primary">
                {step === "select" && "Create Instance from Template"}
                {step === "configure" && (templateDetail?.name ?? "Configure Instance")}
                {step === "creating" && "Creating Instance"}
                {step === "success" && "Instance Created!"}
                {step === "error" && "Creation Failed"}
              </h2>
              {step === "select" && (
                <p className="text-sm text-drasi-text-secondary mt-1">
                  Select a solution template to create a new instance
                </p>
              )}
              {step === "configure" && templateDetail?.description && (
                <p className="text-sm text-drasi-text-secondary mt-1 line-clamp-2">
                  {templateDetail.description}
                </p>
              )}
              {step === "configure" && templateDetail?.version && (
                <span className="inline-block mt-2 text-xs px-2 py-0.5 rounded-full bg-drasi-card text-drasi-text-secondary">
                  v{templateDetail.version}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          {/* Step 1: Select Template */}
          {step === "select" && (
            <div className="space-y-2">
              {loadingTemplates ? (
                <div className="flex flex-col items-center justify-center py-12">
                  <Loader2 size={24} className="animate-spin text-drasi-accent mb-3" />
                  <span className="text-sm text-drasi-text-secondary">Loading templates...</span>
                </div>
              ) : templateError ? (
                <div className="text-center py-12">
                  <Package size={40} className="mx-auto mb-3 text-drasi-error opacity-50" />
                  <p className="text-sm text-drasi-error">{templateError}</p>
                </div>
              ) : templates.length === 0 ? (
                <div className="text-center py-12">
                  <Package size={40} className="mx-auto mb-3 text-drasi-text-secondary opacity-30" />
                  <p className="text-sm text-drasi-text-secondary font-medium">No templates available</p>
                  <p className="text-xs text-drasi-text-secondary mt-1 opacity-70">
                    Add .yaml files to the solutions directory or upload one below
                  </p>
                </div>
              ) : (
                templates.map((t) => (
                  <button
                    key={t.id}
                    onClick={() => setSelectedTemplateId(t.id)}
                    disabled={loadingDetail}
                    className="w-full text-left p-4 rounded-xl border border-drasi-border hover:border-drasi-accent hover:bg-drasi-card transition-all group disabled:opacity-50"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex-1 min-w-0">
                        <h4 className="font-semibold text-drasi-text-primary group-hover:text-drasi-accent transition-colors">
                          {t.name}
                        </h4>
                        {t.description && (
                          <p className="text-sm text-drasi-text-secondary mt-1 line-clamp-2">
                            {t.description}
                          </p>
                        )}
                        <div className="flex items-center gap-4 mt-2">
                          {t.version && (
                            <span className="text-xs text-drasi-text-secondary px-2 py-0.5 rounded-full bg-drasi-card">
                              v{t.version}
                            </span>
                          )}
                          <div className="flex items-center gap-2 text-xs text-drasi-text-secondary">
                            <span className="flex items-center gap-1">
                              <Database size={12} className="text-drasi-source" />
                              {t.sourceCount}
                            </span>
                            <span className="flex items-center gap-1">
                              <Search size={12} className="text-drasi-query" />
                              {t.queryCount}
                            </span>
                            <span className="flex items-center gap-1">
                              <Zap size={12} className="text-drasi-reaction" />
                              {t.reactionCount}
                            </span>
                          </div>
                        </div>
                      </div>
                      {loadingDetail && selectedTemplateId === t.id ? (
                        <Loader2 size={18} className="animate-spin text-drasi-accent shrink-0 mt-1" />
                      ) : (
                        <ChevronRight
                          size={18}
                          className="text-drasi-text-secondary group-hover:text-drasi-accent shrink-0 mt-1 transition-colors"
                        />
                      )}
                    </div>
                  </button>
                ))
              )}

              {/* Upload option */}
              <div className="mt-4 pt-4 border-t border-drasi-border">
                <label className="flex items-center justify-center gap-2 py-3 px-4 rounded-xl border-2 border-dashed border-drasi-border hover:border-drasi-accent text-drasi-text-secondary hover:text-drasi-accent cursor-pointer transition-all text-sm">
                  <Upload size={16} />
                  Upload Custom Template
                  <input
                    type="file"
                    accept=".yaml,.yml"
                    onChange={handleFileUpload}
                    className="hidden"
                  />
                </label>
              </div>
            </div>
          )}

          {/* Step 2: Configure */}
          {step === "configure" && templateDetail && (
            <div className="space-y-5">
              {/* Components in template */}
              {totalComponents > 0 && (
                <div className="rounded-xl border border-drasi-border overflow-hidden">
                  <div className="px-4 py-3 bg-drasi-bg/50 border-b border-drasi-border">
                    <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider">
                      Components in Template
                    </h3>
                  </div>
                  <div className="p-4 space-y-3">
                    {templateDetail.sourceIds.length > 0 && (
                      <div className="flex items-start gap-3">
                        <div className="w-8 h-8 rounded-lg bg-drasi-source/10 flex items-center justify-center shrink-0">
                          <Database size={16} className="text-drasi-source" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-drasi-text-primary">
                            {templateDetail.sourceIds.length} Source
                            {templateDetail.sourceIds.length !== 1 ? "s" : ""}
                          </div>
                          <div className="text-xs text-drasi-text-secondary mt-0.5 flex flex-wrap gap-1.5">
                            {templateDetail.sourceIds.map((id) => (
                              <span key={id} className="px-2 py-0.5 rounded bg-drasi-card">
                                {id}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    )}
                    {templateDetail.queryIds.length > 0 && (
                      <div className="flex items-start gap-3">
                        <div className="w-8 h-8 rounded-lg bg-drasi-query/10 flex items-center justify-center shrink-0">
                          <Search size={16} className="text-drasi-query" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-drasi-text-primary">
                            {templateDetail.queryIds.length} Quer
                            {templateDetail.queryIds.length !== 1 ? "ies" : "y"}
                          </div>
                          <div className="text-xs text-drasi-text-secondary mt-0.5 flex flex-wrap gap-1.5">
                            {templateDetail.queryIds.map((id) => (
                              <span key={id} className="px-2 py-0.5 rounded bg-drasi-card">
                                {id}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    )}
                    {templateDetail.reactionIds.length > 0 && (
                      <div className="flex items-start gap-3">
                        <div className="w-8 h-8 rounded-lg bg-drasi-reaction/10 flex items-center justify-center shrink-0">
                          <Zap size={16} className="text-drasi-reaction" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-drasi-text-primary">
                            {templateDetail.reactionIds.length} Reaction
                            {templateDetail.reactionIds.length !== 1 ? "s" : ""}
                          </div>
                          <div className="text-xs text-drasi-text-secondary mt-0.5 flex flex-wrap gap-1.5">
                            {templateDetail.reactionIds.map((id) => (
                              <span key={id} className="px-2 py-0.5 rounded bg-drasi-card">
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

              {/* Instance Configuration */}
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-drasi-text-primary mb-2">
                    Instance ID <span className="text-drasi-error">*</span>
                  </label>
                  <input
                    type="text"
                    value={instanceId}
                    onChange={(e) => {
                      setInstanceId(e.target.value);
                      setInstanceIdError("");
                    }}
                    placeholder="my-instance"
                    className={`w-full px-4 py-3 bg-drasi-card border rounded-xl text-drasi-text-primary placeholder-drasi-text-secondary/50 focus:outline-none ${
                      instanceIdError
                        ? "border-drasi-error focus:border-drasi-error"
                        : "border-drasi-border focus:border-drasi-accent"
                    }`}
                  />
                  {instanceIdError && (
                    <p className="text-xs text-drasi-error mt-1">{instanceIdError}</p>
                  )}
                </div>

                <div className="flex items-center justify-between p-3 rounded-xl bg-drasi-card border border-drasi-border">
                  <div>
                    <div className="text-sm font-medium text-drasi-text-primary">
                      Persist Index (RocksDB)
                    </div>
                    <div className="text-xs text-drasi-text-secondary">
                      Use RocksDB for persistent query indexes
                    </div>
                  </div>
                  <button
                    onClick={() => setPersistIndex(!persistIndex)}
                    className={`relative w-11 h-6 rounded-full transition-colors ${
                      persistIndex ? "bg-drasi-accent" : "bg-drasi-border"
                    }`}
                  >
                    <span
                      className={`absolute top-1 left-1 w-4 h-4 rounded-full bg-white transition-transform ${
                        persistIndex ? "translate-x-5" : ""
                      }`}
                    />
                  </button>
                </div>
              </div>

              {/* Variables */}
              {templateDetail.variables.length > 0 && (
                <div className="rounded-xl border border-drasi-border overflow-hidden">
                  <div className="px-4 py-3 bg-drasi-bg/50 border-b border-drasi-border">
                    <h3 className="text-xs font-semibold text-drasi-text-secondary uppercase tracking-wider">
                      Configuration Variables
                    </h3>
                  </div>
                  <div className="p-4 space-y-5">
                    {templateDetail.variables.map((v) => (
                      <div key={v.name}>
                        <div className="flex items-start justify-between gap-2 mb-2">
                          <div>
                            <label className="flex items-center gap-2 text-sm font-medium text-drasi-text-primary">
                              <span className="font-mono text-drasi-accent">{v.name}</span>
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
                              <span key={id} className="px-1.5 py-0.5 rounded bg-drasi-card">
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
          )}

          {/* Step 3: Creating */}
          {step === "creating" && (
            <div className="py-12 text-center">
              <div className="relative w-16 h-16 mx-auto mb-6">
                <div className="absolute inset-0 rounded-full border-4 border-drasi-border" />
                <div className="absolute inset-0 rounded-full border-4 border-drasi-accent border-t-transparent animate-spin" />
                <Sparkles size={24} className="absolute inset-0 m-auto text-drasi-accent" />
              </div>
              <p className="text-lg font-medium text-drasi-text-primary">
                Creating Instance
              </p>
              <p className="text-sm text-drasi-text-secondary mt-1">
                Setting up {instanceId} with {totalComponents} component
                {totalComponents !== 1 ? "s" : ""}...
              </p>
            </div>
          )}

          {/* Step 4: Success */}
          {step === "success" && (
            <div className="py-12 text-center">
              <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-green-500/10 flex items-center justify-center">
                <Check size={32} className="text-green-400" />
              </div>
              <p className="text-lg font-medium text-drasi-text-primary">
                Instance Created!
              </p>
              <p className="text-sm text-drasi-text-secondary mt-2">
                <span className="font-mono text-drasi-accent">{instanceId}</span>
              </p>
              {deployResult && (
                <div className="mt-4 flex items-center justify-center gap-4 text-sm text-drasi-text-secondary">
                  {deployResult.sourcesCreated.length > 0 && (
                    <span className="flex items-center gap-1.5">
                      <Database size={14} className="text-drasi-source" />
                      {deployResult.sourcesCreated.length}
                    </span>
                  )}
                  {deployResult.queriesCreated.length > 0 && (
                    <span className="flex items-center gap-1.5">
                      <Search size={14} className="text-drasi-query" />
                      {deployResult.queriesCreated.length}
                    </span>
                  )}
                  {deployResult.reactionsCreated.length > 0 && (
                    <span className="flex items-center gap-1.5">
                      <Zap size={14} className="text-drasi-reaction" />
                      {deployResult.reactionsCreated.length}
                    </span>
                  )}
                </div>
              )}
            </div>
          )}

          {/* Step 5: Error */}
          {step === "error" && deployResult && (
            <div className="py-6">
              <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-red-500/10 flex items-center justify-center">
                <AlertCircle size={32} className="text-red-400" />
              </div>
              <p className="text-center text-lg font-medium text-drasi-text-primary mb-6">
                Creation Failed
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
                    <p className="text-sm text-drasi-text-primary">{e.message}</p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="shrink-0 border-t border-drasi-border p-4 flex items-center justify-between bg-drasi-bg/30">
          {step === "select" && (
            <>
              <button
                onClick={onClose}
                className="px-4 py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
              >
                Cancel
              </button>
              <div />
            </>
          )}

          {step === "configure" && (
            <>
              <button
                onClick={() => {
                  setStep("select");
                  setSelectedTemplateId(null);
                  setUploadedYaml(null);
                  setTemplateDetail(null);
                  setVariables({});
                }}
                className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
              >
                <ChevronLeft size={16} />
                Back
              </button>
              <button
                onClick={handleCreate}
                disabled={!instanceId.trim() || missingRequired}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-drasi-accent text-white font-medium text-sm hover:bg-drasi-accent/90 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
              >
                <Sparkles size={16} />
                Create Instance
              </button>
            </>
          )}

          {step === "creating" && (
            <div className="w-full text-center text-sm text-drasi-text-secondary">
              Please wait...
            </div>
          )}

          {step === "success" && (
            <button
              onClick={() => onSuccess(instanceId)}
              className="w-full py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
            >
              Close
            </button>
          )}

          {step === "error" && (
            <>
              <button
                onClick={onClose}
                className="px-4 py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
              >
                Close
              </button>
              <button
                onClick={() => setStep("configure")}
                className="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-drasi-card border border-drasi-border text-drasi-text-primary text-sm hover:bg-drasi-surface transition-colors"
              >
                <RefreshCw size={14} />
                Try Again
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
