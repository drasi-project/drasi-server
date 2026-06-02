import * as vscode from 'vscode';
import * as yaml from 'yaml';
import { SchemaProvider } from './schema-provider';

type ComponentKind = 'Source' | 'Query' | 'Reaction';

interface VariantOption {
  label: string;
  description?: string;
  schema: any;
  kind: string;
}

interface CollectedSchema {
  properties: Record<string, any>;
  required: Set<string>;
}

export class ComponentYamlGenerator {
  private schemaProvider: SchemaProvider;

  constructor(schemaProvider: SchemaProvider) {
    this.schemaProvider = schemaProvider;
  }

  async createSourceYaml() {
    await this.createComponentYaml('Source');
  }

  async createQueryYaml() {
    await this.createComponentYaml('Query');
  }

  async createReactionYaml() {
    await this.createComponentYaml('Reaction');
  }

  private async createComponentYaml(kind: ComponentKind) {
    const schema = await this.schemaProvider.getOrRefreshSchema();
    const definitions = schema?.definitions ?? {};
    if (!schema || Object.keys(definitions).length === 0) {
      vscode.window.showErrorMessage('Drasi schemas are not available yet. Try Refresh Schemas.');
      return;
    }

    const id = await promptForRequiredText(`${kind} id`, `${kind.toLowerCase()}-id`);
    if (!id) {
      return;
    }

    let specSchema: any;
    let specKind: string | undefined;
    if (kind === 'Query') {
      const querySchemaName = findQuerySchemaName(definitions);
      if (!querySchemaName || !definitions[querySchemaName]) {
        vscode.window.showErrorMessage('Query schema was not found in the OpenAPI definitions.');
        return;
      }
      specSchema = definitions[querySchemaName];
    } else {
      const suffix = kind === 'Source' ? 'SourceConfig' : 'ReactionConfig';
      const options = findVariantSchemas(definitions, suffix);
      if (options.length === 0) {
        vscode.window.showErrorMessage(`${kind} schemas were not found in the OpenAPI definitions.`);
        return;
      }
      const picked = await vscode.window.showQuickPick(
        options.map((option) => ({
          label: option.label,
          description: option.description,
          option,
        })),
        { placeHolder: `Select ${kind} kind` }
      );
      if (!picked) {
        return;
      }
      specSchema = picked.option.schema;
      specKind = picked.option.kind;
    }

    const collected = collectSchema(specSchema, definitions);
    const skipFields = new Set(['id', 'kind']);
    const values = await promptForSchemaFields(
      collected,
      definitions,
      skipFields,
      kind
    );
    if (!values) {
      return;
    }

    const spec: Record<string, unknown> = {
      id,
      ...values,
    };
    if (specKind) {
      spec.kind = specKind;
    }

    const resource = {
      kind,
      id,
      spec,
    };

    await insertYaml(resource, spec, kind);
  }
}

async function insertYaml(resource: Record<string, unknown>, spec: Record<string, unknown>, kind: ComponentKind) {
  const yamlText = yaml.stringify(resource).trimEnd();
  const activeEditor = vscode.window.activeTextEditor;
  let editor = activeEditor;

  if (!editor || editor.document.languageId !== 'yaml') {
    const doc = await vscode.workspace.openTextDocument({ language: 'yaml' });
    editor = await vscode.window.showTextDocument(doc);
  }

  if (!editor) {
    return;
  }

  const doc = editor.document;
  const docText = doc.getText();
  const docs = yaml.parseAllDocuments(docText);
  const configIndex = docs.findIndex((entry) => isConfigDocument(entry?.toJS()));
  if (configIndex >= 0) {
    const configDoc = docs[configIndex];
    const path = await resolveInsertionPath(configDoc, kind);
    if (!path) {
      return;
    }
    const current = configDoc.getIn(path);
    if (yaml.isSeq(current)) {
      current.add(spec);
    } else {
      configDoc.setIn(path, [spec]);
    }
    const newText = docs.map((entry) => entry.toString().trimEnd()).join('\n---\n') + '\n';
    await editor.edit((edit) => {
      const fullRange = new vscode.Range(doc.positionAt(0), doc.positionAt(docText.length));
      edit.replace(fullRange, newText);
    });
    return;
  }

  const separator = docText.trim().length > 0 ? '\n---\n' : '';
  const insertText = `${separator}${yamlText}\n`;
  await editor.edit((edit) => {
    const position = editor.selection.active;
    edit.insert(position, insertText);
  });
}

function findQuerySchemaName(definitions: Record<string, any>): string | undefined {
  if (definitions.QueryConfig) {
    return 'QueryConfig';
  }
  if (definitions.QueryConfigDto) {
    return 'QueryConfigDto';
  }
  return Object.entries(definitions).find(([, schema]) => isQuerySchema(schema))?.[0];
}

