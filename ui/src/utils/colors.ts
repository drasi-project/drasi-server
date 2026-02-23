export const THEME = {
  bg: "#0a0e17",
  surface: "#111827",
  card: "#1e293b",
  border: "#334155",
  textPrimary: "#f1f5f9",
  textSecondary: "#94a3b8",
  source: "#3b82f6",
  query: "#8b5cf6",
  reaction: "#06b6d4",
  running: "#10b981",
  warning: "#f59e0b",
  error: "#ef4444",
  stopped: "#64748b",
} as const;

export type ComponentType = "source" | "query" | "reaction";
export type ComponentStatus =
  | "Running"
  | "Starting"
  | "Stopping"
  | "Stopped"
  | "Error"
  | "Reconfiguring";

export function getTypeColor(type: ComponentType): string {
  return THEME[type];
}

export function getStatusColor(status: ComponentStatus): string {
  switch (status) {
    case "Running":
      return THEME.running;
    case "Starting":
    case "Stopping":
    case "Reconfiguring":
      return THEME.warning;
    case "Error":
      return THEME.error;
    case "Stopped":
      return THEME.stopped;
  }
}

export function getStatusGlowClass(status: ComponentStatus): string {
  switch (status) {
    case "Running":
      return "shadow-glow-running";
    case "Starting":
    case "Stopping":
    case "Reconfiguring":
      return "shadow-glow-warning";
    case "Error":
      return "shadow-glow-error";
    case "Stopped":
      return "";
  }
}
