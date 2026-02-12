import * as vscode from 'vscode';
import axios from 'axios';
import { ConnectionRegistry } from './sdk/config';
import { DrasiYamlDiagnosticProvider } from './yaml-diagnostic';
import { isDrasiYaml } from './drasi-yaml';

const SCHEMA_FILE_PREFIX = 'drasi-resources.schema';
const SCHEMA_FILE_NAME = `${SCHEMA_FILE_PREFIX}.json`;
const SCHEMA_FILE_STATE_KEY = 'drasiServer.schemaFileName';
const SOURCE_SCHEMA_SUFFIX = 'SourceConfig';
const REACTION_SCHEMA_SUFFIX = 'ReactionConfig';
const QUERY_SCHEMA_NAME = 'QueryConfig';
const BOOTSTRAP_SCHEMA_NAME = 'BootstrapProviderConfig';
const BOOTSTRAP_SCHEMA_NAME_LEGACY = 'BootstrapProviderConfigDto';
const BOOTSTRAP_CONFIG_SUFFIX = 'BootstrapConfig';
const BOOTSTRAP_CONFIG_SUFFIX_LEGACY = 'BootstrapConfigDto';

export class SchemaProvider {
  private registry: ConnectionRegistry;
  private diagnosticProvider?: DrasiYamlDiagnosticProvider;
  private yamlApi?: any;
  private readonly schemaProviderUri = 'drasi-schema';
  private lastSchema?: any;
  private storageUri?: vscode.Uri;
  private globalState?: vscode.Memento;
  private schemaFileName?: string;

  constructor(registry: ConnectionRegistry, diagnosticProvider?: DrasiYamlDiagnosticProvider) {
    this.registry = registry;
    this.diagnosticProvider = diagnosticProvider;
  }

  async activate(context: vscode.ExtensionContext) {
    this.storageUri = context.globalStorageUri;
    this.globalState = context.globalState;
    await this.ensureStorage(context.globalStorageUri);
    await this.loadCachedSchema(context.globalStorageUri);
    await this.registerYamlApi();
    await this.refreshSchemas(context.globalStorageUri);
  }

  async refreshSchemas(storageUri: vscode.Uri, apiVersion?: string) {
    const baseUrl = this.registry.getCurrentConnection().url;
    const version = apiVersion ?? 'v1';
    const previousSchemaFileName = this.getCachedSchemaFileName();
    const schemaFileName = this.buildSchemaFileName();
    const schemaUri = vscode.Uri.joinPath(storageUri, schemaFileName);
    try {
      const openapi = await this.fetchOpenApi(baseUrl, version);
      const schema = buildUnionSchema(openapi);
      this.lastSchema = schema;
      await vscode.workspace.fs.writeFile(schemaUri, new TextEncoder().encode(JSON.stringify(schema, null, 2)));
      await this.updateSchemaFileName(schemaFileName);
      this.registerSchemaContributor(schemaUri);
      await this.configureYamlSchemas(schemaUri);
      this.diagnosticProvider?.updateSchema(schema);
      await this.deleteOldSchemaFile(storageUri, previousSchemaFileName, schemaFileName);
    } catch (error) {
      const message = String(error);
      console.warn(`Failed to refresh schemas: ${message}`);
    }
  }

  async getOrRefreshSchema() {
    if (!this.lastSchema && this.storageUri) {
      await this.refreshSchemas(this.storageUri);
    }
    return this.lastSchema;
  }

  private async fetchOpenApi(baseUrl: string, version: string) {
    const res = await axios.get(`${baseUrl}/api/${version}/openapi.json`, {
      validateStatus: () => true,
      timeout: 10000,
    });
    if (res.status < 200 || res.status >= 300) {
      throw new Error(`Failed to fetch OpenAPI: ${res.status} ${res.statusText}`);
    }
    return res.data;
  }

  private async loadCachedSchema(storageUri: vscode.Uri) {
    const schemaFileName = this.getCachedSchemaFileName();
    const schemaUri = vscode.Uri.joinPath(storageUri, schemaFileName);
    try {
      const data = await vscode.workspace.fs.readFile(schemaUri);
      const schema = JSON.parse(new TextDecoder('utf-8').decode(data));
      this.lastSchema = schema;
      this.schemaFileName = schemaFileName;
      await this.configureYamlSchemas(schemaUri);
      this.diagnosticProvider?.updateSchema(schema);
    } catch (_error) {
      // ignore if no cached schema yet
    }
  }

