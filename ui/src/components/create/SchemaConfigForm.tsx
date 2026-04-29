import Form, { type IChangeEvent } from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import type { RJSFSchema, UiSchema } from "@rjsf/utils";
import drasiTheme from "./rjsf-theme";

interface SchemaConfigFormProps {
  schema: RJSFSchema;
  uiSchema?: UiSchema;
  formData: Record<string, unknown>;
  onChange: (data: Record<string, unknown>) => void;
}

export default function SchemaConfigForm({
  schema,
  uiSchema,
  formData,
  onChange,
}: SchemaConfigFormProps) {
  const mergedUiSchema: UiSchema = {
    "ui:submitButtonOptions": { norender: true },
    ...uiSchema,
  };

  return (
    <div className="rjsf-drasi space-y-3">
      <Form
        schema={schema}
        uiSchema={mergedUiSchema}
        formData={formData}
        formContext={formData}
        validator={validator}
        widgets={drasiTheme.widgets}
        templates={drasiTheme.templates}
        onChange={(e: IChangeEvent) => {
          if (e.formData) onChange(e.formData as Record<string, unknown>);
        }}
        liveValidate
      />
    </div>
  );
}
