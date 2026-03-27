import React from "react";
import {
  AlertCircle,
  Database,
  Globe,
  Radio,
  FlaskConical,
  Server,
  Search,
  Zap,
  FileText,
  Rss,
  Gauge,
  GitBranch,
  Code,
  Info,
} from "lucide-react";
import Editor from "@monaco-editor/react";
import StatusBadge from "@/components/shared/StatusBadge";
import ActionButtons from "@/components/shared/ActionButtons";
import ConnectedComponentItem from "@/components/inspector/ConnectedComponentItem";
import QueryCodeViewer from "@/components/inspector/QueryCodeViewer";
import type { ComponentStatus, ComponentType } from "@/utils/colors";
import { getTypeColor } from "@/utils/colors";

const SOURCE_ICON_MAP: Record<string, React.ElementType> = {
  postgres: Database,
  http: Globe,
  grpc: Radio,
  mock: FlaskConical,
  platform: Server,
};

const REACTION_ICON_MAP: Record<string, React.ElementType> = {
  log: FileText,
  http: Globe,
  "http-adaptive": Globe,
  grpc: Radio,
  "grpc-adaptive": Radio,
  sse: Rss,
  platform: Server,
  profiler: Gauge,
};

// Shared connected component type
interface ConnectedComponent {
  id: string;
  type: ComponentType;
  status: ComponentStatus;
  kind?: string;
}

export interface SourceInspectorData {
  isSource: true;
  id: string;
  kind: string;
  status: ComponentStatus;
  error?: string;
  autoStart?: boolean;
  properties?: Record<string, unknown>;
  queries: ConnectedComponent[];
  onStart: () => void;
  onStop: () => void;
  onDelete: () => void;
}

export interface QueryInspectorData {
  isQuery: true;
  id: string;
  status: ComponentStatus;
  error?: string;
  query: string;
  queryLanguage: string;
  sources: ConnectedComponent[];
  reactions: ConnectedComponent[];
  onStart: () => void;
  onStop: () => void;
  onDelete: () => void;
}

export interface ReactionInspectorData {
  isReaction: true;
  id: string;
  kind: string;
  status: ComponentStatus;
  error?: string;
  autoStart?: boolean;
  properties?: Record<string, unknown>;
  queries: ConnectedComponent[];
  onStart: () => void;
  onStop: () => void;
  onDelete: () => void;
}

export type InspectorData =
  | SourceInspectorData
  | QueryInspectorData
  | ReactionInspectorData;

interface CurrentComponentPanelProps {
  data: InspectorData | null;
  onNavigate?: (id: string, type: ComponentType) => void;
  onStartSource?: (id: string) => void;
  onStopSource?: (id: string) => void;
  onStartQuery?: (id: string) => void;
  onStopQuery?: (id: string) => void;
  onStartReaction?: (id: string) => void;
  onStopReaction?: (id: string) => void;
}

export default function CurrentComponentPanel({
  data,
  onNavigate,
  onStartSource,
  onStopSource,
  onStartQuery,
  onStopQuery,
  onStartReaction,
  onStopReaction,
}: CurrentComponentPanelProps) {
  if (!data) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 px-4 text-center">
        <Info size={32} className="text-[var(--drasi-text-secondary)] opacity-30" />
        <p className="text-sm font-medium text-[var(--drasi-text-primary)]">
          No component selected
        </p>
        <p className="text-xs text-[var(--drasi-text-secondary)]">
          Select a component on the canvas to view its details.
        </p>
      </div>
    );
  }

  if ("isSource" in data && data.isSource) {
    return (
      <SourceView
        data={data}
        onNavigate={onNavigate}
        onStartQuery={onStartQuery}
        onStopQuery={onStopQuery}
      />
    );
  }

  if ("isQuery" in data && data.isQuery) {
    return (
      <QueryView
        data={data}
        onNavigate={onNavigate}
        onStartSource={onStartSource}
        onStopSource={onStopSource}
        onStartReaction={onStartReaction}
        onStopReaction={onStopReaction}
      />
    );
  }

  if ("isReaction" in data && data.isReaction) {
    return (
      <ReactionView
        data={data}
        onNavigate={onNavigate}
        onStartQuery={onStartQuery}
        onStopQuery={onStopQuery}
      />
    );
  }

  return null;
}

// ─── Source View ──────────────────────────────────────────

