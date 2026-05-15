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
        label="Source ID"
        field="id"
        value={fields.id}
        onChange={onChange}
        error={errors.id}
        required
        placeholder={`my-${kind}-source`}
      />
      <ConfigEditor
        category="source"
        kind={kind}
        formData={fields}
        onChange={(data) => {
          // Atomic replacement: clear old config keys, apply new ones
          const configKeys = Object.keys(fields).filter(
            (k) => k !== "id" && k !== "autoStart" && k !== "kind",
          );
          for (const key of configKeys) {
            if (!(key in data)) onChange(key, undefined);
          }
          for (const [key, val] of Object.entries(data)) {
            if (key !== "id" && key !== "autoStart" && key !== "kind") onChange(key, val);
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
