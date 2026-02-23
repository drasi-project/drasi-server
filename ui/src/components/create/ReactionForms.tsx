import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import FormField from "./FormField";

interface ReactionFormProps {
  kind: string;
  fields: Record<string, unknown>;
  errors: Record<string, string>;
  onChange: (field: string, value: unknown) => void;
  availableQueries: { id: string }[];
}

function QueryMultiSelect({
  selected,
  available,
  onChange,
  error,
}: {
  selected: string[];
  available: { id: string }[];
  onChange: (field: string, value: unknown) => void;
  error?: string;
}) {
  const toggle = (qId: string) => {
    const current = [...selected];
    const idx = current.indexOf(qId);
    if (idx >= 0) current.splice(idx, 1);
    else current.push(qId);
    onChange("queries", current);
  };

  return (
    <div className="space-y-1">
      <label className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
        Queries <span className="text-drasi-error">*</span>
      </label>
      {error && <p className="text-[10px] text-drasi-error">{error}</p>}
      {available.length === 0 ? (
        <p className="text-xs text-drasi-text-secondary italic">
          No queries available. Create a query first.
        </p>
      ) : (
        <div className="space-y-1.5">
          {available.map((q) => (
            <label
              key={q.id}
              className="flex items-center gap-2 p-2 rounded-lg bg-drasi-card border border-drasi-border cursor-pointer hover:border-drasi-text-secondary transition-colors"
            >
              <input
                type="checkbox"
                checked={selected.includes(q.id)}
                onChange={() => toggle(q.id)}
                className="rounded border-drasi-border text-drasi-reaction focus:ring-drasi-reaction"
              />
              <span className="text-sm text-drasi-text-primary">{q.id}</span>
            </label>
          ))}
        </div>
      )}
    </div>
  );
}

function AdvancedToggle({
  open,
  onToggle,
}: {
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex items-center gap-1 text-xs text-drasi-text-secondary hover:text-drasi-text-primary transition-colors mt-2"
    >
      {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
      Advanced
    </button>
  );
}

function LogReactionForm({ fields, errors, onChange, availableQueries }: ReactionFormProps) {
  return (
    <>
      <FormField label="Reaction ID" field="id" value={fields.id} onChange={onChange} error={errors.id} required placeholder="my-log-reaction" />
      <QueryMultiSelect selected={(fields.queries as string[]) ?? []} available={availableQueries} onChange={onChange} error={errors.queries} />
      <FormField label="Auto Start" field="autoStart" value={fields.autoStart} onChange={onChange} type="toggle" />
    </>
  );
}

function HttpReactionForm({ kind, fields, errors, onChange, availableQueries }: ReactionFormProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);
  const isAdaptive = kind === "http-adaptive";

  return (
    <>
      <FormField label="Reaction ID" field="id" value={fields.id} onChange={onChange} error={errors.id} required placeholder={isAdaptive ? "my-http-adaptive" : "my-http-reaction"} />
      <QueryMultiSelect selected={(fields.queries as string[]) ?? []} available={availableQueries} onChange={onChange} error={errors.queries} />
      <FormField label="Base URL" field="baseUrl" value={fields.baseUrl} onChange={onChange} error={errors.baseUrl} required placeholder="http://webhook.example.com" />
      <FormField label="Auto Start" field="autoStart" value={fields.autoStart} onChange={onChange} type="toggle" />
      <AdvancedToggle open={showAdvanced} onToggle={() => setShowAdvanced(!showAdvanced)} />
      {showAdvanced && (
        <>
          <FormField label="Bearer Token" field="token" value={fields.token} onChange={onChange} type="password" placeholder="Optional bearer token" />
          <FormField label="Timeout (ms)" field="timeoutMs" value={fields.timeoutMs} onChange={onChange} type="number" placeholder="5000" />
          {isAdaptive && (
            <>
              <FormField label="Min Batch Size" field="adaptiveMinBatchSize" value={fields.adaptiveMinBatchSize} onChange={onChange} type="number" />
              <FormField label="Max Batch Size" field="adaptiveMaxBatchSize" value={fields.adaptiveMaxBatchSize} onChange={onChange} type="number" />
              <FormField label="Batch Timeout (ms)" field="adaptiveBatchTimeoutMs" value={fields.adaptiveBatchTimeoutMs} onChange={onChange} type="number" />
            </>
          )}
        </>
      )}
    </>
  );
}

