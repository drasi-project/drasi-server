import FormField from "./FormField";

interface QueryFormProps {
  fields: Record<string, unknown>;
  errors: Record<string, string>;
  onChange: (field: string, value: unknown) => void;
  availableSources: { id: string; kind: string }[];
}

const TEMPLATES = [
  {
    label: "All nodes",
    query: "MATCH (n) RETURN n",
  },
  {
    label: "Filtered by property",
    query: 'MATCH (n:Label) WHERE n.property > 0 RETURN n',
  },
  {
    label: "Relationship traversal",
    query: "MATCH (a:TypeA)-[:REL]->(b:TypeB) RETURN a, b",
  },
];

export default function QueryForm({
  fields,
  errors,
  onChange,
  availableSources,
}: QueryFormProps) {
  const selectedSources = (fields.sources as string[]) ?? [];

  const toggleSource = (sourceId: string) => {
    const current = [...selectedSources];
    const idx = current.indexOf(sourceId);
    if (idx >= 0) {
      current.splice(idx, 1);
    } else {
      current.push(sourceId);
    }
    onChange("sources", current);
  };

  return (
    <>
      <FormField
        label="Query ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder="my-query"
      />

      {/* Templates */}
      <div className="space-y-1">
        <label className="text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
          Templates
        </label>
        <div className="flex flex-wrap gap-1.5">
          {TEMPLATES.map((t) => (
            <button
              key={t.label}
              type="button"
              onClick={() => onChange("query", t.query)}
              className="px-2 py-1 text-[10px] rounded-md bg-drasi-card border border-drasi-border text-drasi-text-secondary hover:border-drasi-query hover:text-drasi-query transition-colors"
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      {/* Query editor */}
      <FormField
        label="Cypher Query"
        field="query"
        value={fields.query}
        onChange={onChange}
        error={errors.query}
        required
        type="textarea"
        placeholder="MATCH (n:Label) WHERE n.property > 0 RETURN n"
      />

      <FormField
        label="Query Language"
        field="queryLanguage"
        value={fields.queryLanguage}
        onChange={onChange}
        placeholder="Cypher"
        helpText="Cypher or GQL"
      />

      {/* Source selection */}
      <div className="space-y-1">
        <label className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
          Sources <span className="text-drasi-error">*</span>
        </label>
        {errors.sources && (
          <p className="text-[10px] text-drasi-error">{errors.sources}</p>
        )}
        {availableSources.length === 0 ? (
          <p className="text-xs text-drasi-text-secondary italic">
            No sources available. Create a source first.
          </p>
        ) : (
          <div className="space-y-1.5">
            {availableSources.map((src) => (
              <label
                key={src.id}
                className="flex items-center gap-2 p-2 rounded-lg bg-drasi-card border border-drasi-border cursor-pointer hover:border-drasi-text-secondary transition-colors"
              >
                <input
                  type="checkbox"
                  checked={selectedSources.includes(src.id)}
                  onChange={() => toggleSource(src.id)}
                  className="rounded border-drasi-border text-drasi-source focus:ring-drasi-source"
                />
                <span className="text-sm text-drasi-text-primary">
                  {src.id}
                </span>
                <span className="text-[10px] text-drasi-text-secondary ml-auto">
                  {src.kind}
                </span>
              </label>
            ))}
          </div>
        )}
      </div>

      <FormField
        label="Auto Start"
        field="autoStart"
        value={fields.autoStart}
        onChange={onChange}
        type="toggle"
      />

      <FormField
        label="Enable Bootstrap"
        field="enableBootstrap"
        value={fields.enableBootstrap}
        onChange={onChange}
        type="toggle"
        helpText="Load initial data from sources on startup"
      />
    </>
  );
}
