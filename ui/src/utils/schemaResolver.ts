/**
 * Takes the raw API schema response and produces a self-contained JSON Schema
 * suitable for RJSF and monaco-yaml.
 *
 * - Resolves all $ref references by inlining the referenced schemas into `definitions`
 * - Identifies the root config schema (the one matching *Config pattern)
 * - Replaces ConfigValue $refs with simple string type (with description about env vars)
 * - Adds titles to oneOf variants based on discriminator enum values
 * - Returns a single JSON Schema object with the root properties at top level
 */
export function resolvePluginSchema(apiResponse: {
  kind: string;
  category: string;
  schema: Record<string, unknown>;
}): { jsonSchema: Record<string, unknown>; rootSchemaName: string } | null {
  const { schema, kind, category } = apiResponse;
  if (!schema || typeof schema !== "object") return null;

  const schemaMap = schema as Record<string, Record<string, unknown>>;

  // Find root schema — matches pattern like "source.mock.MockSourceConfig"
  const prefix = `${category}.${kind}`.toLowerCase();
  const rootKey = Object.keys(schemaMap).find((k) => {
    const lower = k.toLowerCase();
    return (
      (lower.startsWith(prefix) && lower.endsWith("config")) ||
      lower.includes(`${kind.toLowerCase()}config`)
    );
  }) ?? Object.keys(schemaMap).find((k) => k.endsWith("Config"));

  if (!rootKey) return null;

  const rootSchema = JSON.parse(
    JSON.stringify(schemaMap[rootKey]),
  ) as Record<string, unknown>;

  // Build definitions from all non-root schemas
  const definitions: Record<string, unknown> = {};
  for (const [name, def] of Object.entries(schemaMap)) {
    if (name !== rootKey) {
      definitions[name] = JSON.parse(JSON.stringify(def));
    }
  }

  // Add titles to oneOf variants based on discriminator enum values
  addOneOfTitles(definitions);

  // Rewrite $ref paths inside definitions themselves (they can cross-reference)
  for (const [name, def] of Object.entries(definitions)) {
    definitions[name] = rewriteRefs(def, definitions);
  }

  // Rewrite $ref paths in the root schema
  const rewritten = rewriteRefs(rootSchema, definitions);

  return {
    jsonSchema: {
      ...(rewritten as Record<string, unknown>),
      definitions,
    },
    rootSchemaName: rootKey,
  };
}

/**
 * Generate a YAML template string from a resolved JSON Schema.
 * Includes YAML comments with property descriptions, types, and constraints.
 * Pre-populates required fields with example values.
 */
export function generateYamlTemplate(
  jsonSchema: Record<string, unknown>,
): string {
  const lines: string[] = [];
  const props = jsonSchema.properties as Record<string, Record<string, unknown>> | undefined;
  const defs = jsonSchema.definitions as Record<string, Record<string, unknown>> | undefined;
  const required = new Set(
    Array.isArray(jsonSchema.required) ? (jsonSchema.required as string[]) : [],
  );

  if (!props) return "# No configuration properties\n";

  for (const [name, prop] of Object.entries(props)) {
    const isRequired = required.has(name);
    const desc = (prop.description as string) || "";
    const resolved = prop.$ref && defs ? resolveLocalRef(prop.$ref as string, defs) : prop;

    // Add comment with description
    if (desc) {
      lines.push(`# ${desc}`);
    }

    // Handle oneOf (discriminated union)
    if (resolved.oneOf && Array.isArray(resolved.oneOf)) {
      const discriminator = resolved.discriminator as Record<string, unknown> | undefined;
      const discProp = (discriminator?.propertyName as string) || "type";
      const variants = (resolved.oneOf as Record<string, unknown>[])
        .map((v) => {
          const vProps = v.properties as Record<string, Record<string, unknown>> | undefined;
          if (!vProps?.[discProp]?.enum) return null;
          return (vProps[discProp].enum as string[])[0];
        })
        .filter(Boolean);

      if (variants.length > 0) {
        lines.push(`# Options: ${variants.join(", ")}`);
        lines.push(`${name}:`);
        lines.push(`  ${discProp}: ${variants[0]}`);
        // Add other properties from the first variant
        const firstVariant = (resolved.oneOf as Record<string, unknown>[])[0];
        const fProps = firstVariant?.properties as Record<string, Record<string, unknown>> | undefined;
        if (fProps) {
          for (const [fp, fv] of Object.entries(fProps)) {
            if (fp === discProp) continue;
            const example = getExampleValue(fv);
            if (fv.description) {
              lines.push(`  # ${fv.description as string}`);
            }
            lines.push(`  ${fp}: ${example}`);
          }
        }
      } else {
        lines.push(`${name}: {}`);
      }
    } else if (resolved.type === "object" && resolved.additionalProperties) {
      // Map type
      lines.push(`# Key-value map`);
      lines.push(`${name}: {}`);
    } else if (resolved.type === "object" && resolved.properties) {
      // Nested object
      lines.push(`${name}:`);
      const nestedProps = resolved.properties as Record<string, Record<string, unknown>>;
      for (const [np, nv] of Object.entries(nestedProps)) {
        const nDesc = (nv.description as string) || "";
        if (nDesc) lines.push(`  # ${nDesc}`);
        lines.push(`  ${np}: ${getExampleValue(nv)}`);
      }
    } else {
      // Simple property
      const typeHint = getTypeHint(resolved);
      if (typeHint && !desc) {
        lines.push(`# ${typeHint}`);
      }
      const value = getExampleValue(resolved);
      if (isRequired) {
        lines.push(`${name}: ${value}  # required`);
      } else {
        lines.push(`# ${name}: ${value}`);
      }
    }
    lines.push("");
  }

  return lines.join("\n");
}

