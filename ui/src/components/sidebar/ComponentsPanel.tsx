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
  Code,
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

// Well-known icons and descriptions for specific kinds
const KIND_META: Record<string, { icon: React.ElementType; label: string; description: string }> = {
  // Sources
  "source/postgres": { icon: Database, label: "PostgreSQL", description: "PostgreSQL CDC source" },
  "source/http": { icon: Globe, label: "HTTP", description: "HTTP endpoint source" },
  "source/grpc": { icon: Radio, label: "gRPC", description: "gRPC streaming source" },
  "source/mock": { icon: FlaskConical, label: "Mock", description: "Mock data for testing" },
  // Reactions
  "reaction/log": { icon: FileText, label: "Log", description: "Console log output" },
  "reaction/http": { icon: Globe, label: "HTTP", description: "HTTP webhook reaction" },
  "reaction/sse": { icon: Rss, label: "SSE", description: "Server-Sent Events stream" },
  "reaction/grpc": { icon: Radio, label: "gRPC", description: "gRPC streaming reaction" },
};

// Queries are always available (they're built into drasi-lib, not plugins)
const QUERY_ENTRIES: CatalogEntry[] = [
  {
    componentType: "query",
    kind: "cypher",
    label: "openCypher Query",
    description: "Continuous openCypher query",
    icon: Code,
    color: "#3b82f6",
  },
  {
    componentType: "query",
    kind: "gql",
    label: "GQL Query",
    description: "Continuous GQL query",
    icon: Search,
    color: "#3b82f6",
  },
];

const SOURCE_COLOR = "#22c55e";
const REACTION_COLOR = "#8b5cf6";

interface ComponentsPanelProps {
  onStartCreate: (componentType: "source" | "query" | "reaction", kind?: string) => void;
}

export default function ComponentsPanel({ onStartCreate }: ComponentsPanelProps) {
  const [searchText, setSearchText] = useState("");
  const [filter, setFilter] = useState<CatalogFilter>("all");
  const { kinds: pluginKinds } = usePluginKinds();

  // Build catalog entries entirely from installed plugins + built-in queries
  const entries = useMemo(() => {
    const sources: CatalogEntry[] = [];
    const reactions: CatalogEntry[] = [];

    if (pluginKinds) {
      for (const pk of pluginKinds.sources) {
        const meta = KIND_META[`source/${pk.kind}`];
        sources.push({
          componentType: "source",
          kind: pk.kind,
          label: meta?.label ?? pk.kind.charAt(0).toUpperCase() + pk.kind.slice(1),
          description: meta?.description ?? "Plugin-provided source",
          icon: meta?.icon ?? Puzzle,
          color: SOURCE_COLOR,
        });
      }
      for (const pk of pluginKinds.reactions) {
        const meta = KIND_META[`reaction/${pk.kind}`];
        reactions.push({
          componentType: "reaction",
          kind: pk.kind,
          label: meta?.label ?? pk.kind.charAt(0).toUpperCase() + pk.kind.slice(1),
          description: meta?.description ?? "Plugin-provided reaction",
          icon: meta?.icon ?? Puzzle,
          color: REACTION_COLOR,
        });
      }
    }

    return [...sources, ...QUERY_ENTRIES, ...reactions];
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