function SourceView({
  data,
  onNavigate,
  onStartQuery,
  onStopQuery,
}: {
  data: SourceInspectorData;
  onNavigate?: (id: string, type: ComponentType) => void;
  onStartQuery?: (id: string) => void;
  onStopQuery?: (id: string) => void;
}) {
  const accentColor = getTypeColor("source");
  const queryColor = getTypeColor("query");
  const showError = data.status === "Error" && data.error;
  const Icon = SOURCE_ICON_MAP[data.kind] || Database;

  return (
    <div className="flex flex-col min-w-0 overflow-hidden">
      {/* Header */}
      <div
        className="p-3 border-b border-[var(--drasi-border)]"
        style={{ borderTopColor: accentColor, borderTopWidth: 3 }}
      >
        <div className="flex items-center gap-2.5 mb-2">
          <div
            className="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
            style={{ backgroundColor: `${accentColor}20` }}
          >
            <Icon size={16} style={{ color: accentColor }} />
          </div>
          <div className="min-w-0">
            <h2 className="text-sm font-bold text-[var(--drasi-text-primary)] truncate">
              {data.id}
            </h2>
            <div className="flex items-center gap-1.5">
              <span
                className="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider"
                style={{
                  backgroundColor: `${accentColor}20`,
                  color: accentColor,
                  border: `1px solid ${accentColor}40`,
                }}
              >
                {data.kind}
              </span>
              <span className="text-[10px] text-[var(--drasi-text-secondary)]">
                Source
              </span>
            </div>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <StatusBadge status={data.status} size="sm" />
          <ActionButtons
            status={data.status}
            componentName={data.id}
            onStart={data.onStart}
            onStop={data.onStop}
            onDelete={data.onDelete}
            deleteDisabled={data.queries.length > 0}
            deleteDisabledReason={
              data.queries.length > 0
                ? `Cannot delete: ${data.queries.length} query(s) depend on this source`
                : undefined
            }
            compact
          />
        </div>
      </div>

      {/* Error */}
      {showError && <ErrorBlock error={data.error!} />}

      {/* Data Flow */}
      <div className="p-3 border-b border-[var(--drasi-border)]">
        <SectionTitle icon={<GitBranch size={12} />} title="Data Flow" />
        <ConnectedList
          label="Queries"
          labelIcon={<Search size={10} className="text-drasi-query" />}
          accentColor={queryColor}
          direction="OUTPUT"
          items={data.queries}
          onNavigate={onNavigate}
          onStart={onStartQuery}
          onStop={onStopQuery}
        />
      </div>

      {/* Configuration */}
      <ConfigSection autoStart={data.autoStart} properties={data.properties} />
    </div>
  );
}

// ─── Query View ──────────────────────────────────────────