  private async configureYamlSchemas(schemaUri: vscode.Uri) {
    // Schema association is now handled entirely by the contributor callback
    // which detects drasi files by content (apiVersion: drasi.io/v1)
    this.registerSchemaContributor(schemaUri);
  }

  private async ensureStorage(storageUri: vscode.Uri) {
    try {
      await vscode.workspace.fs.createDirectory(storageUri);
    } catch (_error) {
      // ignore
    }
  }

  private async registerYamlApi() {
    if (this.yamlApi) {
      return;
    }
    const extension = vscode.extensions.getExtension('redhat.vscode-yaml');
    if (!extension) {
      return;
    }
    const api = await extension.activate();
    if (api?.registerContributor) {
      this.yamlApi = api;
    }
  }

  private registerSchemaContributor(schemaUri: vscode.Uri) {
    if (!this.yamlApi?.registerContributor) {
      return;
    }
    const schemaKey = schemaUri.toString();
    const contentUri = `${this.schemaProviderUri}://schema?cache=${this.globalState?.get<string>(`${SCHEMA_FILE_STATE_KEY}.timestamp`) ?? Date.now()}`;
    this.yamlApi.registerContributor(
      this.schemaProviderUri,
      (resource: string) => {
        if (!resource) {
          return undefined;
        }
        try {
          const uri = vscode.Uri.parse(resource);
          const doc = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString());
          if (doc && isDrasiYaml(doc.getText())) {
            return contentUri;
          }
        } catch {
          // ignore
        }
        return undefined;
      },
      (uri: string) => {
        if (uri === contentUri && this.lastSchema) {
          return JSON.stringify(this.lastSchema);
        }
        return undefined;
      },
      'drasi'
    );
  }

  private buildSchemaFileName() {
    return `${SCHEMA_FILE_PREFIX}.${Date.now()}.json`;
  }

  private getCachedSchemaFileName() {
    return this.schemaFileName
      ?? this.globalState?.get<string>(SCHEMA_FILE_STATE_KEY)
      ?? SCHEMA_FILE_NAME;
  }

  private async updateSchemaFileName(schemaFileName: string) {
    this.schemaFileName = schemaFileName;
    await this.globalState?.update(SCHEMA_FILE_STATE_KEY, schemaFileName);
    await this.globalState?.update(`${SCHEMA_FILE_STATE_KEY}.timestamp`, Date.now());
  }

  private async deleteOldSchemaFile(storageUri: vscode.Uri, previousSchemaFileName: string, schemaFileName: string) {
    if (!previousSchemaFileName || previousSchemaFileName === schemaFileName) {
      return;
    }
    try {
      await vscode.workspace.fs.delete(
        vscode.Uri.joinPath(storageUri, previousSchemaFileName),
        { useTrash: false }
      );
    } catch (_error) {
      // ignore if prior schema file does not exist
    }
  }
}

