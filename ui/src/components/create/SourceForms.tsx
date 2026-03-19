import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import FormField from "./FormField";

interface SourceFormProps {
  kind: string;
  fields: Record<string, unknown>;
  errors: Record<string, string>;
  onChange: (field: string, value: unknown) => void;
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

function MockSourceForm({ fields, errors, onChange }: SourceFormProps) {
  return (
    <>
      <FormField
        label="Source ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder="my-mock-source"
      />
      <FormField
        label="Interval (ms)"
        field="intervalMs"
        value={fields.intervalMs}
        onChange={onChange}
        type="number"
        placeholder="1000"
        helpText="Milliseconds between generated events"
      />
      <FormField
        label="Auto Start"
        field="autoStart"
        value={fields.autoStart}
        onChange={onChange}
        type="toggle"
      />
    </>
  );
}

function HttpSourceForm({ fields, errors, onChange }: SourceFormProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <>
      <FormField
        label="Source ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder="my-http-source"
      />
      <FormField
        label="Host"
        field="host"
        value={fields.host}
        onChange={onChange}
        placeholder="0.0.0.0"
      />
      <FormField
        label="Port"
        field="port"
        value={fields.port}
        onChange={onChange}
        type="number"
        placeholder="9000"
      />
      <FormField
        label="Auto Start"
        field="autoStart"
        value={fields.autoStart}
        onChange={onChange}
        type="toggle"
      />
      <AdvancedToggle
        open={showAdvanced}
        onToggle={() => setShowAdvanced(!showAdvanced)}
      />
      {showAdvanced && (
        <>
          <FormField
            label="Timeout (ms)"
            field="timeoutMs"
            value={fields.timeoutMs}
            onChange={onChange}
            type="number"
            placeholder="10000"
          />
        </>
      )}
    </>
  );
}

function GrpcSourceForm({ fields, errors, onChange }: SourceFormProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <>
      <FormField
        label="Source ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder="my-grpc-source"
      />
      <FormField
        label="Host"
        field="host"
        value={fields.host}
        onChange={onChange}
        placeholder="0.0.0.0"
      />
      <FormField
        label="Port"
        field="port"
        value={fields.port}
        onChange={onChange}
        type="number"
        placeholder="50051"
      />
      <FormField
        label="Auto Start"
        field="autoStart"
        value={fields.autoStart}
        onChange={onChange}
        type="toggle"
      />
      <AdvancedToggle
        open={showAdvanced}
        onToggle={() => setShowAdvanced(!showAdvanced)}
      />
      {showAdvanced && (
        <FormField
          label="Timeout (ms)"
          field="timeoutMs"
          value={fields.timeoutMs}
          onChange={onChange}
          type="number"
          placeholder="5000"
        />
      )}
    </>
  );
}

function PostgresSourceForm({ fields, errors, onChange }: SourceFormProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <>
      <FormField
        label="Source ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder="my-postgres-source"
      />
      <div className="grid grid-cols-2 gap-3">
        <FormField
          label="Host"
          field="host"
          value={fields.host}
          onChange={onChange}
          error={errors.host}
          required
          placeholder="localhost"
        />
        <FormField
          label="Port"
          field="port"
          value={fields.port}
          onChange={onChange}
          error={errors.port}
          required
          type="number"
          placeholder="5432"
        />
      </div>
      <FormField
        label="Database"
        field="database"
        value={fields.database}
        onChange={onChange}
        error={errors.database}
        required
        placeholder="mydb"
      />
      <div className="grid grid-cols-2 gap-3">
        <FormField
          label="User"
          field="user"
          value={fields.user}
          onChange={onChange}
          error={errors.user}
          required
          placeholder="postgres"
        />
        <FormField
          label="Password"
          field="password"
          value={fields.password}
          onChange={onChange}
          error={errors.password}
          required
          type="password"
          placeholder="••••••••"
        />
      </div>
      <FormField
        label="Tables"
        field="tables"
        value={
          Array.isArray(fields.tables)
            ? (fields.tables as string[]).join(", ")
            : fields.tables
        }
        onChange={(f, v) =>
          onChange(
            f,
            String(v)
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean),
          )
        }
        placeholder="public.users, public.orders"
        helpText="Comma-separated list of schema.table"
      />
      <FormField
        label="Auto Start"
        field="autoStart"
        value={fields.autoStart}
        onChange={onChange}
        type="toggle"
      />
      <AdvancedToggle
        open={showAdvanced}
        onToggle={() => setShowAdvanced(!showAdvanced)}
      />
      {showAdvanced && (
        <>
          <FormField
            label="Slot Name"
            field="slotName"
            value={fields.slotName}
            onChange={onChange}
            placeholder="drasi_slot"
          />
          <FormField
            label="Publication Name"
            field="publicationName"
            value={fields.publicationName}
            onChange={onChange}
            placeholder="drasi_publication"
          />
          <FormField
            label="SSL Mode"
            field="sslMode"
            value={fields.sslMode}
            onChange={onChange}
            placeholder="prefer"
            helpText="prefer, disable, or require"
          />
        </>
      )}
    </>
  );
}

function PlatformSourceForm({ fields, errors, onChange }: SourceFormProps) {
  return (
    <>
      <FormField
        label="Source ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder="my-platform-source"
      />
      <FormField
        label="Auto Start"
        field="autoStart"
        value={fields.autoStart}
        onChange={onChange}
        type="toggle"
      />
    </>
  );
}

export default function SourceForm(props: SourceFormProps) {
  switch (props.kind) {
    case "mock":
      return <MockSourceForm {...props} />;
    case "http":
      return <HttpSourceForm {...props} />;
    case "grpc":
      return <GrpcSourceForm {...props} />;
    case "postgres":
      return <PostgresSourceForm {...props} />;
    case "platform":
      return <PlatformSourceForm {...props} />;
    default:
      return <p className="text-drasi-text-secondary">Unknown source kind: {props.kind}</p>;
  }
}
