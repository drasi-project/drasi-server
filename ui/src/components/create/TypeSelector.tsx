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
} from "lucide-react";

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
    description: "Ingest data changes",
    icon: Database,
    color: "#3b82f6",
  },
  {
    id: "query",
    label: "Query",
    description: "Continuous Cypher query",
    icon: Search,
    color: "#8b5cf6",
  },
  {
    id: "reaction",
    label: "Reaction",
    description: "React to changes",
    icon: Zap,
    color: "#06b6d4",
  },
];

const SOURCE_KINDS: TypeOption[] = [
  {
    id: "postgres",
    label: "PostgreSQL",
    description: "CDC via logical replication",
    icon: Database,
    color: "#3b82f6",
  },
  {
    id: "http",
    label: "HTTP",
    description: "Receive events via webhooks",
    icon: Globe,
    color: "#3b82f6",
  },
  {
    id: "grpc",
    label: "gRPC",
    description: "High-perf binary streaming",
    icon: Radio,
    color: "#3b82f6",
  },
  {
    id: "mock",
    label: "Mock",
    description: "Generate test data",
    icon: FlaskConical,
    color: "#3b82f6",
  },
  {
    id: "platform",
    label: "Platform",
    description: "Redis Streams integration",
    icon: Server,
    color: "#3b82f6",
  },
];

const REACTION_KINDS: TypeOption[] = [
  {
    id: "log",
    label: "Log",
    description: "Output to console",
    icon: FileText,
    color: "#06b6d4",
  },
  {
    id: "http-reaction",
    label: "HTTP",
    description: "Send webhooks",
    icon: Globe,
    color: "#06b6d4",
  },
  {
    id: "http-adaptive",
    label: "HTTP Adaptive",
    description: "Dynamic batch webhooks",
    icon: Globe,
    color: "#06b6d4",
  },
  {
    id: "grpc-reaction",
    label: "gRPC",
    description: "Stream via gRPC",
    icon: Radio,
    color: "#06b6d4",
  },
  {
    id: "sse",
    label: "SSE",
    description: "Server-Sent Events",
    icon: Rss,
    color: "#06b6d4",
  },
  {
    id: "profiler",
    label: "Profiler",
    description: "Performance metrics",
    icon: Gauge,
    color: "#06b6d4",
  },
];

interface TypeSelectorProps {
  level: "component" | "source-kind" | "reaction-kind";
  onSelect: (type: SelectableType) => void;
  onCancel: () => void;
}

export default function TypeSelector({
  level,
  onSelect,
  onCancel,
}: TypeSelectorProps) {
  const options =
    level === "component"
      ? COMPONENT_TYPES
      : level === "source-kind"
        ? SOURCE_KINDS
        : REACTION_KINDS;

  const title =
    level === "component"
      ? "What do you want to create?"
      : level === "source-kind"
        ? "Select Source Type"
        : "Select Reaction Type";

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