function buildUnionSchema(openapi: any) {
  // YAML language server expects JSON Schema, so normalize OpenAPI-only markers.
  const definitions = normalizeOpenApiSchemas(openapi?.components?.schemas ?? {});
  const convertedDefinitions = convertSchemaRefs(definitions);
  ensureSchemaAliases(convertedDefinitions);
  ensureLogReactionSchemas(convertedDefinitions);
  ensureConfigValueSchema(convertedDefinitions);
  if (!convertedDefinitions.StateStoreConfig) {
    convertedDefinitions.StateStoreConfig = { type: 'object', additionalProperties: true };
  }
  const sourceSchemaNames = findSchemaNamesBySuffix(convertedDefinitions, SOURCE_SCHEMA_SUFFIX);
  const reactionSchemaNames = findSchemaNamesBySuffix(convertedDefinitions, REACTION_SCHEMA_SUFFIX);
  const queryName = convertedDefinitions[QUERY_SCHEMA_NAME]
    ? QUERY_SCHEMA_NAME
    : findSchema(convertedDefinitions, isQuerySchema);

  const sourceUnion = buildKindUnion(
    convertedDefinitions,
    sourceSchemaNames,
    SOURCE_SCHEMA_SUFFIX,
    'Source',
    getSourceCommonProperties(convertedDefinitions),
    ['id'],
    (schema, kind) => applySourceBootstrapSchema(schema, kind, convertedDefinitions),
  );
  const reactionUnion = buildKindUnion(
    convertedDefinitions,
    reactionSchemaNames,
    REACTION_SCHEMA_SUFFIX,
    'Reaction',
    getReactionCommonProperties(),
    ['id', 'queries'],
  );

  const sourceRef = sourceUnion ? { $ref: '#/definitions/SourceConfig' } : minimalSourceSchema();
  const reactionRef = reactionUnion ? { $ref: '#/definitions/ReactionConfig' } : minimalReactionSchema();
  const queryRef = queryName ? { $ref: `#/definitions/${queryName}` } : minimalQuerySchema();

  const configSchema = {
    type: 'object',
    properties: {
      sources: {
        type: 'array',
        items: sourceRef,
      },
      queries: {
        type: 'array',
        items: queryRef,
      },
      reactions: {
        type: 'array',
        items: reactionRef,
      },
      host: definitions['ConfigValueString'] ? { $ref: '#/definitions/ConfigValueString' } : { type: 'string' },
      port: definitions['ConfigValueU16'] ? { $ref: '#/definitions/ConfigValueU16' } : { type: 'integer' },
      logLevel: definitions['ConfigValueString'] ? { $ref: '#/definitions/ConfigValueString' } : { type: 'string' },
      persistConfig: { type: 'boolean' },
      persistIndex: { type: 'boolean' },
      id: definitions['ConfigValueString'] ? { $ref: '#/definitions/ConfigValueString' } : { type: 'string' },
      defaultPriorityQueueCapacity: definitions['ConfigValueUsize']
        ? { $ref: '#/definitions/ConfigValueUsize' }
        : { type: 'integer' },
      defaultDispatchBufferCapacity: definitions['ConfigValueUsize']
        ? { $ref: '#/definitions/ConfigValueUsize' }
        : { type: 'integer' },
      stateStore: definitions['StateStoreConfig'] ? { $ref: '#/definitions/StateStoreConfig' } : { type: 'object' },
      instances: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            sources: {
              type: 'array',
              items: sourceRef,
            },
            queries: {
              type: 'array',
              items: queryRef,
            },
            reactions: {
              type: 'array',
              items: reactionRef,
            },
            id: definitions['ConfigValueString'] ? { $ref: '#/definitions/ConfigValueString' } : { type: 'string' },
            persistIndex: { type: 'boolean' },
            stateStore: definitions['StateStoreConfig'] ? { $ref: '#/definitions/StateStoreConfig' } : { type: 'object' },
            defaultPriorityQueueCapacity: definitions['ConfigValueUsize']
              ? { $ref: '#/definitions/ConfigValueUsize' }
              : { type: 'integer' },
            defaultDispatchBufferCapacity: definitions['ConfigValueUsize']
              ? { $ref: '#/definitions/ConfigValueUsize' }
              : { type: 'integer' },
          },
          additionalProperties: true,
        },
      },
    },
    additionalProperties: true,
  };

  const resourceSchema = {
    type: 'object',
    properties: {
      apiVersion: { type: 'string' },
      kind: { enum: ['Source', 'Query', 'Reaction'] },
      id: { type: 'string' },
      spec: {
        oneOf: [sourceRef, queryRef, reactionRef],
      },
    },
    required: ['kind', 'id', 'spec'],
    additionalProperties: true,
  };

  return {
    $schema: 'http://json-schema.org/draft-07/schema#',
    oneOf: [configSchema, resourceSchema, sourceRef, queryRef, reactionRef],
    definitions: {
      ...convertedDefinitions,
      ...(sourceUnion ? { SourceConfig: sourceUnion } : {}),
      ...(reactionUnion ? { ReactionConfig: reactionUnion } : {}),
    },
  };
}

function findSchemaNamesBySuffix(definitions: Record<string, any>, suffix: string): string[] {
  return Object.keys(definitions).filter((name) => name.endsWith(suffix)).sort();
}

