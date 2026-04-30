import type { WidgetProps } from "@rjsf/utils";

const inputBase =
  "w-full bg-drasi-card border rounded-lg px-3 py-2 text-sm text-drasi-text-primary placeholder:text-drasi-text-secondary/50 focus:outline-none focus:ring-1 transition-colors";
const inputNormal = "border-drasi-border focus:ring-drasi-source";
const inputError = "border-drasi-error focus:ring-drasi-error";

function TextWidget(props: WidgetProps) {
  const { id, value, required, disabled, readonly, onChange, onBlur, onFocus, placeholder, rawErrors } = props;
  const hasError = rawErrors && rawErrors.length > 0;

  return (
    <input
      id={id}
      type="text"
      value={value ?? ""}
      required={required}
      disabled={disabled}
      readOnly={readonly}
      placeholder={placeholder}
      className={`${inputBase} ${hasError ? inputError : inputNormal}`}
      onChange={(e) => onChange(e.target.value === "" ? undefined : e.target.value)}
      onBlur={(e) => onBlur(id, e.target.value)}
      onFocus={(e) => onFocus(id, e.target.value)}
    />
  );
}

function PasswordWidget(props: WidgetProps) {
  const { id, value, required, disabled, readonly, onChange, onBlur, onFocus, placeholder, rawErrors } = props;
  const hasError = rawErrors && rawErrors.length > 0;

  return (
    <input
      id={id}
      type="password"
      value={value ?? ""}
      required={required}
      disabled={disabled}
      readOnly={readonly}
      placeholder={placeholder}
      className={`${inputBase} ${hasError ? inputError : inputNormal}`}
      onChange={(e) => onChange(e.target.value === "" ? undefined : e.target.value)}
      onBlur={(e) => onBlur(id, e.target.value)}
      onFocus={(e) => onFocus(id, e.target.value)}
    />
  );
}

function TextareaWidget(props: WidgetProps) {
  const { id, value, required, disabled, readonly, onChange, onBlur, onFocus, placeholder, rawErrors } = props;
  const hasError = rawErrors && rawErrors.length > 0;

  return (
    <textarea
      id={id}
      value={value ?? ""}
      required={required}
      disabled={disabled}
      readOnly={readonly}
      placeholder={placeholder}
      rows={3}
      className={`${inputBase} font-mono ${hasError ? inputError : inputNormal}`}
      onChange={(e) => onChange(e.target.value === "" ? undefined : e.target.value)}
      onBlur={(e) => onBlur(id, e.target.value)}
      onFocus={(e) => onFocus(id, e.target.value)}
    />
  );
}

function SelectWidget(props: WidgetProps) {
  const { id, value, required, disabled, readonly, onChange, onBlur, onFocus, options, rawErrors } = props;
  const hasError = rawErrors && rawErrors.length > 0;
  const { enumOptions } = options;

  return (
    <select
      id={id}
      value={value ?? ""}
      required={required}
      disabled={disabled || readonly}
      className={`${inputBase} ${hasError ? inputError : inputNormal}`}
      onChange={(e) => {
        const selected = e.target.value;
        if (selected === "") {
          onChange(undefined);
          return;
        }
        // Preserve original type from enumOptions
        const match = Array.isArray(enumOptions)
          ? enumOptions.find((o) => String(o.value) === selected)
          : undefined;
        onChange(match ? match.value : selected);
      }}
      onBlur={(e) => onBlur(id, e.target.value)}
      onFocus={(e) => onFocus(id, e.target.value)}
    >
      <option value="">Select...</option>
      {Array.isArray(enumOptions) &&
        enumOptions.map((opt) => (
          <option key={String(opt.value)} value={String(opt.value)}>
            {opt.label}
          </option>
        ))}
    </select>
  );
}

function CheckboxWidget(props: WidgetProps) {
  const { id, value, disabled, readonly, onChange, label } = props;
  const checked = Boolean(value);
  const isDisabled = disabled || readonly;

  return (
    <div className="flex items-center justify-between py-2">
      <span id={`${id}-label`} className="text-sm text-drasi-text-primary">{label}</span>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-labelledby={`${id}-label`}
        disabled={isDisabled}
        onClick={() => onChange(!checked)}
        className={`relative w-10 h-5 rounded-full transition-colors ${
          checked ? "bg-drasi-running" : "bg-drasi-border"
        } ${isDisabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
      >
        <span
          className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
            checked ? "translate-x-5" : "translate-x-0.5"
          }`}
        />
      </button>
    </div>
  );
}

function RangeWidget(props: WidgetProps) {
  const { id, value, disabled, readonly, onChange, schema } = props;
  const min = schema.minimum ?? 0;
  const max = schema.maximum ?? 100;
  const step = schema.multipleOf ?? 1;

  return (
    <div className="flex items-center gap-3">
      <input
        id={id}
        type="range"
        value={value ?? min}
        min={min}
        max={max}
        step={step}
        disabled={disabled || readonly}
        className="flex-1 h-1.5 bg-drasi-border rounded-full appearance-none cursor-pointer accent-drasi-source"
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <span className="text-xs text-drasi-text-secondary tabular-nums min-w-[2.5rem] text-right">
        {value ?? min}
      </span>
    </div>
  );
}

function NumberWidget(props: WidgetProps) {
  const { id, value, required, disabled, readonly, onChange, onBlur, onFocus, placeholder, rawErrors, schema } = props;
  const hasError = rawErrors && rawErrors.length > 0;
  const step = schema.type === "integer" ? 1 : "any";

  return (
    <input
      id={id}
      type="number"
      value={value ?? ""}
      step={step}
      required={required}
      disabled={disabled}
      readOnly={readonly}
      placeholder={placeholder}
      className={`${inputBase} ${hasError ? inputError : inputNormal}`}
      onChange={(e) => {
        const raw = e.target.value;
        if (raw === "") {
          onChange(undefined);
          return;
        }
        const num = schema.type === "integer" ? parseInt(raw, 10) : parseFloat(raw);
        onChange(Number.isNaN(num) ? undefined : num);
      }}
      onBlur={() => onBlur(id, value)}
      onFocus={() => onFocus(id, value)}
    />
  );
}

export {
  TextWidget,
  PasswordWidget,
  TextareaWidget,
  SelectWidget,
  CheckboxWidget,
  RangeWidget,
  NumberWidget,
};
