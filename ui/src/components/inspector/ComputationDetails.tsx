import type { ComputationComponent, ComputationInspection } from "@/api/types";
import { computationColor, computationStatus, isInternalComponent, relationshipEndpoints, relationshipLabel } from "@/utils/computation";
import StatusBadge from "@/components/shared/StatusBadge";

interface Props {
  component: ComputationComponent;
  inspection: ComputationInspection;
  standalone: boolean;
  onNavigate: (id: string) => void;
}

export default function ComputationDetails({ component, inspection, standalone, onNavigate }: Props) {
  const connections = inspection.relationships.filter((relationship) => {
    const { from, to } = relationshipEndpoints(relationship);
    return from === component.id || to === component.id;
  });
  const ids = new Set(inspection.components.map((c) => c.id));
  return (
    <section className="p-3 space-y-4 border-t border-drasi-border text-xs" aria-label="Computation details">
      {standalone && (
        <header className="space-y-2">
          <h2 className="font-semibold text-sm break-all text-drasi-text-primary">{component.id}</h2>
          <div style={{ color: computationColor(component.role) }}>
            {component.role}{isInternalComponent(component) ? " (internal)" : ""}
          </div>
          <StatusBadge status={computationStatus(component)} />
          <p className="text-drasi-text-secondary">Graph inspection only. Editing and lifecycle actions for this component are not available here.</p>
        </header>
      )}
      <h3 className="font-semibold text-drasi-text-primary">Computation state</h3>
      <dl className="space-y-1 text-drasi-text-secondary">
        {[
          ["Role", component.role],
          ["Realization", component.realization ?? "Not observed"],
          ["Lifecycle", component.lifecycle ?? "Not observed"],
          ["Health", component.health ?? "Not observed"],
          ["Auto-start", component.autoStart ? "Yes" : "No"],
          ["Failure phase", component.failurePhase ?? "None reported"],
          ["Implementation", component.implementation
            ? `${component.implementation.name} (${component.implementation.version})` : "Not reported by host"],
        ].map(([label, value]) => (
          <div key={label} className="flex gap-2 justify-between">
            <dt className="shrink-0">{label}</dt><dd className="text-right break-all text-drasi-text-primary">{value}</dd>
          </div>
        ))}
      </dl>
      <div className="space-y-2">
        <h3 className="font-semibold text-drasi-text-primary">Ports ({component.ports.length})</h3>
        {component.ports.length === 0 && <p className="text-drasi-text-secondary">No declared data ports. Host subscriptions or control connections may still exist.</p>}
        {component.ports.map((port) => (
          <div key={port.id} className="p-2 rounded border border-drasi-border bg-drasi-card break-all">
            <div className="font-semibold text-drasi-text-primary">{port.id} ({port.direction})</div>
            <div className="text-drasi-text-secondary">{port.schema.id} v{port.schema.version}</div>
            <div className="text-drasi-text-secondary">{port.schema.encoding}</div>
            {port.requirements.required.length > 0 && <div className="text-drasi-text-secondary">Requires: {port.requirements.required.join(", ")}</div>}
          </div>
        ))}
      </div>
      <div className="space-y-2">
        <h3 className="font-semibold text-drasi-text-primary">Graph connections ({connections.length})</h3>
        {connections.length === 0 && <p className="text-drasi-text-secondary">No connections declared.</p>}
        {connections.map((relationship) => {
          const { from, to } = relationshipEndpoints(relationship);
          const other = from === component.id ? to : from;
          return (
            <div key={JSON.stringify(relationship)} className="p-2 rounded border border-drasi-border space-y-1">
              <span className="text-drasi-text-secondary">{from === component.id ? "To " : "From "}</span>
              {ids.has(other) ? (
                <button className="text-drasi-text-primary underline break-all text-left" onClick={() => onNavigate(other)}>{other}</button>
              ) : <span className="text-drasi-warning break-all">{other} (unresolved)</span>}
              <div className="text-drasi-text-secondary break-all">{relationshipLabel(relationship)}</div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
