import { useState, useEffect } from "react";
import {
  Database,
  Globe,
  Radio,
  FlaskConical,
  Server,
  Search,
  FileText,
  Rss,
  Gauge,
  Zap,
  Package,
  ChevronRight,
  Upload,
  Loader2,
  Layers,
  Sparkles,
} from "lucide-react";
import * as api from "@/api/client";
import type { SolutionTemplateSummary } from "@/api/types";

export type SelectableType =
  | "source"
  | "query"
  | "reaction"
  | "postgres"
  | "http"
  | "grpc"
  | "mock"
  | "platform"
  | "log"
  | "http-reaction"
  | "http-adaptive"
  | "grpc-reaction"
  | "grpc-adaptive"
  | "sse"
  | "platform-reaction"
  | "profiler";

interface TypeOption {
  id: SelectableType;
  label: string;
  description: string;
  icon: React.ElementType;
  color: string;
}

const COMPONENT_TYPES: TypeOption[] = [
  {
    id: "source",
    label: "Source",
    description: "Ingest data from databases, APIs, or streams",
    icon: Database,
    color: "#22c55e",
  },
  {
    id: "query",
    label: "Query",
    description: "Transform data with continuous Cypher queries",
    icon: Search,
    color: "#3b82f6",
  },
  {
    id: "reaction",
    label: "Reaction",
    description: "Trigger actions when query results change",
    icon: Zap,
    color: "#8b5cf6",
  },
];

const SOURCE_KINDS: TypeOption[] = [
  {
    id: "postgres",
    label: "PostgreSQL",
    description: "CDC via logical replication",
    icon: Database,
    color: "#22c55e",
  },
  {
    id: "http",
    label: "HTTP",
    description: "Receive events via webhooks",
    icon: Globe,
    color: "#22c55e",
  },
  {
    id: "grpc",
    label: "gRPC",
    description: "High-perf binary streaming",
    icon: Radio,
    color: "#22c55e",
  },
  {
    id: "mock",
    label: "Mock",
    description: "Generate test data",
    icon: FlaskConical,
    color: "#22c55e",
  },
  {
    id: "platform",
    label: "Platform",
    description: "Redis Streams integration",
    icon: Server,
    color: "#22c55e",
  },
];

const REACTION_KINDS: TypeOption[] = [
  {
    id: "log",
    label: "Log",
    description: "Output to console",
    icon: FileText,
    color: "#8b5cf6",
  },
  {
    id: "http-reaction",
    label: "HTTP",
    description: "Send webhooks",
    icon: Globe,
    color: "#8b5cf6",
  },
  {
    id: "http-adaptive",
    label: "HTTP Adaptive",
    description: "Dynamic batch webhooks",
    icon: Globe,
    color: "#8b5cf6",
  },
  {
    id: "grpc-reaction",
    label: "gRPC",
    description: "Stream via gRPC",
    icon: Radio,
    color: "#8b5cf6",
  },
  {
    id: "sse",
    label: "SSE",
    description: "Server-Sent Events",
    icon: Rss,
    color: "#8b5cf6",
  },
  {
    id: "profiler",
    label: "Profiler",
    description: "Performance metrics",
    icon: Gauge,
    color: "#8b5cf6",
  },
];

type Tab = "components" | "catalog";

interface TypeSelectorProps {
  level: "component" | "source-kind" | "reaction-kind";
  onSelect: (type: SelectableType) => void;
  onSelectSolution: (templateId: string) => void;
  onUploadSolution: (yaml: string) => void;
  onCancel: () => void;
}