function QueryView({
  data,
  onNavigate,
  onStartSource,
  onStopSource,
  onStartReaction,
  onStopReaction,
}: {
  data: QueryInspectorData;
  onNavigate?: (id: string, type: ComponentType) => void;
  onStartSource?: (id: string) => void;
  onStopSource?: (id: string) => void;
  onStartReaction?: (id: string) => void;
  onStopReaction?: (id: string) => void;
}) {
  const accentColor = getTypeColor("query");
  const sourceColor = getTypeColor("source");
  const reactionColor = getTypeColor("reaction");
  const showError = data.status === "Error" && data.error;

  return (
    <div className="flex flex-col min-w-0 overflow-hidden">
      {/* Header */}
      <div
        className="p-3 border-b border-[var(--drasi-border)]"
        style={{ borderTopColor: accentColor, borderTopWidth: 3 }}
      >
        <div className="flex items-center gap-2.5 mb-2">
          <div
            className="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
            style={{ backgroundColor: `${accentColor}20` }}
          >
            <Search size={16} style={{ color: accentColor }} />
          </div>
          <div className="min-w-0">
            <h2 className="text-sm font-bold text-[var(--drasi-text-primary)] truncate">
              {data.id}
            </h2>
            <div className="flex items-center gap-1.5">
              <span
                className="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider"
                style={{
                  backgroundColor: `${accentColor}20`,
                  color: accentColor,
                  border: `1px solid ${accentColor}40`,
                }}
              >
                {data.queryLanguage}
              </span>
              <span className="text-[10px] text-[var(--drasi-text-secondary)]">
                Continuous Query
              </span>
            </div>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <StatusBadge status={data.status} size="sm" />
          <ActionButtons
            status={data.status}
            componentName={data.id}
            onStart={data.onStart}
            onStop={data.onStop}
            onDelete={data.onDelete}
            deleteDisabled={data.reactions.length > 0}
            deleteDisabledReason={
              data.reactions.length > 0
                ? `Cannot delete: ${data.reactions.length} reaction(s) depend on this query`
                : undefined
            }
            compact
          />
        </div>
      </div>

      {/* Error */}
      {showError && <ErrorBlock error={data.error!} />}

      {/* Data Flow */}
      <div className="p-3 border-b border-[var(--drasi-border)]">
        <SectionTitle icon={<GitBranch size={12} />} title="Data Flow" />
        <div className="space-y-3">
          <ConnectedList
            label="Sources"
            labelIcon={<Database size={10} className="text-drasi-source" />}
            accentColor={sourceColor}
            direction="INPUT"
            items={data.sources}
            onNavigate={onNavigate}
            onStart={onStartSource}
            onStop={onStopSource}
          />
          <ConnectedList
            label="Reactions"
            labelIcon={<Zap size={10} className="text-drasi-reaction" />}
            accentColor={reactionColor}
            direction="OUTPUT"
            items={data.reactions}
            onNavigate={onNavigate}
            onStart={onStartReaction}
            onStop={onStopReaction}
          />
        </div>
      </div>

      {/* Query Definition */}
      <div className="p-3">
        <SectionTitle
          icon={<Code size={12} className="text-drasi-query" />}
          title="Query Definition"
        />
        <QueryCodeViewer
          query={data.query}
          queryLanguage={data.queryLanguage}
        />
      </div>
    </div>
  );
}

// ─── Reaction View ───────────────────────────────────────

function ReactionView({
  data,
  onNavigate,
  onStartQuery,
  onStopQuery,
}: {
  data: ReactionInspectorData;
  onNavigate?: (id: string, type: ComponentType) => void;
  onStartQuery?: (id: string) => void;
  onStopQuery?: (id: string) => void;
}) {
  const accentColor = getTypeColor("reaction");
  const queryColor = getTypeColor("query");
  const showError = data.status === "Error" && data.error;
  const Icon = REACTION_ICON_MAP[data.kind] || Zap;

  return (
    <div className="flex flex-col min-w-0 overflow-hidden">
      {/* Header */}
      <div
        className="p-3 border-b border-[var(--drasi-border)]"
        style={{ borderTopColor: accentColor, borderTopWidth: 3 }}
      >
        <div className="flex items-center gap-2.5 mb-2">
          <div
            className="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
            style={{ backgroundColor: `${accentColor}20` }}
          >
            <Icon size={16} style={{ color: accentColor }} />
          </div>
          <div className="min-w-0">
            <h2 className="text-sm font-bold text-[var(--drasi-text-primary)] truncate">
              {data.id}
            </h2>
            <div className="flex items-center gap-1.5">
              <span
                className="px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider"
                style={{
                  backgroundColor: `${accentColor}20`,
                  color: accentColor,
                  border: `1px solid ${accentColor}40`,
                }}
              >
                {data.kind}
              </span>
              <span className="text-[10px] text-[var(--drasi-text-secondary)]">
                Reaction
              </span>
            </div>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <StatusBadge status={data.status} size="sm" />
          <ActionButtons
            status={data.status}
            componentName={data.id}
            onStart={data.onStart}
            onStop={data.onStop}
            onDelete={data.onDelete}
            compact
          />
        </div>
      </div>

      {/* Error */}
      {showError && <ErrorBlock error={data.error!} />}

      {/* Data Flow */}
      <div className="p-3 border-b border-[var(--drasi-border)]">
        <SectionTitle icon={<GitBranch size={12} />} title="Data Flow" />
        <ConnectedList
          label="Queries"
          labelIcon={<Search size={10} className="text-drasi-query" />}
          accentColor={queryColor}
          direction="INPUT"
          items={data.queries}
          onNavigate={onNavigate}
          onStart={onStartQuery}
          onStop={onStopQuery}
        />
      </div>

      {/* Configuration */}
      <ConfigSection autoStart={data.autoStart} properties={data.properties} />
    </div>
  );
}

// ─── Shared helpers ──────────────────────────────────────

