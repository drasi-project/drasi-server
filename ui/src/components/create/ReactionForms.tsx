import FormField from "./FormField";
import ConfigEditor from "./ConfigEditor";

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

export default function ReactionForm({
  kind,
  fields,
  errors,
  onChange,
  availableQueries,
}: ReactionFormProps) {
  return (
    <>
      <FormField
        label="Reaction ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder={`my-${kind}-reaction`}
      />
      <QueryMultiSelect
        selected={(fields.queries as string[]) ?? []}
        available={availableQueries}
        onChange={onChange}
        error={errors.queries}
      />
      <ConfigEditor
        category="reaction"
        kind={kind}
        formData={fields}
        onChange={(data) => {
          for (const [key, val] of Object.entries(data)) {
            if (key !== "id" && key !== "autoStart" && key !== "queries") {
              onChange(key, val);
            }
          }
        }}
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