function buildKindUnion(
  definitions: Record<string, any>,
  names: string[],
  suffix: string,
  trimWord: string,
  commonProperties: Record<string, any>,
  commonRequired: string[],
  transformVariant?: (schema: any, kind: string) => any,
) {
  if (names.length === 0) {
    return undefined;
  }

  const kinds = names.map((name) => schemaNameToKind(name, suffix, trimWord));
  const variants = names.map((name) => {
    const baseSchema = definitions[name] ?? {};
    const kind = schemaNameToKind(name, suffix, trimWord);
    let merged = mergeSchema(baseSchema, {
      kind,
      commonProperties,
      commonRequired,
    });
    if (transformVariant) {
      merged = transformVariant(merged, kind);
    }
    return merged;
  });

  return {
    allOf: [
      {
        type: 'object',
        properties: {
          kind: {
            enum: kinds,
          },
        },
        required: ['kind'],
      },
      {
        oneOf: variants,
      },
    ],
  };
}

function mergeSchema(
  schema: Record<string, any>,
  options: {
    kind: string;
    commonProperties: Record<string, any>;
    commonRequired: string[];
  }
) {
  const properties = isObjectSchema(schema) ? (schema.properties ?? {}) : {};
  const required = Array.isArray(schema.required) ? schema.required : [];
  const mergedProperties = {
    ...properties,
    ...options.commonProperties,
    kind: { enum: [options.kind] },
  };
  const mergedRequired = Array.from(new Set([...required, 'kind', ...options.commonRequired]));

  return {
    ...schema,
    type: 'object',
    properties: mergedProperties,
    required: mergedRequired,
  };
}

function schemaNameToKind(name: string, suffix: string, trimWord: string) {
  let base = name.endsWith(suffix) ? name.slice(0, -suffix.length) : name;
  if (trimWord && base.endsWith(trimWord)) {
    base = base.slice(0, -trimWord.length);
  }
  return toKebabCase(base);
}

function getSourceCommonProperties(definitions: Record<string, any>) {
  return {
    id: { type: 'string' },
    autoStart: { type: 'boolean' },
    bootstrapProvider: getBootstrapProviderSchemaRef(definitions),
  };
}

function getReactionCommonProperties() {
  return {
    id: { type: 'string' },
    queries: { type: 'array', items: { type: 'string' } },
    autoStart: { type: 'boolean' },
  };
}

function toKebabCase(value: string) {
  return value
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/([A-Z]+)([A-Z][a-z0-9])/g, '$1-$2')
    .toLowerCase();
}

function findSchema(definitions: Record<string, any>, predicate: (schema: any) => boolean) {
  for (const [name, schema] of Object.entries(definitions)) {
    if (predicate(schema)) {
      return name;
    }
  }
  return undefined;
}

function isObjectSchema(schema: any) {
  return schema && (schema.type === 'object' || schema.properties);
}

function isQuerySchema(schema: any) {
  return isObjectSchema(schema)
    && !!schema.properties?.query
    && !!schema.properties?.id;
}

function applySourceBootstrapSchema(
  schema: any,
  kind: string,
  definitions: Record<string, any>,
) {
  if (!isObjectSchema(schema)) {
    return schema;
  }

  const properties = schema.properties ?? {};
  if (!properties.bootstrapProvider) {
    return schema;
  }

  const bootstrapProvider = buildBootstrapProviderSchemaForSourceKind(definitions, kind);
  if (!bootstrapProvider) {
    return schema;
  }

  return {
    ...schema,
    type: 'object',
    properties: {
      ...properties,
      bootstrapProvider,
    },
  };
}

function buildBootstrapProviderSchemaForSourceKind(
  definitions: Record<string, any>,
  kind: string,
) {
  const bootstrapSchema = getBootstrapProviderSchemaRef(definitions);
  const configName = findBootstrapConfigSchemaName(definitions, kind);
  if (!configName) {
    return bootstrapSchema;
  }

  const relaxed = buildRelaxedBootstrapConfigSchema(definitions, configName, kind);
  return {
    oneOf: [bootstrapSchema, relaxed],
  };
}

