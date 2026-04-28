import Form, { type IChangeEvent } from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import type { RJSFSchema, UiSchema } from "@rjsf/utils";

interface SchemaConfigFormProps {
  schema: RJSFSchema;
  formData: Record<string, unknown>;
  onChange: (data: Record<string, unknown>) => void;
}

export default function SchemaConfigForm({
  schema,
  formData,
  onChange,
}: SchemaConfigFormProps) {
  const uiSchema: UiSchema = {
    "ui:submitButtonOptions": { norender: true },
  };

  return (
    <div className="rjsf-drasi">
      <Form
        schema={schema}
        uiSchema={uiSchema}
        formData={formData}
        validator={validator}
        onChange={(e: IChangeEvent) => {
          if (e.formData) onChange(e.formData as Record<string, unknown>);
        }}
        liveValidate
      />
    </div>
  );
}