function findVariantSchemas(definitions: Record<string, any>, suffix: string): VariantOption[] {
  const names = Object.keys(definitions)
    .filter((name) => name.endsWith(suffix) && name !== suffix)
    .filter((name) => !(name.endsWith('Dto') && definitions[name.replace(/Dto$/, '')]));

  return names.map((name) => {
    const schema = resolveSchema(definitions[name], definitions);
    const kind = schema?.properties?.kind?.enum?.[0] ?? schemaNameToKind(name, suffix);
    return {
      label: kind,
      description: name,
      schema,
      kind,
    };
  });
}

function schemaNameToKind(name: string, suffix: string) {
  // Strip dotted namespace prefix (e.g. "source.grpc.GrpcSourceConfig" → "GrpcSourceConfig")
  const lastDot = name.lastIndexOf('.');
  let base = lastDot >= 0 ? name.slice(lastDot + 1) : name;
  if (suffix && base.endsWith(suffix)) {
    base = base.slice(0, -suffix.length);
  }
  if (base.endsWith('Source')) {
    base = base.slice(0, -'Source'.length);
  }
  if (base.endsWith('Reaction')) {
    base = base.slice(0, -'Reaction'.length);
  }
  return base
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/([A-Z]+)([A-Z][a-z0-9])/g, '$1-$2')
    .toLowerCase();
}

function collectSchema(schema: any, definitions: Record<string, any>): CollectedSchema {
  const collected: CollectedSchema = { properties: {}, required: new Set<string>() };
  const visit = (value: any) => {
    const resolved = resolveSchema(value, definitions);
    if (!resolved || typeof resolved !== 'object') {
      return;
    }
    if (Array.isArray(resolved.allOf)) {
      resolved.allOf.forEach((entry: any) => visit(entry));
    }
    const props = resolved.properties ?? {};
    collected.properties = { ...collected.properties, ...props };
    if (Array.isArray(resolved.required)) {
      resolved.required.forEach((field: string) => collected.required.add(field));
    }
  };
  visit(schema);
  return collected;
}

async function promptForSchemaFields(
  collected: CollectedSchema,
  definitions: Record<string, any>,
  skipFields: Set<string>,
  kindLabel: string
) {
  const entries = Object.entries(collected.properties).sort((a, b) => {
    const aRequired = collected.required.has(a[0]);
    const bRequired = collected.required.has(b[0]);
    return Number(bRequired) - Number(aRequired);
  });

  const result: Record<string, unknown> = {};

  for (const [name, schema] of entries) {
    if (skipFields.has(name)) {
      continue;
    }
    const required = collected.required.has(name);
    const value = await promptForValue(name, schema, definitions, required, kindLabel);
    if (value === undefined) {
      if (required) {
        return undefined;
      }
      continue;
    }
    result[name] = value;
  }

  return result;
}

async function promptForValue(
  name: string,
  schema: any,
  definitions: Record<string, any>,
  required: boolean,
  kindLabel: string
): Promise<unknown | undefined> {
  const resolved = resolveSchema(schema, definitions);
  const isConfigValue = isConfigValueSchema(resolved);
  const promptLabel = `${kindLabel} ${name}`;

  if (resolved?.enum) {
    const options = resolved.enum.map((value: string) => ({ label: String(value), value }));
    if (!required) {
      options.unshift({ label: 'Skip', value: undefined });
    }
    const picked = await vscode.window.showQuickPick(options, { placeHolder: promptLabel });
    return picked?.value;
  }

  if (resolved?.type === 'boolean' && !isConfigValue) {
    const options = [
      { label: 'true', value: true },
      { label: 'false', value: false },
    ];
    if (!required) {
      options.unshift({ label: 'Skip', value: undefined });
    }
    const picked = await vscode.window.showQuickPick(options, { placeHolder: promptLabel });
    return picked?.value;
  }

  if (resolved?.type === 'array') {
    return await promptForArrayValue(promptLabel, resolved, definitions, required);
  }

  if (resolved?.type === 'object' && resolved.properties) {
    return await promptForObjectValue(promptLabel, required);
  }

  if ((resolved?.type === 'integer' || resolved?.type === 'number') && !isConfigValue) {
    return await promptForNumber(promptLabel, required, resolved?.default);
  }

  const value = await promptForText(promptLabel, required, resolved?.default);
  if (value === undefined || value === '') {
    return required ? undefined : undefined;
  }

  if (isConfigValue) {
    return parseConfigValue(value, resolved);
  }

  return value;
}

async function promptForArrayValue(
  promptLabel: string,
  schema: any,
  definitions: Record<string, any>,
  required: boolean
): Promise<unknown | undefined> {
  const items = resolveSchema(schema.items, definitions);
  if (items?.properties?.sourceId && Array.isArray(items.required) && items.required.includes('sourceId')) {
    const input = await promptForText(`${promptLabel} (comma-separated source IDs)`, required);
    if (!input) {
      return required ? undefined : undefined;
    }
    return input.split(',').map((entry) => entry.trim()).filter(Boolean).map((sourceId) => ({ sourceId }));
  }

  if (items?.type === 'string' || items?.enum) {
    const input = await promptForText(`${promptLabel} (comma-separated)`, required);
    if (!input) {
      return required ? undefined : undefined;
    }
    return input.split(',').map((entry) => entry.trim()).filter(Boolean);
  }

  const input = await promptForText(`${promptLabel} (JSON array)`, required);
  if (!input) {
    return required ? undefined : undefined;
  }
  try {
    const parsed = JSON.parse(input);
    return Array.isArray(parsed) ? parsed : undefined;
  } catch (_error) {
    vscode.window.showErrorMessage(`Invalid JSON for ${promptLabel}.`);
    return required ? undefined : undefined;
  }
}

