import { useState, useCallback } from "react";

export interface DraftState {
  componentType: "source" | "query" | "reaction";
  kind: string;
  fields: Record<string, unknown>;
  errors: Record<string, string>;
  dirty: boolean;
  saving: boolean;
}

// Defaults are now provided by the plugin schema. Only autoStart is set here.
const SOURCE_DEFAULTS: Record<string, Record<string, unknown>> = {};
const REACTION_DEFAULTS: Record<string, Record<string, unknown>> = {};

const QUERY_DEFAULTS: Record<string, unknown> = {
  autoStart: true,
  queryLanguage: "Cypher",
  query: "",
  sources: [],
  enableBootstrap: true,
};

// All sources and reactions just need "id"; reactions also need "queries".
// Schema validation handles field-level requirements.
const REQUIRED_FIELDS: Record<string, string[]> = {
  // Queries
  query: ["id", "query", "sources"],
};

function getDefaults(
  componentType: "source" | "query" | "reaction",
  kind: string,
): Record<string, unknown> {
  if (componentType === "source") return SOURCE_DEFAULTS[kind] ?? { autoStart: true };
  if (componentType === "reaction") return REACTION_DEFAULTS[kind] ?? { autoStart: true };
  return { ...QUERY_DEFAULTS };
}

function validate(
  kind: string,
  fields: Record<string, unknown>,
): Record<string, string> {
  const required = REQUIRED_FIELDS[kind] ?? ["id"];
  const errors: Record<string, string> = {};
  for (const key of required) {
    const val = fields[key];
    if (val === undefined || val === null || val === "") {
      errors[key] = "Required";
    } else if (Array.isArray(val) && val.length === 0) {
      errors[key] = "At least one required";
    }
  }
  return errors;
}

export function useDraft() {
  const [draft, setDraft] = useState<DraftState | null>(null);

  const startDraft = useCallback(
    (
      componentType: "source" | "query" | "reaction",
      kind: string,
    ) => {
      setDraft({
        componentType,
        kind,
        fields: { ...getDefaults(componentType, kind) },
        errors: {},
        dirty: false,
        saving: false,
      });
    },
    [],
  );

  const updateField = useCallback((field: string, value: unknown) => {
    setDraft((prev) => {
      if (!prev) return prev;
      const newFields = { ...prev.fields, [field]: value };
      return {
        ...prev,
        fields: newFields,
        dirty: true,
        errors: validate(prev.kind, newFields),
      };
    });
  }, []);

  const isValid = useCallback((): boolean => {
    if (!draft) return false;
    const errors = validate(draft.kind, draft.fields);
    setDraft((prev) => (prev ? { ...prev, errors } : prev));
    return Object.keys(errors).length === 0;
  }, [draft]);

  const setSaving = useCallback((saving: boolean) => {
    setDraft((prev) => (prev ? { ...prev, saving } : prev));
  }, []);

  const discard = useCallback(() => {
    setDraft(null);
  }, []);

  return { draft, startDraft, updateField, isValid, setSaving, discard };
}
