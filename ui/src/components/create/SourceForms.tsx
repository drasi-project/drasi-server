import FormField from "./FormField";
import ConfigEditor from "./ConfigEditor";

interface SourceFormProps {
  kind: string;
  fields: Record<string, unknown>;
  errors: Record<string, string>;
  onChange: (field: string, value: unknown) => void;
}

export default function SourceForm({
  kind,
  fields,
  errors,
  onChange,
}: SourceFormProps) {
  return (
    <>
      <FormField
        label="Source name"
        field="name"
        value={fields.name}
        onChange={onChange}
        error={errors.name}
        required
        placeholder={`my-${kind}-source`}
      />
      <FormField label="UUID (optional)" field="id" value={fields.id}
        onChange={onChange} error={errors.id}
        helpText="Leave blank for an assigned UUID. Supplying one requires instance opt-in." />
      <ConfigEditor
        category="source"
        kind={kind}
        formData={fields}
        onChange={(data) => {
          // Atomic replacement: clear old config keys, apply new ones
          const configKeys = Object.keys(fields).filter(
            (k) => k !== "id" && k !== "name" && k !== "autoStart" && k !== "kind",
          );
          for (const key of configKeys) {
            if (!(key in data)) onChange(key, undefined);
          }
          for (const [key, val] of Object.entries(data)) {
            if (key !== "id" && key !== "name" && key !== "autoStart" && key !== "kind") onChange(key, val);
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
