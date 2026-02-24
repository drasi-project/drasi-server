/**
 * Returns current theme colors by reading CSS variables for theme-aware
 * values and using fixed values for semantic colors.
 */
export function getTheme() {
  const style = getComputedStyle(document.documentElement);
  return {
    bg: style.getPropertyValue("--drasi-bg").trim() || "#0a0e17",
    surface: style.getPropertyValue("--drasi-surface").trim() || "#111827",
    card: style.getPropertyValue("--drasi-card").trim() || "#1e293b",
    border: style.getPropertyValue("--drasi-border").trim() || "#334155",
    textPrimary: style.getPropertyValue("--drasi-text-primary").trim() || "#f1f5f9",
    textSecondary: style.getPropertyValue("--drasi-text-secondary").trim() || "#94a3b8",
    source: "#3b82f6",
    query: "#8b5cf6",
    reaction: "#06b6d4",
    running: "#10b981",
    warning: "#f59e0b",
    error: "#ef4444",
    stopped: "#64748b",
  };
}

/** Static reference for semantic colors (not theme-dependent). */
export const THEME = {
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