function resolveLocalRef(ref: string, defs: Record<string, Record<string, unknown>>): Record<string, unknown> {
  const name = ref.replace("#/definitions/", "");
  return defs[name] || { type: "object" };
}

function getExampleValue(prop: Record<string, unknown>): string {
  if (prop.default !== undefined) return JSON.stringify(prop.default);
  if (prop.enum && Array.isArray(prop.enum)) return JSON.stringify(prop.enum[0]);

  switch (prop.type) {
    case "string":
      return prop.format === "password" ? '""' : '""';
    case "integer":
    case "number":
      return prop.minimum !== undefined ? String(prop.minimum) : "0";
    case "boolean":
      return "false";
    case "array":
      return "[]";
    case "object":
      return "{}";
    default:
      return '""';
  }
}

function getTypeHint(prop: Record<string, unknown>): string {
  const parts: string[] = [];
  if (prop.type) parts.push(`Type: ${prop.type as string}`);
  if (prop.format) parts.push(`Format: ${prop.format as string}`);
  if (prop.minimum !== undefined) parts.push(`Min: ${prop.minimum}`);
  if (prop.maximum !== undefined) parts.push(`Max: ${prop.maximum}`);
  if (prop.enum && Array.isArray(prop.enum)) parts.push(`Values: ${(prop.enum as string[]).join(", ")}`);
  return parts.join(" | ");
}

/**
 * Walk all definitions and add `title` to each `oneOf` variant
 * based on the discriminator property's enum value.
 */
function addOneOfTitles(definitions: Record<string, unknown>): void {
  for (const def of Object.values(definitions)) {
    if (typeof def !== "object" || def === null) continue;
    const record = def as Record<string, unknown>;

    const discriminator = record.discriminator as Record<string, unknown> | undefined;
    const oneOf = record.oneOf as Record<string, unknown>[] | undefined;

    if (discriminator && oneOf && Array.isArray(oneOf)) {
      const propName = discriminator.propertyName as string;
      if (!propName) continue;

      for (const variant of oneOf) {
        if (typeof variant !== "object" || variant === null) continue;
        const props = (variant as Record<string, unknown>).properties as Record<string, Record<string, unknown>> | undefined;
        if (!props || !props[propName]) continue;

        const enumValues = props[propName].enum as string[] | undefined;
        if (enumValues && enumValues.length === 1) {
          (variant as Record<string, unknown>).title = enumValues[0];
        }
      }
    }
  }
}

function rewriteRefs(
  obj: unknown,
  definitions: Record<string, unknown>,
): unknown {
  if (typeof obj !== "object" || obj === null) return obj;

  const record = obj as Record<string, unknown>;

  // Handle $ref
  if (typeof record.$ref === "string") {
    const refName = record.$ref.replace("#/components/schemas/", "");

    // Handle ConfigValue — replace with string type
    if (refName === "ConfigValue" || refName.includes("ConfigValue")) {
      return {
        type: "string",
        description:
          ((record.description as string) || "") +
          " (Supports ${ENV_VAR:-default} syntax)",
      };
    }

    // If the referenced schema exists in definitions, rewrite the ref path
    if (refName in definitions) {
      return { $ref: `#/definitions/${refName}` };
    }

    // Unknown ref — replace with a generic object/string schema so RJSF doesn't crash
    // This happens when the plugin schema references types defined elsewhere in the
    // server's OpenAPI spec (e.g., TemplateSpecDto, QueryConfigDto).
    const shortName = refName.split(".").pop() || refName;
    return {
      type: "object",
      title: shortName,
      description: `Configuration for ${shortName}`,
      additionalProperties: true,
    };
  }

  if (Array.isArray(obj)) {
    return obj.map((item) => rewriteRefs(item, definitions));
  }

  // Handle allOf wrapping pattern (used for nullable fields):
  // { "allOf": [{ "$ref": "..." }], "nullable": true }
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(record)) {
    if (key === "allOf" && Array.isArray(value)) {
      // If allOf has a single $ref entry, unwrap it
      if (
        value.length === 1 &&
        typeof value[0] === "object" &&
        value[0] !== null &&
        "$ref" in value[0]
      ) {
        const resolved = rewriteRefs(value[0], definitions);
        // Merge the resolved ref with any sibling properties (like nullable, description)
        const siblings: Record<string, unknown> = {};
        for (const [sk, sv] of Object.entries(record)) {
          if (sk !== "allOf") siblings[sk] = sv;
        }
        return { ...siblings, ...(resolved as Record<string, unknown>) };
      }
      result[key] = value.map((item) => rewriteRefs(item, definitions));
    } else {
      result[key] = rewriteRefs(value, definitions);
    }
  }
  return result;
}
