interface SchemaFormProps {
  schema: Record<string, unknown>;
  values: Record<string, unknown>;
  onChange: (values: Record<string, unknown>) => void;
}

export function SchemaForm({ schema, values, onChange }: SchemaFormProps) {
  const schemaObj = schema as Record<string, Record<string, unknown>>;
  const properties = (schemaObj?.properties ?? {}) as Record<
    string,
    Record<string, unknown>
  >;
  const required = new Set(
    (Array.isArray(schemaObj?.required) ? schemaObj.required : []) as string[],
  );

  return (
    <div className="space-y-3">
      {Object.entries(properties).map(([name, prop]) => (
        <SchemaField
          key={name}
          name={name}
          schema={prop}
          required={required.has(name)}
          value={values[name] ?? ""}
          onChange={(val) => onChange({ ...values, [name]: val })}
        />
      ))}
    </div>
  );
}

function SchemaField({
  name,
  schema,
  required,
  value,
  onChange,
}: {
  name: string;
  schema: Record<string, unknown>;
  required: boolean;
  value: unknown;
  onChange: (val: unknown) => void;
}) {
  const type = (schema.type as string) || "string";
  const description = (schema.description as string) || "";
  const label = `${name}${required ? " *" : ""}`;
  const enumValues = schema.enum as string[] | undefined;

  if (type === "boolean") {
    return (
      <div className="flex items-center justify-between py-2">
        <div>
          <span className="text-sm text-drasi-text-primary">{label}</span>
          {description && (
            <p className="text-[10px] text-drasi-text-secondary">
              {description}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={() => onChange(!value)}
          className={`relative w-10 h-5 rounded-full transition-colors ${
            value ? "bg-drasi-running" : "bg-drasi-border"
          }`}
        >
          <span
            className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
              value ? "translate-x-5" : "translate-x-0.5"
            }`}
          />
        </button>
      </div>
    );
  }

  if (enumValues) {
    return (
      <div className="space-y-1">
        <label className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
          {label}
        </label>
        <select
          value={String(value)}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-drasi-card border border-drasi-border rounded-lg px-3 py-2 text-sm text-drasi-text-primary focus:outline-none focus:ring-1 focus:ring-drasi-source transition-colors"
        >
          <option value="">Select...</option>
          {enumValues.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
        {description && (
          <p className="text-[10px] text-drasi-text-secondary">{description}</p>
        )}
      </div>
    );
  }

  const inputType =
    type === "integer" || type === "number"
      ? "number"
      : (schema.format as string) === "password"
        ? "password"
        : "text";

  const defaultVal = schema.default;

  return (
    <div className="space-y-1">
      <label className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
        {label}
      </label>
      <input
        type={inputType}
        value={String(value ?? "")}
        onChange={(e) =>
          onChange(
            type === "integer"
              ? parseInt(e.target.value) || ""
              : type === "number"
                ? parseFloat(e.target.value) || ""
                : e.target.value,
          )
        }
        placeholder={
          defaultVal !== undefined ? `Default: ${String(defaultVal)}` : ""
        }
        className="w-full bg-drasi-card border border-drasi-border rounded-lg px-3 py-2 text-sm text-drasi-text-primary placeholder:text-drasi-text-secondary/50 focus:outline-none focus:ring-1 focus:ring-drasi-source transition-colors"
      />
      {description && (
        <p className="text-[10px] text-drasi-text-secondary">{description}</p>
      )}
    </div>
  );
}

export default SchemaForm;
