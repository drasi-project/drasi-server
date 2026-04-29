import { useMemo, useState, useEffect, useCallback } from "react";
import { FileText, FormInput } from "lucide-react";
import { usePluginSchema } from "@/hooks/usePluginSchema";
import { resolvePluginSchema, generateYamlTemplate } from "@/utils/schemaResolver";
import { extractUiSchema } from "@/utils/uiSchemaMapper";
import YamlConfigEditor, {
  formDataToYaml,
  yamlToFormData,
} from "./YamlConfigEditor";
import SchemaConfigForm from "./SchemaConfigForm";

type EditorMode = "form" | "yaml";

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
  const [mode, setMode] = useState<EditorMode>("form");
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

  const uiSchema = useMemo(() => {
    if (!resolved) return undefined;
    return extractUiSchema(resolved.jsonSchema);
  }, [resolved]);

  // Extract config-only fields (exclude id, autoStart, kind, queries)
  const configOnly = useMemo(() => {
    const result: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(formData)) {
      if (k !== "id" && k !== "autoStart" && k !== "kind" && k !== "queries") {
        result[k] = v;
      }
    }
    return result;
  }, [formData]);

  // Initialize YAML text from formData or generate template from schema
  useEffect(() => {
    if (initialized || !resolved) return;

    if (Object.keys(configOnly).length > 0) {
      setYamlText(formDataToYaml(configOnly));
    } else {
      setYamlText(generateYamlTemplate(resolved.jsonSchema));
    }
    setInitialized(true);
  }, [resolved, configOnly, initialized]);

  // Sync YAML text when switching to YAML mode
  const switchToYaml = useCallback(() => {
    setYamlText(formDataToYaml(configOnly));
    setMode("yaml");
  }, [configOnly]);

  // Sync form data when switching to form mode
  const switchToForm = useCallback(() => {
    const parsed = yamlToFormData(yamlText);
    if (Object.keys(parsed).length > 0) {
      onChange(parsed);
    }
    setMode("form");
  }, [yamlText, onChange]);

  const handleFormChange = useCallback(
    (data: Record<string, unknown>) => {
      onChange(data);
    },
    [onChange],
  );

  const handleYamlChange = useCallback(
    (text: string) => {
      setYamlText(text);
      const parsed = yamlToFormData(text);
      if (Object.keys(parsed).length > 0) {
        onChange(parsed);
      }
    },
    [onChange],
  );

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
        <div className="flex items-center gap-1 bg-drasi-surface border border-drasi-border rounded-lg p-0.5">
          <button
            type="button"
            onClick={() => (mode === "yaml" ? switchToForm() : undefined)}
            className={`flex items-center gap-1 px-2 py-1 rounded-md text-[10px] font-medium transition-colors ${
              mode === "form"
                ? "bg-drasi-card text-drasi-text-primary shadow-sm"
                : "text-drasi-text-secondary hover:text-drasi-text-primary"
            }`}
          >
            <FormInput className="w-3 h-3" />
            Form
          </button>
          <button
            type="button"
            onClick={() => (mode === "form" ? switchToYaml() : undefined)}
            className={`flex items-center gap-1 px-2 py-1 rounded-md text-[10px] font-medium transition-colors ${
              mode === "yaml"
                ? "bg-drasi-card text-drasi-text-primary shadow-sm"
                : "text-drasi-text-secondary hover:text-drasi-text-primary"
            }`}
          >
            <FileText className="w-3 h-3" />
            YAML
          </button>
        </div>
      </div>

      {mode === "form" ? (
        <SchemaConfigForm
          schema={resolved.jsonSchema as Record<string, unknown>}
          uiSchema={uiSchema}
          formData={configOnly}
          onChange={handleFormChange}
        />
      ) : (
        <YamlConfigEditor
          schema={resolved.jsonSchema}
          value={yamlText}
          onChange={handleYamlChange}
        />
      )}
    </div>
  );
}
