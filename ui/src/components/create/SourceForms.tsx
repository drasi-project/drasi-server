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
          for (const [key, val] of Object.entries(data)) {
            if (key !== "id" && key !== "autoStart") onChange(key, val);
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