async function promptForObjectValue(promptLabel: string, required: boolean): Promise<unknown | undefined> {
  const input = await promptForText(`${promptLabel} (JSON object)`, required);
  if (!input) {
    return required ? undefined : undefined;
  }
  try {
    return JSON.parse(input);
  } catch (_error) {
    vscode.window.showErrorMessage(`Invalid JSON for ${promptLabel}.`);
    return required ? undefined : undefined;
  }
}

async function promptForNumber(
  promptLabel: string,
  required: boolean,
  defaultValue?: number
): Promise<number | undefined> {
  const input = await promptForText(promptLabel, required, defaultValue);
  if (!input) {
    return required ? undefined : undefined;
  }
  const parsed = Number(input);
  if (Number.isNaN(parsed)) {
    vscode.window.showErrorMessage(`${promptLabel} must be a number.`);
    return required ? undefined : undefined;
  }
  return parsed;
}

function parseConfigValue(input: string, schema: any): unknown {
  const trimmed = input.trim();
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    try {
      return JSON.parse(trimmed);
    } catch (_error) {
      return input;
    }
  }
  if (schema?.type === 'integer' || schema?.type === 'number') {
    const parsed = Number(trimmed);
    return Number.isNaN(parsed) ? input : parsed;
  }
  if (schema?.type === 'boolean') {
    if (trimmed.toLowerCase() === 'true') {
      return true;
    }
    if (trimmed.toLowerCase() === 'false') {
      return false;
    }
  }
  return input;
}

async function promptForRequiredText(promptLabel: string, placeholder?: string) {
  return await promptForText(promptLabel, true, placeholder);
}

async function promptForText(promptLabel: string, required: boolean, defaultValue?: unknown): Promise<string | undefined> {
  const value = await vscode.window.showInputBox({
    prompt: promptLabel,
    placeHolder: defaultValue !== undefined ? String(defaultValue) : undefined,
    ignoreFocusOut: true,
  });
  if (value === undefined) {
    return undefined;
  }
  if (!value && required && defaultValue === undefined) {
    vscode.window.showErrorMessage(`${promptLabel} is required.`);
    return undefined;
  }
  if (!value && defaultValue !== undefined) {
    return String(defaultValue);
  }
  return value;
}

function resolveSchema(schema: any, definitions: Record<string, any>): any {
  if (!schema || typeof schema !== 'object') {
    return schema;
  }
  if (schema.$ref && typeof schema.$ref === 'string') {
    const refName = schema.$ref.replace('#/definitions/', '');
    return resolveSchema(definitions[refName], definitions);
  }
  return schema;
}

function isConfigValueSchema(schema: any): boolean {
  if (!schema || !Array.isArray(schema.oneOf)) {
    return false;
  }
  return schema.oneOf.some((entry) => entry?.properties?.kind?.enum?.includes('Secret'));
}

function isQuerySchema(schema: any): boolean {
  const resolved = schema && typeof schema === 'object' ? schema : {};
  const props = resolved.properties ?? {};
  return !!props.query && !!props.id;
}

function kindToConfigKey(kind: ComponentKind) {
  switch (kind) {
    case 'Source':
      return 'sources';
    case 'Query':
      return 'queries';
    case 'Reaction':
      return 'reactions';
  }
}

async function resolveInsertionPath(doc: yaml.Document.Parsed, kind: ComponentKind) {
  const key = kindToConfigKey(kind);
  const config = doc.toJS() as Record<string, any>;
  if (!config || typeof config !== 'object') {
    return undefined;
  }

  if (Array.isArray(config[key]) || (!config.instances && !config[key])) {
    return [key];
  }

  if (Array.isArray(config.instances)) {
    const instances = config.instances;
    if (instances.length === 1) {
      return ['instances', 0, key];
    }
    const picked = await vscode.window.showQuickPick(
      instances.map((instance: any, index: number) => ({
        label: instance?.id ? String(instance.id) : `instance ${index + 1}`,
        description: instance?.id ? undefined : 'Unnamed instance',
        index,
      })),
      { placeHolder: `Select instance for ${key}` }
    );
    if (!picked) {
      return undefined;
    }
    return ['instances', picked.index, key];
  }

  return [key];
}

function isConfigDocument(document: any) {
  if (!document || typeof document !== 'object') {
    return false;
  }
  const keys = [
    'apiVersion',
    'sources',
    'queries',
    'reactions',
    'instances',
    'host',
    'port',
    'logLevel',
    'persistConfig',
    'persistIndex',
    'id',
    'defaultPriorityQueueCapacity',
    'defaultDispatchBufferCapacity',
    'stateStore',
  ];
  return keys.some((key) => Object.prototype.hasOwnProperty.call(document, key));
}