function buildRelaxedBootstrapConfigSchema(
  definitions: Record<string, any>,
  schemaName: string,
  kind: string,
) {
  const baseSchema = resolveDefinitionSchema(definitions, definitions[schemaName]);
  const properties = isObjectSchema(baseSchema) ? (baseSchema.properties ?? {}) : {};

  return {
    ...baseSchema,
    type: 'object',
    properties: {
      ...properties,
      kind: { enum: [kind] },
    },
    required: ['kind'],
  };
}

function resolveDefinitionSchema(definitions: Record<string, any>, schema: any) {
  if (!schema || typeof schema !== 'object') {
    return schema;
  }

  let current = schema;
  const seen = new Set<any>();
  while (current?.$ref && typeof current.$ref === 'string') {
    if (seen.has(current)) {
      break;
    }
    seen.add(current);
    const refName = current.$ref.replace('#/definitions/', '');
    const resolved = definitions[refName];
    if (!resolved) {
      break;
    }
    current = resolved;
  }
  return current;
}

function getBootstrapProviderSchemaRef(definitions: Record<string, any>) {
  if (definitions[BOOTSTRAP_SCHEMA_NAME]) {
    return { $ref: `#/definitions/${BOOTSTRAP_SCHEMA_NAME}` };
  }
  if (definitions[BOOTSTRAP_SCHEMA_NAME_LEGACY]) {
    return { $ref: `#/definitions/${BOOTSTRAP_SCHEMA_NAME_LEGACY}` };
  }
  return { type: 'object' };
}

function findBootstrapConfigSchemaName(definitions: Record<string, any>, kind: string) {
  const names = [
    ...findSchemaNamesBySuffix(definitions, BOOTSTRAP_CONFIG_SUFFIX),
    ...findSchemaNamesBySuffix(definitions, BOOTSTRAP_CONFIG_SUFFIX_LEGACY),
  ];

  for (const name of names) {
    const suffix = name.endsWith(BOOTSTRAP_CONFIG_SUFFIX)
      ? BOOTSTRAP_CONFIG_SUFFIX
      : BOOTSTRAP_CONFIG_SUFFIX_LEGACY;
    const schemaKind = schemaNameToKind(name, suffix, '');
    if (schemaKind === kind) {
      return name;
    }
  }

  return undefined;
}

function minimalSourceSchema() {
  return {
    type: 'object',
    properties: {
      id: { type: 'string' },
      kind: { type: 'string' },
    },
    required: ['id', 'kind'],
  };
}

function minimalReactionSchema() {
  return {
    type: 'object',
    properties: {
      id: { type: 'string' },
      kind: { type: 'string' },
      queries: { type: 'array', items: { type: 'string' } },
    },
    required: ['id', 'queries'],
  };
}

function minimalQuerySchema() {
  return {
    type: 'object',
    properties: {
      id: { type: 'string' },
      query: { type: 'string' },
    },
    required: ['id', 'query'],
  };
}

function normalizeOpenApiSchemas(value: any): any {
  return normalizeOpenApiSchemasInner(value, new WeakMap());
}

function normalizeOpenApiSchemasInner(value: any, cache: WeakMap<object, any>): any {
  if (Array.isArray(value)) {
    return value.map((entry) => normalizeOpenApiSchemasInner(entry, cache));
  }
  if (!value || typeof value !== 'object') {
    return value;
  }
  const cached = cache.get(value);
  if (cached) {
    return cached;
  }
  const result: Record<string, any> = {};
  cache.set(value, result);
  for (const [key, entry] of Object.entries(value)) {
    if (key === 'nullable') {
      continue;
    }
    result[key] = normalizeOpenApiSchemasInner(entry, cache);
  }

  if (result.$ref && Object.keys(result).length > 1) {
    return {
      allOf: [{ $ref: result.$ref }, ...Object.entries(result).filter(([k]) => k !== '$ref').map(([, v]) => v)],
    };
  }

  // Unwrap single-element allOf that contains only a $ref (leftover from nullable stripping)
  if (result.allOf && Array.isArray(result.allOf) && result.allOf.length === 1
      && Object.keys(result).length === 1) {
    const inner = result.allOf[0];
    if (inner && typeof inner === 'object' && inner.$ref && Object.keys(inner).length === 1) {
      return inner;
    }
  }

  return result;
}

