interface FormFieldProps {
  label: string;
  field: string;
  value: unknown;
  onChange: (field: string, value: unknown) => void;
  error?: string;
  required?: boolean;
  type?: "text" | "number" | "password" | "toggle" | "textarea";
  placeholder?: string;
  helpText?: string;
}

export default function FormField({
  label,
  field,
  value,
  onChange,
  error,
  required = false,
  type = "text",
  placeholder,
  helpText,
}: FormFieldProps) {
  if (type === "toggle") {
    return (
      <div className="flex items-center justify-between py-2">
        <div>
          <span className="text-sm text-drasi-text-primary">{label}</span>
          {helpText && (
            <p className="text-[10px] text-drasi-text-secondary">{helpText}</p>
          )}
        </div>
        <button
          type="button"
          onClick={() => onChange(field, !value)}
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

  return (
    <div className="space-y-1">
      <label className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
        {label}
        {required && <span className="text-drasi-error">*</span>}
      </label>
      {type === "textarea" ? (
        <textarea
          value={String(value ?? "")}
          onChange={(e) => onChange(field, e.target.value)}
          placeholder={placeholder}
          rows={3}
          className={`w-full bg-drasi-card border rounded-lg px-3 py-2 text-sm text-drasi-text-primary placeholder:text-drasi-text-secondary/50 focus:outline-none focus:ring-1 transition-colors font-mono ${
            error
              ? "border-drasi-error focus:ring-drasi-error"
              : "border-drasi-border focus:ring-drasi-source"
          }`}
        />
      ) : (
        <input
          type={type === "password" ? "password" : type === "number" ? "number" : "text"}
          value={String(value ?? "")}
          onChange={(e) =>
            onChange(
              field,
              type === "number" ? Number(e.target.value) || 0 : e.target.value,
            )
          }
          placeholder={placeholder}
          className={`w-full bg-drasi-card border rounded-lg px-3 py-2 text-sm text-drasi-text-primary placeholder:text-drasi-text-secondary/50 focus:outline-none focus:ring-1 transition-colors ${
            error
              ? "border-drasi-error focus:ring-drasi-error"
              : "border-drasi-border focus:ring-drasi-source"
          }`}
        />
      )}
      {error && <p className="text-[10px] text-drasi-error">{error}</p>}
      {helpText && !error && (
        <p className="text-[10px] text-drasi-text-secondary">{helpText}</p>
      )}
    </div>
  );
}
