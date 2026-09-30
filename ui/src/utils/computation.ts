import type { ComputationComponent, ComputationRelationship } from "../api/types";
import type { ComponentStatus } from "./colors";

const INTERNAL_IMPLEMENTATIONS = new Set([
  "drasi/source-plugin-subscription",
  "drasi/query-scheduled-source",
  "drasi/query-results-outlet",
]);

export function isInternalComponent(component: ComputationComponent): boolean {
  return component.id.startsWith("__")
    || INTERNAL_IMPLEMENTATIONS.has(component.implementation?.name ?? "");
}

export function computationStatus(component: ComputationComponent): ComponentStatus {
  if (component.realization === "CreationFailed" || component.lifecycle === "Failed") return "Error";
  if (component.realization === "Pending" || component.realization === "Creating"
    || component.realization === "Blocked") return component.realization;
  switch (component.lifecycle) {
    case "Running":
    case "Starting":
    case "Stopping":
    case "Stopped":
    case "Quiescing":
    case "Quiesced":
      return component.lifecycle;
    default:
      return "Unknown";
  }
}

export function computationColor(role: string): string {
  switch (role) {
    case "Source": return "#22c55e";
    case "Query": return "#3b82f6";
    case "Reaction": return "#8b5cf6";
    case "Transformer": return "#f59e0b";
    case "Sink": return "#ec4899";
    case "Service": return "#06b6d4";
    default: return "#94a3b8";
  }
}

export function relationshipEndpoints(relationship: ComputationRelationship) {
  return relationship.representation === "NativeProvider"
    ? { from: relationship.from.component, to: relationship.to.component }
    : { from: relationship.from, to: relationship.to };
}

export function relationshipLabel(relationship: ComputationRelationship): string {
  switch (relationship.representation) {
    case "NativeProvider":
      return `${relationship.from.port} -> ${relationship.to.port}; ${relationship.binding ?? "Unknown"}; ${relationship.availability ?? "Unknown"}`;
    case "HostSubscription":
      return `Host subscription; producer ${relationship.producerStarted === null ? "unknown" : relationship.producerStarted ? "started" : "not started"}; consumer ${relationship.consumerStarted === null ? "unknown" : relationship.consumerStarted ? "started" : "not started"}`;
    case "ControlConnection":
      return "Control connection (not data flow)";
  }
}