function SseReactionForm({ fields, errors, onChange, availableQueries }: ReactionFormProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <>
      <FormField label="Reaction ID" field="id" value={fields.id} onChange={onChange} error={errors.id} required placeholder="my-sse-reaction" />
      <QueryMultiSelect selected={(fields.queries as string[]) ?? []} available={availableQueries} onChange={onChange} error={errors.queries} />
      <FormField label="Auto Start" field="autoStart" value={fields.autoStart} onChange={onChange} type="toggle" />
      <AdvancedToggle open={showAdvanced} onToggle={() => setShowAdvanced(!showAdvanced)} />
      {showAdvanced && (
        <>
          <FormField label="Host" field="host" value={fields.host} onChange={onChange} placeholder="0.0.0.0" />
          <FormField label="Port" field="port" value={fields.port} onChange={onChange} type="number" placeholder="8081" />
          <FormField label="SSE Path" field="ssePath" value={fields.ssePath} onChange={onChange} placeholder="/events" />
          <FormField label="Heartbeat Interval (ms)" field="heartbeatIntervalMs" value={fields.heartbeatIntervalMs} onChange={onChange} type="number" placeholder="30000" />
        </>
      )}
    </>
  );
}

function GrpcReactionForm({ kind, fields, errors, onChange, availableQueries }: ReactionFormProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);
  const isAdaptive = kind === "grpc-adaptive";

  return (
    <>
      <FormField label="Reaction ID" field="id" value={fields.id} onChange={onChange} error={errors.id} required placeholder={isAdaptive ? "my-grpc-adaptive" : "my-grpc-reaction"} />
      <QueryMultiSelect selected={(fields.queries as string[]) ?? []} available={availableQueries} onChange={onChange} error={errors.queries} />
      <FormField label="Endpoint" field="endpoint" value={fields.endpoint} onChange={onChange} error={errors.endpoint} required placeholder="grpc://localhost:50052" />
      <FormField label="Auto Start" field="autoStart" value={fields.autoStart} onChange={onChange} type="toggle" />
      <AdvancedToggle open={showAdvanced} onToggle={() => setShowAdvanced(!showAdvanced)} />
      {showAdvanced && (
        <>
          <FormField label="Timeout (ms)" field="timeoutMs" value={fields.timeoutMs} onChange={onChange} type="number" placeholder="5000" />
          {isAdaptive && (
            <>
              <FormField label="Min Batch Size" field="adaptiveMinBatchSize" value={fields.adaptiveMinBatchSize} onChange={onChange} type="number" />
              <FormField label="Max Batch Size" field="adaptiveMaxBatchSize" value={fields.adaptiveMaxBatchSize} onChange={onChange} type="number" />
            </>
          )}
        </>
      )}
    </>
  );
}

function ProfilerReactionForm({ fields, errors, onChange, availableQueries }: ReactionFormProps) {
  return (
    <>
      <FormField label="Reaction ID" field="id" value={fields.id} onChange={onChange} error={errors.id} required placeholder="my-profiler" />
      <QueryMultiSelect selected={(fields.queries as string[]) ?? []} available={availableQueries} onChange={onChange} error={errors.queries} />
      <FormField label="Auto Start" field="autoStart" value={fields.autoStart} onChange={onChange} type="toggle" />
    </>
  );
}

function PlatformReactionForm({ fields, errors, onChange, availableQueries }: ReactionFormProps) {
  return (
    <>
      <FormField label="Reaction ID" field="id" value={fields.id} onChange={onChange} error={errors.id} required placeholder="my-platform-reaction" />
      <QueryMultiSelect selected={(fields.queries as string[]) ?? []} available={availableQueries} onChange={onChange} error={errors.queries} />
      <FormField label="Auto Start" field="autoStart" value={fields.autoStart} onChange={onChange} type="toggle" />
    </>
  );
}

export default function ReactionForm(props: ReactionFormProps) {
  switch (props.kind) {
    case "log":
      return <LogReactionForm {...props} />;
    case "http-reaction":
    case "http":
    case "http-adaptive":
      return <HttpReactionForm {...props} />;
    case "sse":
      return <SseReactionForm {...props} />;
    case "grpc-reaction":
    case "grpc":
    case "grpc-adaptive":
      return <GrpcReactionForm {...props} />;
    case "profiler":
      return <ProfilerReactionForm {...props} />;
    case "platform-reaction":
    case "platform":
      return <PlatformReactionForm {...props} />;
    default:
      return <p className="text-drasi-text-secondary">Unknown reaction kind: {props.kind}</p>;
  }
}
