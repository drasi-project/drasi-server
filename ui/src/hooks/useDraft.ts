import { useState, useCallback } from "react";

export interface DraftState {
  componentType: "source" | "query" | "reaction";
  kind: string;
  fields: Record<string, unknown>;
  errors: Record<string, string>;
  dirty: boolean;
  saving: boolean;
}

const SOURCE_DEFAULTS: Record<string, Record<string, unknown>> = {
  mock: { autoStart: true, intervalMs: 1000 },
  http: { autoStart: true, host: "0.0.0.0", port: 9000, timeoutMs: 10000 },
  grpc: { autoStart: true, host: "0.0.0.0", port: 50051, timeoutMs: 5000 },
  postgres: {
    autoStart: true,
    host: "localhost",
    port: 5432,
    sslMode: "prefer",
  },
  platform: { autoStart: true },
};

const REACTION_DEFAULTS: Record<string, Record<string, unknown>> = {
  log: { autoStart: true },
  http: { autoStart: true, timeoutMs: 5000 },
  "http-adaptive": {
    autoStart: true,
    timeoutMs: 5000,
    adaptiveMinBatchSize: 10,
    adaptiveMaxBatchSize: 100,
    adaptiveBatchTimeoutMs: 1000,
  },
  grpc: { autoStart: true, timeoutMs: 5000 },
  "grpc-adaptive": {
    autoStart: true,
    timeoutMs: 5000,
    adaptiveMinBatchSize: 10,
    adaptiveMaxBatchSize: 100,
  },
  sse: {
    autoStart: true,
    host: "0.0.0.0",
    port: 8081,
    ssePath: "/events",
    heartbeatIntervalMs: 30000,
  },
  platform: { autoStart: true },
  profiler: { autoStart: true },
};

const QUERY_DEFAULTS: Record<string, unknown> = {
  autoStart: true,
  queryLanguage: "Cypher",
  query: "",
  sources: [],
  enableBootstrap: true,
};

const REQUIRED_FIELDS: Record<string, string[]> = {
  // Sources
  mock: ["id"],
  http: ["id"],
  grpc: ["id"],
  postgres: ["id", "host", "port", "database", "user", "password"],
  platform: ["id"],
  // Reactions
  log: ["id", "queries"],
  "http-reaction": ["id", "queries", "baseUrl"],
  "http-adaptive": ["id", "queries", "baseUrl"],
  "grpc-reaction": ["id", "queries", "endpoint"],
  "grpc-adaptive": ["id", "queries", "endpoint"],
  sse: ["id", "queries"],
  "platform-reaction": ["id", "queries"],
  profiler: ["id", "queries"],
  // Queries
  query: ["id", "query", "sources"],
};

function getDefaults(
  componentType: "source" | "query" | "reaction",
  kind: string,
): Record<string, unknown> {
  if (componentType === "source") return SOURCE_DEFAULTS[kind] ?? {};
  if (componentType === "reaction") return REACTION_DEFAULTS[kind] ?? {};
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
