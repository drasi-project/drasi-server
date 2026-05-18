import { useState } from "react";
import type {
  FieldTemplateProps,
  ObjectFieldTemplateProps,
  ArrayFieldTemplateProps,
  ErrorListProps,
} from "@rjsf/utils";
import { ChevronDown, ChevronRight, Plus } from "lucide-react";

/** Convert camelCase/PascalCase field names to human-readable "Title Case" labels. */
function humanizeLabel(raw: string): string {
  // Insert space before uppercase letters that follow lowercase letters or digits
  const spaced = raw
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2");
  // Capitalize each word
  return spaced
    .split(" ")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function FieldTemplate(props: FieldTemplateProps) {
  const { id, label, required, children, rawErrors, rawDescription, schema, uiSchema, registry } = props;

  // Support x-ui:condition — hide field if condition not met
  const condition = uiSchema?.["x-ui:condition"] as
    | { field: string; value?: unknown; notEmpty?: boolean }
    | undefined;
  if (condition && registry.formContext) {
    const ctx = registry.formContext as Record<string, unknown>;
    const fieldVal = ctx[condition.field];
    if (condition.notEmpty) {
      if (fieldVal === undefined || fieldVal === null || fieldVal === "") {
        return null;
      }
    } else if (fieldVal !== condition.value) {
      return null;
    }
  }

  // Don't render labels for objects, booleans, or arrays
  // (CheckboxWidget renders its own label; ArrayFieldTemplate renders its own title)
  const isObject = schema.type === "object";
  const isBoolean = schema.type === "boolean";
  const isArray = schema.type === "array";
  const showLabel = !isObject && !isBoolean && !isArray && label;
  const displayLabel = showLabel ? humanizeLabel(label) : "";

  return (
    <div className="space-y-1">
      {showLabel && (
        <label
          htmlFor={id}
          className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider"
        >
          {displayLabel}
          {required && <span className="text-drasi-error">*</span>}
        </label>
      )}
      {children}
      {rawErrors && rawErrors.length > 0 && (
        <ul className="space-y-0.5">
          {rawErrors.map((error, i) => (
            <li key={i} className="text-[10px] text-drasi-error">
              {error}
            </li>
          ))}
        </ul>
      )}
      {rawDescription && (!rawErrors || rawErrors.length === 0) && (
        <p className="text-[10px] text-drasi-text-secondary">{rawDescription}</p>
      )}
    </div>
  );
}

interface GroupInfo {
  name: string;
  collapsed: boolean;
  fields: ObjectFieldTemplateProps["properties"];
}

function ObjectFieldTemplate(props: ObjectFieldTemplateProps) {
  const { properties, title, description, uiSchema } = props;

  // Separate fields into ungrouped and grouped
  const ungrouped: ObjectFieldTemplateProps["properties"] = [];
  const groupMap = new Map<string, GroupInfo>();
  const groupOrder: string[] = [];

  for (const prop of properties) {
    const fieldUiSchema = uiSchema?.[prop.name];
    const groupName = fieldUiSchema?.["x-ui:group"] as string | undefined;

    if (groupName) {
      if (!groupMap.has(groupName)) {
        const collapsed = Boolean(fieldUiSchema?.["x-ui:collapsed"]);
        groupMap.set(groupName, { name: groupName, collapsed, fields: [] });
        groupOrder.push(groupName);
      }
      groupMap.get(groupName)!.fields.push(prop);
    } else {
      ungrouped.push(prop);
    }
  }

  return (
    <fieldset className="space-y-3">
      {title && (
        <legend className="text-sm font-semibold text-drasi-text-primary mb-2">
          {title}
        </legend>
      )}
      {description && (
        <p className="text-[10px] text-drasi-text-secondary mb-2">{description}</p>
      )}
      {ungrouped.map((prop) => (
        <div key={prop.name}>{prop.content}</div>
      ))}
      {groupOrder.map((groupName) => {
        const group = groupMap.get(groupName)!;
        return <CollapsibleGroup key={groupName} group={group} />;
      })}
    </fieldset>
  );
}

function CollapsibleGroup({ group }: { group: GroupInfo }) {
  const [collapsed, setCollapsed] = useState(group.collapsed);

  return (
    <div className="border border-drasi-border rounded-lg overflow-hidden">
      <button
        type="button"
        onClick={() => setCollapsed(!collapsed)}
        className="flex items-center gap-2 w-full px-3 py-2 bg-drasi-surface text-sm font-semibold text-drasi-text-primary hover:bg-drasi-card transition-colors"
      >
        {collapsed ? (
          <ChevronRight className="w-3.5 h-3.5 text-drasi-text-secondary" />
        ) : (
          <ChevronDown className="w-3.5 h-3.5 text-drasi-text-secondary" />
        )}
        {group.name}
      </button>
      {!collapsed && (
        <div className="px-3 py-3 space-y-3">
          {group.fields.map((prop) => (
            <div key={prop.name}>{prop.content}</div>
          ))}
        </div>
      )}
    </div>
  );
}

function ArrayFieldTemplate(props: ArrayFieldTemplateProps) {
  const { items, canAdd, onAddClick, title, rawErrors } = props;

  return (
    <div className="space-y-2">
      {title && (
        <span className="flex items-center gap-1 text-xs font-medium text-drasi-text-secondary uppercase tracking-wider">
          {humanizeLabel(title)}
        </span>
      )}
      {items.map((item, index) => (
        <div
          key={index}
          className="p-2 bg-drasi-surface border border-drasi-border rounded-lg"
        >
          {item}
        </div>
      ))}
      {canAdd && (
        <button
          type="button"
          onClick={onAddClick}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-drasi-source border border-drasi-border rounded-lg hover:bg-drasi-card transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          Add item
        </button>
      )}
      {rawErrors && rawErrors.length > 0 && (
        <ul className="space-y-0.5">
          {rawErrors.map((error, i) => (
            <li key={i} className="text-[10px] text-drasi-error">
              {error}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// Hide the default error list — we show errors inline per field
function ErrorListTemplate(_props: ErrorListProps) {
  return null;
}

export { FieldTemplate, ObjectFieldTemplate, ArrayFieldTemplate, ErrorListTemplate };
