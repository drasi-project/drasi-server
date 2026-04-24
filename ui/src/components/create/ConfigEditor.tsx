import { useMemo, useState, useEffect } from "react";
import { usePluginSchema } from "@/hooks/usePluginSchema";
import { resolvePluginSchema, generateYamlTemplate } from "@/utils/schemaResolver";
import YamlConfigEditor, {
  formDataToYaml,
  yamlToFormData,
} from "./YamlConfigEditor";

interface ConfigEditorProps {
  category: string;
  kind: string;
  formData: Record<string, unknown>;
  onChange: (data: Record<string, unknown>) => void;
}

export default function ConfigEditor({
  category,
  kind,
  formData,
  onChange,
}: ConfigEditorProps) {
  const { schema: rawSchema, loading, error } = usePluginSchema(category, kind);
  const [yamlText, setYamlText] = useState<string>("");
  const [initialized, setInitialized] = useState(false);

  const resolved = useMemo(() => {
    if (!rawSchema) return null;
    return resolvePluginSchema({
      kind: rawSchema.kind,
      category: rawSchema.category,
      schema: rawSchema.schema,
    });
  }, [rawSchema]);

  // Initialize YAML text from formData or generate template from schema
  useEffect(() => {
    if (initialized || !resolved) return;

    const configKeys = Object.keys(formData).filter(
      (k) => k !== "id" && k !== "autoStart" && k !== "kind",
    );

    if (configKeys.length > 0) {
      const configOnly: Record<string, unknown> = {};
      for (const k of configKeys) {
        configOnly[k] = formData[k];
      }
      setYamlText(formDataToYaml(configOnly));
    } else {
      setYamlText(generateYamlTemplate(resolved.jsonSchema));
    }
    setInitialized(true);
  }, [resolved, formData, initialized]);

  if (loading) {
    return (
      <p className="text-sm text-drasi-text-secondary p-2">Loading schema…</p>
    );
  }

  if (error || !resolved) {
    return (
      <p className="text-sm text-drasi-text-secondary p-2">
        {error
          ? `Could not load schema for "${kind}": ${error}`
          : `No schema available for "${kind}".`}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between px-1">
        <span className="text-[10px] font-semibold text-drasi-text-secondary uppercase tracking-wider">
          Plugin Configuration
        </span>
      </div>
      <YamlConfigEditor
        schema={resolved.jsonSchema}
        value={yamlText}
        onChange={(text) => {
          setYamlText(text);
          const parsed = yamlToFormData(text);
          if (Object.keys(parsed).length > 0) {
            onChange(parsed);
          }
        }}
      />
    </div>
  );
}
