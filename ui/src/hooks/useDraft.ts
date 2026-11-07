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
  inputs: [],
  enableBootstrap: true,
};

// Names are required; UUIDs are optional and assigned by the instance by default.
// Schema validation handles field-level requirements.
const REQUIRED_FIELDS: Record<string, string[]> = {
  // Queries
  query: ["name", "query", "inputs"],
  reaction: ["name", "queries"],
};

function getDefaults(
  componentType: "source" | "query" | "reaction",
  kind: string,
): Record<string, unknown> {
  if (componentType === "source") return SOURCE_DEFAULTS[kind] ?? { autoStart: true };
  if (componentType === "reaction") return REACTION_DEFAULTS[kind] ?? { autoStart: true };
  return {
    ...QUERY_DEFAULTS,
    queryLanguage: kind === "gql" ? "GQL" : "Cypher",
  };
}

function validate(
  kind: string,
  fields: Record<string, unknown>,
): Record<string, string> {
  const required = REQUIRED_FIELDS[kind] ?? ["name"];
  const errors: Record<string, string> = {};
  for (const key of required) {
    const val = fields[key];
    if (val === undefined || val === null || val === "") {
      errors[key] = "Required";
    } else if (Array.isArray(val) && val.length === 0) {
      errors[key] = "At least one required";
    }
    if (typeof fields.name === "string" && /[\s\p{Cc}]/u.test(fields.name)) {
      errors.name = "Names cannot contain whitespace or control characters";
    }
    if (fields.id && (typeof fields.id !== "string"
      || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(fields.id))) {
      errors.id = "Enter a UUID, or leave blank for an assigned identity";
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
        errors: validate(prev.componentType, newFields),
      };
    });
  }, []);

  const isValid = useCallback((): boolean => {
    if (!draft) return false;
    const errors = validate(draft.componentType, draft.fields);
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