function convertSchemaRefs(value: any): any {
  return convertSchemaRefsInner(value, new WeakMap());
}

function convertSchemaRefsInner(value: any, cache: WeakMap<object, any>): any {
  if (Array.isArray(value)) {
    return value.map((entry) => convertSchemaRefsInner(entry, cache));
  }
  if (!value || typeof value !== 'object') {
    return value;
  }
  const cached = cache.get(value);
  if (cached) {
    return cached;
  }
  const result: Record<string, any> = {};
  cache.set(value, result);
  for (const [key, entry] of Object.entries(value)) {
    if (key === '$ref' && typeof entry === 'string') {
      result[key] = entry.replace('#/components/schemas/', '#/definitions/');
      continue;
    }
    result[key] = convertSchemaRefsInner(entry, cache);
  }
  return result;
}

function ensureConfigValueSchema(definitions: Record<string, any>) {
  const configSchemas = [
    { name: 'ConfigValueString', schema: buildConfigValueSchema({ type: 'string' }) },
    { name: 'ConfigValueU16', schema: buildConfigValueSchema({ type: 'integer', minimum: 0, maximum: 65535 }) },
    { name: 'ConfigValueU32', schema: buildConfigValueSchema({ type: 'integer', minimum: 0 }) },
    { name: 'ConfigValueU64', schema: buildConfigValueSchema({ type: 'integer', minimum: 0 }) },
    { name: 'ConfigValueUsize', schema: buildConfigValueSchema({ type: 'integer', minimum: 0 }) },
    { name: 'ConfigValueBool', schema: buildConfigValueSchema({ type: 'boolean' }) },
    { name: 'ConfigValueSslMode', schema: buildConfigValueSchema({ type: 'string', enum: ['disable', 'prefer', 'require'] }) },
  ];

  for (const entry of configSchemas) {
    const existing = definitions[entry.name];
    if (!existing || isSelfRefSchema(existing, entry.name)) {
      definitions[entry.name] = entry.schema;
    }
  }

  const available = configSchemas.map((entry) => entry.name).filter((name) => definitions[name]);
  if (available.length === 0) {
    return;
  }

  if (!definitions.ConfigValue || isSelfRefSchema(definitions.ConfigValue, 'ConfigValue')) {
    definitions.ConfigValue = {
      oneOf: available.map((name) => ({ $ref: `#/definitions/${name}` })),
    };
  }
}

function ensureSchemaAliases(definitions: Record<string, any>) {
  const aliases = [
    ['PostgresBootstrapConfigDto', 'PostgresBootstrapConfig'],
    ['ApplicationBootstrapConfigDto', 'ApplicationBootstrapConfig'],
    ['ScriptFileBootstrapConfigDto', 'ScriptFileBootstrapConfig'],
    ['PlatformBootstrapConfigDto', 'PlatformBootstrapConfig'],
    ['MockSourceConfigDto', 'MockSourceConfig'],
    ['HttpSourceConfigDto', 'HttpSourceConfig'],
    ['GrpcSourceConfigDto', 'GrpcSourceConfig'],
    ['PostgresSourceConfigDto', 'PostgresSourceConfig'],
    ['PlatformSourceConfigDto', 'PlatformSourceConfig'],
    ['TableKeyConfigDto', 'TableKeyConfig'],
    ['SslModeDto', 'SslMode'],
    ['LogReactionConfigDto', 'LogReactionConfig'],
    ['LogQueryConfigDto', 'LogQueryConfig'],
    ['TemplateSpecDto', 'LogTemplateSpec'],
    ['HttpReactionConfigDto', 'HttpReactionConfig'],
    ['HttpAdaptiveReactionConfigDto', 'HttpAdaptiveReactionConfig'],
    ['HttpQueryConfigDto', 'HttpQueryConfig'],
    ['CallSpecDto', 'CallSpec'],
    ['GrpcReactionConfigDto', 'GrpcReactionConfig'],
    ['GrpcAdaptiveReactionConfigDto', 'GrpcAdaptiveReactionConfig'],
    ['SseReactionConfigDto', 'SseReactionConfig'],
    ['SseQueryConfigDto', 'SseQueryConfig'],
    ['SseTemplateSpecDto', 'SseTemplateSpec'],
    ['PlatformReactionConfigDto', 'PlatformReactionConfig'],
    ['ProfilerReactionConfigDto', 'ProfilerReactionConfig'],
    ['QueryConfigDto', 'QueryConfig'],
    ['SourceSubscriptionConfigDto', 'SourceSubscriptionConfig'],
    ['RedbStateStoreConfigDto', 'RedbStateStoreConfig'],
  ];

  for (const [legacy, current] of aliases) {
    if (!definitions[legacy] && definitions[current]) {
      definitions[legacy] = { $ref: `#/definitions/${current}` };
    }
    if (!definitions[current] && definitions[legacy]) {
      definitions[current] = { $ref: `#/definitions/${legacy}` };
    }
  }
}

