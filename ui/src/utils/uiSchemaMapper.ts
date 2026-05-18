import type { UiSchema } from "@rjsf/utils";

/**
 * Condition for conditional visibility of a field.
 */
export interface UiCondition {
  field: string;
  value?: unknown;
  notEmpty?: boolean;
}

/**
 * Represents a group of fields for section rendering.
 */
export interface FieldGroup {
  name: string;
  collapsed: boolean;
  fields: string[]; // field names in display order
}

/**
 * Extract x-ui:* extensions from a JSON Schema and produce an RJSF uiSchema.
 *
 * Walks `schema.properties` (and recursively into nested objects/definitions)
 * looking for `x-ui:*` keys. Maps them to RJSF uiSchema format.
 *
 * Also sets `ui:submitButtonOptions: { norender: true }` at the root level.
 */
export function extractUiSchema(schema: Record<string, unknown>): UiSchema {
  const uiSchema: UiSchema = {
    "ui:submitButtonOptions": { norender: true },
  };

  const properties = schema.properties as
    | Record<string, Record<string, unknown>>
    | undefined;
  if (!properties) return uiSchema;

  for (const [propName, propSchema] of Object.entries(properties)) {
    const fieldUi = extractFieldUi(propSchema, schema);
    if (fieldUi && Object.keys(fieldUi).length > 0) {
      uiSchema[propName] = fieldUi;
    }
  }

  // Generate ui:order from x-ui:group + x-ui:order annotations
  const groups = extractFieldGroups(schema);
  if (groups.length > 0) {
    const orderedFields: string[] = [];
    for (const group of groups) {
      orderedFields.push(...group.fields);
    }
    // Append wildcard so unlisted fields still render
    orderedFields.push("*");
    uiSchema["ui:order"] = orderedFields;
  }

  return uiSchema;
}

/**
 * Analyze schema properties for x-ui:group annotations and produce
 * an ordered list of field groups.
 *
 * Fields without a group are collected into an implicit "" (empty) group
 * that renders first without a section header.
 *
 * Within each group, fields are ordered by x-ui:order (ascending),
 * then by their order in the schema properties object.
 */
export function extractFieldGroups(
  schema: Record<string, unknown>,
): FieldGroup[] {
  const properties = schema.properties as
    | Record<string, Record<string, unknown>>
    | undefined;
  if (!properties) return [];

  const groupMap = new Map<
    string,
    { collapsed: boolean; fields: { name: string; order: number; index: number }[] }
  >();
  const groupOrder: string[] = [];

  let index = 0;
  for (const [propName, propSchema] of Object.entries(properties)) {
    const resolved = resolveProperty(propSchema, schema);
    const groupName = (resolved["x-ui:group"] as string) ?? "";
    const order = (resolved["x-ui:order"] as number) ?? Infinity;
    const collapsed = (resolved["x-ui:collapsed"] as boolean) ?? false;

    if (!groupMap.has(groupName)) {
      groupMap.set(groupName, { collapsed: false, fields: [] });
      groupOrder.push(groupName);
    }

    const group = groupMap.get(groupName)!;
    group.fields.push({ name: propName, order, index });
    if (collapsed) {
      group.collapsed = true;
    }

    index++;
  }

  // Sort fields within each group by order, then by original index
  for (const group of groupMap.values()) {
    group.fields.sort((a, b) => {
      if (a.order !== b.order) return a.order - b.order;
      return a.index - b.index;
    });
  }

  // Build result: ungrouped ("") first, then named groups in order of first appearance
  const result: FieldGroup[] = [];

  // Put the ungrouped fields first if they exist
  if (groupMap.has("")) {
    const ungrouped = groupMap.get("")!;
    result.push({
      name: "",
      collapsed: ungrouped.collapsed,
      fields: ungrouped.fields.map((f) => f.name),
    });
  }

  for (const groupName of groupOrder) {
    if (groupName === "") continue;
    const group = groupMap.get(groupName)!;
    result.push({
      name: groupName,
      collapsed: group.collapsed,
      fields: group.fields.map((f) => f.name),
    });
  }

  return result;
}

/**
 * Extract UI annotations from a single property schema, returning
 * RJSF-compatible uiSchema entries for that field.
 */
function extractFieldUi(
  propSchema: Record<string, unknown>,
  rootSchema: Record<string, unknown>,
): UiSchema | null {
  const resolved = resolveProperty(propSchema, rootSchema);
  const ui: UiSchema = {};

  if (resolved["x-ui:widget"] !== undefined) {
    ui["ui:widget"] = resolved["x-ui:widget"] as string;
  }

  if (resolved["x-ui:placeholder"] !== undefined) {
    ui["ui:placeholder"] = resolved["x-ui:placeholder"] as string;
  }

  if (resolved["x-ui:condition"] !== undefined) {
    ui["x-ui:condition"] = resolved["x-ui:condition"] as UiCondition;
  }

  if (resolved["x-ui:group"] !== undefined) {
    ui["x-ui:group"] = resolved["x-ui:group"] as string;
  }

  if (resolved["x-ui:order"] !== undefined) {
    ui["x-ui:order"] = resolved["x-ui:order"] as number;
  }

  if (resolved["x-ui:collapsed"] !== undefined) {
    ui["x-ui:collapsed"] = resolved["x-ui:collapsed"] as boolean;
  }

  // Recurse into nested object properties
  if (resolved.type === "object" && resolved.properties) {
    const nestedProps = resolved.properties as Record<
      string,
      Record<string, unknown>
    >;
    for (const [nestedName, nestedSchema] of Object.entries(nestedProps)) {
      const nestedUi = extractFieldUi(nestedSchema, rootSchema);
      if (nestedUi && Object.keys(nestedUi).length > 0) {
        ui[nestedName] = nestedUi;
      }
    }
  }

  return Object.keys(ui).length > 0 ? ui : null;
}

/**
 * Resolve a property schema by following $ref if present, merging
 * x-ui:* extensions from both the referencing object and the definition.
 */
function resolveProperty(
  propSchema: Record<string, unknown>,
  rootSchema: Record<string, unknown>,
): Record<string, unknown> {
  if (!propSchema.$ref || typeof propSchema.$ref !== "string") {
    return propSchema;
  }

  const definitions = (rootSchema.definitions ??
    rootSchema.$defs) as Record<string, Record<string, unknown>> | undefined;
  if (!definitions) return propSchema;

  const refName = (propSchema.$ref as string).replace("#/definitions/", "").replace("#/$defs/", "");
  const resolved = definitions[refName];
  if (!resolved) return propSchema;

  // Merge: properties on the referencing object override the definition
  return { ...resolved, ...propSchema, $ref: undefined };
}