function ErrorBlock({ error }: { error: string }) {
  return (
    <div className="p-3 border-b border-[var(--drasi-border)]">
      <div className="flex items-start gap-2 p-2.5 bg-red-500/10 rounded-lg border border-red-500/20">
        <AlertCircle size={14} className="text-red-500 shrink-0 mt-0.5" />
        <div>
          <h3 className="text-[10px] font-semibold text-red-400 uppercase tracking-wider mb-0.5">
            Error Details
          </h3>
          <p className="text-xs text-red-300 break-words leading-relaxed">
            {error}
          </p>
        </div>
      </div>
    </div>
  );
}

function SectionTitle({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <div className="flex items-center gap-1.5 mb-2">
      <span className="text-[var(--drasi-text-secondary)]">{icon}</span>
      <h3 className="text-[10px] font-semibold text-[var(--drasi-text-secondary)] uppercase tracking-wider">
        {title}
      </h3>
    </div>
  );
}

function ConnectedList({
  label,
  labelIcon,
  accentColor,
  direction,
  items,
  onNavigate,
  onStart,
  onStop,
}: {
  label: string;
  labelIcon: React.ReactNode;
  accentColor: string;
  direction: string;
  items: ConnectedComponent[];
  onNavigate?: (id: string, type: ComponentType) => void;
  onStart?: (id: string) => void;
  onStop?: (id: string) => void;
}) {
  return (
    <div>
      <div className="flex items-center gap-1.5 mb-1.5">
        {labelIcon}
        <span className="text-[10px] font-medium" style={{ color: accentColor }}>
          {label} ({items.length})
        </span>
        <div className="flex-1 h-px bg-[var(--drasi-border)]" />
        <span className="text-[9px] text-[var(--drasi-text-secondary)]">
          {direction}
        </span>
      </div>
      {items.length > 0 ? (
        <div className="grid gap-1.5">
          {items.map((item) => (
            <ConnectedComponentItem
              key={item.id}
              id={item.id}
              type={item.type}
              status={item.status}
              kind={item.kind}
              accentColor={accentColor}
              onNavigate={onNavigate}
              onStart={onStart}
              onStop={onStop}
            />
          ))}
        </div>
      ) : (
        <div className="text-xs text-[var(--drasi-text-secondary)] italic py-1">
          No {label.toLowerCase()} connected
        </div>
      )}
    </div>
  );
}

function ConfigSection({
  autoStart,
  properties,
}: {
  autoStart?: boolean;
  properties?: Record<string, unknown>;
}) {
  const yamlContent = React.useMemo(() => {
    const config: Record<string, unknown> = {};
    if (autoStart !== undefined) config.autoStart = autoStart;
    if (properties) {
      for (const [k, v] of Object.entries(properties)) {
        config[k] = v;
      }
    }
    try {
      // Use js-yaml if available, otherwise JSON
      return Object.keys(config).length > 0
        ? Object.entries(config)
            .map(([k, v]) => {
              if (typeof v === "object" && v !== null) {
                return `${k}: ${JSON.stringify(v, null, 2).replace(/\n/g, "\n  ")}`;
              }
              return `${k}: ${JSON.stringify(v)}`;
            })
            .join("\n")
        : "# No configuration";
    } catch {
      return "# No configuration";
    }
  }, [autoStart, properties]);

  return (
    <div className="p-3">
      <h3 className="text-[10px] font-semibold text-[var(--drasi-text-secondary)] uppercase tracking-wider mb-2">
        Configuration
      </h3>
      <div className="rounded-lg border border-[var(--drasi-border)]/50 overflow-hidden">
        <Editor
          height={Math.min(Math.max(yamlContent.split("\n").length * 18 + 16, 60), 300) + "px"}
          language="yaml"
          value={yamlContent}
          theme={document.documentElement.classList.contains("dark") ? "vs-dark" : "vs"}
          options={{
            readOnly: true,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            lineNumbers: "off",
            fontSize: 11,
            fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Menlo', monospace",
            tabSize: 2,
            automaticLayout: true,
            padding: { top: 6, bottom: 6 },
            renderLineHighlight: "none",
            overviewRulerLanes: 0,
            overviewRulerBorder: false,
            scrollbar: {
              vertical: "auto",
              horizontal: "auto",
              verticalScrollbarSize: 4,
              horizontalScrollbarSize: 4,
            },
            folding: true,
            wordWrap: "on",
          }}
        />
      </div>
    </div>
  );
}