function ensureLogReactionSchemas(definitions: Record<string, any>) {
  const logReaction = resolveLogReactionSchema(definitions);
  if (!logReaction) {
    return;
  }

  const templateName = ensureLogTemplateSpec(definitions);
  const queryName = ensureLogQueryConfig(definitions, templateName);
  const queryRef = { $ref: `#/definitions/${queryName}` };

  if (logReaction.properties.defaultTemplate) {
    logReaction.properties.defaultTemplate = queryRef;
  }

  const routes = logReaction.properties.routes;
  if (routes?.additionalProperties) {
    routes.additionalProperties = queryRef;
  }
}

function resolveLogReactionSchema(definitions: Record<string, any>) {
  const logReaction = definitions.LogReactionConfig;
  if (logReaction?.properties) {
    return logReaction;
  }
  const logReactionDto = definitions.LogReactionConfigDto;
  if (logReactionDto?.properties) {
    return logReactionDto;
  }
  if (logReaction?.$ref === '#/definitions/LogReactionConfigDto' && logReactionDto?.properties) {
    return logReactionDto;
  }
  return undefined;
}

function ensureLogTemplateSpec(definitions: Record<string, any>) {
  if (definitions.LogTemplateSpec) {
    return 'LogTemplateSpec';
  }
  if (definitions.TemplateSpecDto) {
    definitions.LogTemplateSpec = { $ref: '#/definitions/TemplateSpecDto' };
    return 'LogTemplateSpec';
  }
  definitions.LogTemplateSpec = {
    type: 'object',
    properties: {
      template: { type: 'string' },
    },
    additionalProperties: false,
  };
  return 'LogTemplateSpec';
}

function ensureLogQueryConfig(definitions: Record<string, any>, templateName: string) {
  if (definitions.LogQueryConfig) {
    return 'LogQueryConfig';
  }
  if (definitions.LogQueryConfigDto) {
    definitions.LogQueryConfig = { $ref: '#/definitions/LogQueryConfigDto' };
    return 'LogQueryConfig';
  }
  const templateRef = { $ref: `#/definitions/${templateName}` };
  definitions.LogQueryConfig = {
    type: 'object',
    properties: {
      added: templateRef,
      updated: templateRef,
      deleted: templateRef,
    },
    additionalProperties: false,
  };
  return 'LogQueryConfig';
}

function buildConfigValueSchema(staticSchema: Record<string, any>) {
  const defaultSchema = staticSchema.type === 'string'
    ? { type: 'string' }
    : { anyOf: [{ type: 'string' }, staticSchema] };
  return {
    oneOf: [
      staticSchema,
      {
        type: 'string',
        pattern: '^\\$\\{[A-Z0-9_]+(?::-[^}]+)?\\}$',
      },
      {
        type: 'object',
        properties: {
          kind: { enum: ['Secret', 'EnvironmentVariable'] },
          name: { type: 'string' },
          default: defaultSchema,
        },
        required: ['kind', 'name'],
        additionalProperties: false,
      },
    ],
  };
}

function isSelfRefSchema(schema: any, name: string) {
  if (!schema || typeof schema !== 'object') {
    return false;
  }
  const keys = Object.keys(schema);
  if (keys.length !== 1 || !schema.$ref) {
    return false;
  }
  return schema.$ref === `#/definitions/${name}` || schema.$ref === `#/components/schemas/${name}`;
}