export default function TypeSelector({
  level,
  onSelect,
  onSelectSolution,
  onUploadSolution,
  onCancel,
}: TypeSelectorProps) {
  const [activeTab, setActiveTab] = useState<Tab>("components");
  const [templates, setTemplates] = useState<SolutionTemplateSummary[]>([]);
  const [loadingTemplates, setLoadingTemplates] = useState(false);
  const [templateError, setTemplateError] = useState<string | null>(null);

  // Load solution templates when switching to catalog tab
  useEffect(() => {
    if (level === "component" && activeTab === "catalog" && templates.length === 0 && !loadingTemplates) {
      setLoadingTemplates(true);
      api.listSolutions()
        .then(setTemplates)
        .catch((e) => setTemplateError(e instanceof Error ? e.message : "Failed to load"))
        .finally(() => setLoadingTemplates(false));
    }
  }, [level, activeTab, templates.length, loadingTemplates]);

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = () => {
      const content = reader.result as string;
      onUploadSolution(content);
    };
    reader.readAsText(file);
  };

  // For source/reaction kind selection, show the simple grid
  if (level !== "component") {
    const options = level === "source-kind" ? SOURCE_KINDS : REACTION_KINDS;
    const title = level === "source-kind" ? "Select Source Type" : "Select Reaction Type";

    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
        <div className="bg-drasi-surface border border-drasi-border rounded-2xl p-6 max-w-lg w-full mx-4 shadow-2xl">
          <h2 className="text-lg font-bold text-drasi-text-primary mb-4 text-center">
            {title}
          </h2>
          <div className="grid grid-cols-3 gap-3">
            {options.map((opt) => {
              const Icon = opt.icon;
              return (
                <button
                  key={opt.id}
                  onClick={() => onSelect(opt.id)}
                  className="type-card"
                >
                  <div
                    className="p-2.5 rounded-xl"
                    style={{ backgroundColor: `${opt.color}20` }}
                  >
                    <Icon size={24} style={{ color: opt.color }} />
                  </div>
                  <span className="text-sm font-semibold text-drasi-text-primary">
                    {opt.label}
                  </span>
                  <span className="text-[10px] text-drasi-text-secondary text-center leading-tight">
                    {opt.description}
                  </span>
                </button>
              );
            })}
          </div>
          <button
            onClick={onCancel}
            className="w-full mt-4 action-btn-ghost text-center"
          >
            Cancel
          </button>
        </div>
      </div>
    );
  }

  // Component level: tabbed interface
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="bg-drasi-surface border border-drasi-border rounded-2xl shadow-2xl w-full max-w-xl mx-4 overflow-hidden flex flex-col" style={{ height: '480px' }}>
        {/* Tab Bar */}
        <div className="flex gap-2 p-3 bg-drasi-bg/50 shrink-0">
          <button
            onClick={() => setActiveTab("components")}
            className={`flex-1 flex items-center justify-center gap-2 py-2.5 px-4 text-sm font-medium rounded-lg transition-all ${
              activeTab === "components"
                ? "bg-drasi-surface text-drasi-text-primary shadow-sm border border-drasi-border"
                : "text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-surface/50"
            }`}
          >
            <Layers size={16} />
            <span>Add Component</span>
          </button>
          <button
            onClick={() => setActiveTab("catalog")}
            className={`flex-1 flex items-center justify-center gap-2 py-2.5 px-4 text-sm font-medium rounded-lg transition-all ${
              activeTab === "catalog"
                ? "bg-drasi-surface text-drasi-text-primary shadow-sm border border-drasi-border"
                : "text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-surface/50"
            }`}
          >
            <Sparkles size={16} />
            <span>Browse Catalog</span>
          </button>
        </div>

        {/* Content */}
        <div className="p-6 flex-1 overflow-y-auto">
          {activeTab === "components" ? (
            /* Components Tab */
            <div className="space-y-3">
              <p className="text-sm text-drasi-text-secondary text-center mb-4">
                Create an individual component
              </p>
              {COMPONENT_TYPES.map((opt) => {
                const Icon = opt.icon;
                return (
                  <button
                    key={opt.id}
                    onClick={() => onSelect(opt.id)}
                    className="w-full flex items-center gap-4 p-4 rounded-xl border border-drasi-border hover:border-drasi-accent hover:bg-drasi-card transition-all group text-left"
                  >
                    <div
                      className="p-3 rounded-xl shrink-0"
                      style={{ backgroundColor: `${opt.color}15` }}
                    >
                      <Icon size={24} style={{ color: opt.color }} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="font-semibold text-drasi-text-primary group-hover:text-drasi-accent transition-colors">
                        {opt.label}
                      </div>
                      <div className="text-sm text-drasi-text-secondary mt-0.5">
                        {opt.description}
                      </div>
                    </div>
                    <ChevronRight
                      size={18}
                      className="text-drasi-text-secondary group-hover:text-drasi-accent shrink-0 transition-colors"
                    />
                  </button>
                );
              })}
            </div>
          ) : (
            /* Catalog Tab */
            <div className="flex flex-col h-full">
              <p className="text-sm text-drasi-text-secondary text-center mb-4 shrink-0">
                Deploy a pre-configured solution with all components
              </p>

              {loadingTemplates ? (
                <div className="flex flex-col items-center justify-center flex-1">
                  <Loader2 size={24} className="animate-spin text-drasi-accent mb-3" />
                  <span className="text-sm text-drasi-text-secondary">Loading templates...</span>
                </div>
              ) : templateError ? (
                <div className="text-center flex-1 flex flex-col items-center justify-center">
                  <Package size={40} className="mb-3 text-drasi-error opacity-50" />
                  <p className="text-sm text-drasi-error">{templateError}</p>
                </div>
              ) : templates.length === 0 ? (
                <div className="text-center flex-1 flex flex-col items-center justify-center">
                  <Package size={40} className="mb-3 text-drasi-text-secondary opacity-30" />
                  <p className="text-sm text-drasi-text-secondary font-medium">No templates available</p>
                  <p className="text-xs text-drasi-text-secondary mt-1 opacity-70">
                    Add .yaml files to the solutions directory
                  </p>
                </div>
              ) : (
                <div className="space-y-2 flex-1 overflow-y-auto pr-1">
                  {templates.map((t) => (
                    <button
                      key={t.id}
                      onClick={() => onSelectSolution(t.id)}
                      className="w-full text-left p-4 rounded-xl border border-drasi-border hover:border-drasi-accent hover:bg-drasi-card transition-all group"
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
                        <ChevronRight
                          size={18}
                          className="text-drasi-text-secondary group-hover:text-drasi-accent shrink-0 mt-1 transition-colors"
                        />
                      </div>
                    </button>
                  ))}
                </div>
              )}

              {/* Upload option */}
              <div className="mt-4 pt-4 border-t border-drasi-border shrink-0">
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
        </div>

        {/* Footer */}
        <div className="px-6 pb-6">
          <button
            onClick={onCancel}
            className="w-full py-2.5 rounded-xl text-sm text-drasi-text-secondary hover:text-drasi-text-primary hover:bg-drasi-card transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
