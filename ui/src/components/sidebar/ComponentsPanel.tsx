import { useState, useMemo } from "react";
import {
  Search,
  X,
  Database,
  Globe,
  Radio,
  FlaskConical,
  Rss,
  FileText,
  Zap,
  Puzzle,
} from "lucide-react";
import { usePluginKinds } from "@/hooks/usePluginKinds";

type CatalogFilter = "all" | "source" | "query" | "reaction";

const FILTERS: { id: CatalogFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "source", label: "Sources" },
  { id: "query", label: "Queries" },
  { id: "reaction", label: "Reactions" },
];

interface CatalogEntry {
  componentType: "source" | "query" | "reaction";
  kind: string;
  label: string;
  description: string;
  icon: React.ElementType;
  color: string;
}

// Well-known icons for specific kinds
const KIND_ICONS: Record<string, React.ElementType> = {
  postgres: Database,
  http: Globe,
  grpc: Radio,
  mock: FlaskConical,
  log: FileText,
  sse: Rss,
};

// Built-in source entries
const BUILTIN_SOURCES: CatalogEntry[] = [
  {
    componentType: "source",
    kind: "postgres",
    label: "PostgreSQL",
    description: "PostgreSQL CDC source",
    icon: Database,
    color: "#22c55e",
  },
  {
    componentType: "source",
    kind: "http",
    label: "HTTP",
    description: "HTTP endpoint source",
    icon: Globe,
    color: "#22c55e",
  },
  {
    componentType: "source",
    kind: "grpc",
    label: "gRPC",
    description: "gRPC streaming source",
    icon: Radio,
    color: "#22c55e",
  },
  {
    componentType: "source",
    kind: "mock",
    label: "Mock",
    description: "Mock data for testing",
    icon: FlaskConical,
    color: "#22c55e",
  },
];

// Built-in query entry
const BUILTIN_QUERIES: CatalogEntry[] = [
  {
    componentType: "query",
    kind: "query",
    label: "Cypher Query",
    description: "Continuous Cypher query",
    icon: Search,
    color: "#3b82f6",
  },
];

// Built-in reaction entries
const BUILTIN_REACTIONS: CatalogEntry[] = [
  {
    componentType: "reaction",
    kind: "log",
    label: "Log",
    description: "Console log output",
    icon: FileText,
    color: "#8b5cf6",
  },
  {
    componentType: "reaction",
    kind: "http",
    label: "HTTP",
    description: "HTTP webhook reaction",
    icon: Globe,
    color: "#8b5cf6",
  },
  {
    componentType: "reaction",
    kind: "sse",
    label: "SSE",
    description: "Server-Sent Events stream",
    icon: Rss,
    color: "#8b5cf6",
  },
  {
    componentType: "reaction",
    kind: "grpc",
    label: "gRPC",
    description: "gRPC streaming reaction",
    icon: Radio,
    color: "#8b5cf6",
  },
];

// Descriptions for well-known source/reaction kinds
const SOURCE_DESCRIPTIONS: Record<string, string> = {
  postgres: "PostgreSQL CDC source",
  http: "HTTP endpoint source",
  grpc: "gRPC streaming source",
  mock: "Mock data for testing",
};

const REACTION_DESCRIPTIONS: Record<string, string> = {
  log: "Console log output",
  http: "HTTP webhook reaction",
  grpc: "gRPC streaming reaction",
  sse: "Server-Sent Events stream",
};

interface ComponentsPanelProps {
  onStartCreate: (componentType: "source" | "query" | "reaction", kind?: string) => void;
}

export default function ComponentsPanel({ onStartCreate }: ComponentsPanelProps) {
  const [searchText, setSearchText] = useState("");
  const [filter, setFilter] = useState<CatalogFilter>("all");
  const { kinds: pluginKinds } = usePluginKinds();

  // Build catalog entries: merge built-ins with plugin-provided kinds
  const entries = useMemo(() => {
    const builtinSourceKinds = new Set(BUILTIN_SOURCES.map((s) => s.kind));
    const builtinReactionKinds = new Set(BUILTIN_REACTIONS.map((r) => r.kind));

    const sources = [...BUILTIN_SOURCES];
    const reactions = [...BUILTIN_REACTIONS];

    if (pluginKinds) {
      for (const pk of pluginKinds.sources) {
        if (!builtinSourceKinds.has(pk.kind)) {
          sources.push({
            componentType: "source",
            kind: pk.kind,
            label: pk.kind.charAt(0).toUpperCase() + pk.kind.slice(1),
            description: SOURCE_DESCRIPTIONS[pk.kind] ?? `Plugin-provided source`,
            icon: KIND_ICONS[pk.kind] ?? Puzzle,
            color: "#22c55e",
          });
        }
      }
      for (const pk of pluginKinds.reactions) {
        if (!builtinReactionKinds.has(pk.kind)) {
          reactions.push({
            componentType: "reaction",
            kind: pk.kind,
            label: pk.kind.charAt(0).toUpperCase() + pk.kind.slice(1),
            description: REACTION_DESCRIPTIONS[pk.kind] ?? `Plugin-provided reaction`,
            icon: KIND_ICONS[pk.kind] ?? Puzzle,
            color: "#8b5cf6",
          });
        }
      }
    }

    return [...sources, ...BUILTIN_QUERIES, ...reactions];
  }, [pluginKinds]);

  // Filter and search
  const filtered = useMemo(() => {
    let result = entries;
    if (filter !== "all") {
      result = result.filter((e) => e.componentType === filter);
    }
    if (searchText.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(
        (e) =>
          e.label.toLowerCase().includes(q) ||
          e.description.toLowerCase().includes(q) ||
          e.kind.toLowerCase().includes(q),
      );
    }
    return result;
  }, [entries, filter, searchText]);

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
            placeholder="Search components…"
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

      {/* Filter chips */}
      <div className="px-3 pb-2 flex gap-1 flex-wrap flex-shrink-0">
        {FILTERS.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors ${
              filter === f.id
                ? "bg-[var(--drasi-card)] text-[var(--drasi-text-primary)] border border-[var(--drasi-text-secondary)]"
                : "text-[var(--drasi-text-secondary)] border border-[var(--drasi-border)] hover:border-[var(--drasi-text-secondary)]"
            }`}
          >
            {f.label}
          </button>
        ))}
      </div>

      {/* Component cards */}
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-40">
            <Zap
              size={32}
              className="text-[var(--drasi-text-secondary)] opacity-30 mb-3"
            />
            <p className="text-sm text-[var(--drasi-text-secondary)] font-medium">
              No matching components
            </p>
          </div>
        ) : (
          <div className="space-y-1.5">
            {filtered.map((entry) => {
              const Icon = entry.icon;
              return (
                <button
                  key={`${entry.componentType}-${entry.kind}`}
                  onClick={() => onStartCreate(entry.componentType, entry.kind)}
                  className="w-full text-left flex items-center gap-3 p-2.5 rounded-xl border border-[var(--drasi-border)] hover:border-[var(--drasi-text-secondary)] hover:bg-[var(--drasi-card)] transition-colors group"
                >
                  <div
                    className="p-2 rounded-lg flex-shrink-0"
                    style={{ backgroundColor: `${entry.color}15` }}
                  >
                    <Icon size={16} style={{ color: entry.color }} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-xs font-semibold text-[var(--drasi-text-primary)] group-hover:text-[var(--drasi-accent)] transition-colors truncate">
                      {entry.label}
                    </div>
                    <div className="text-[10px] text-[var(--drasi-text-secondary)] truncate">
                      {entry.description}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
