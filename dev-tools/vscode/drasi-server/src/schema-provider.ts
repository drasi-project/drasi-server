import * as vscode from 'vscode';
import axios from 'axios';
import { ConnectionRegistry } from './sdk/config';
import { DrasiYamlDiagnosticProvider } from './yaml-diagnostic';

const SCHEMA_FILE_NAME = 'drasi-resources.schema.json';

export class SchemaProvider {
  private registry: ConnectionRegistry;
  private diagnosticProvider?: DrasiYamlDiagnosticProvider;

  constructor(registry: ConnectionRegistry, diagnosticProvider?: DrasiYamlDiagnosticProvider) {
    this.registry = registry;
    this.diagnosticProvider = diagnosticProvider;
  }

  async activate(context: vscode.ExtensionContext) {
    await this.ensureStorage(context.globalStorageUri);
    await this.loadCachedSchema(context.globalStorageUri);
    await this.refreshSchemas(context.globalStorageUri);
  }

  async refreshSchemas(storageUri: vscode.Uri) {
    const baseUrl = this.registry.getCurrentConfig().url;
    const schemaUri = vscode.Uri.joinPath(storageUri, SCHEMA_FILE_NAME);
    try {
      const openapi = await this.fetchOpenApi(baseUrl);
      const schema = buildUnionSchema(openapi);
      await vscode.workspace.fs.writeFile(schemaUri, new TextEncoder().encode(JSON.stringify(schema, null, 2)));
      await this.configureYamlSchemas(schemaUri);
      this.diagnosticProvider?.updateSchema(schema);
    } catch (error) {
      const message = String(error);
      console.warn(`Failed to refresh schemas: ${message}`);
    }
  }

  private async fetchOpenApi(baseUrl: string) {
    const res = await axios.get(`${baseUrl}/api/v1/openapi.json`, {
      validateStatus: () => true,
      timeout: 10000,
    });
    if (res.status < 200 || res.status >= 300) {
      throw new Error(`Failed to fetch OpenAPI: ${res.status} ${res.statusText}`);
    }
    return res.data;
  }

  private async loadCachedSchema(storageUri: vscode.Uri) {
    const schemaUri = vscode.Uri.joinPath(storageUri, SCHEMA_FILE_NAME);
    try {
      const data = await vscode.workspace.fs.readFile(schemaUri);
      const schema = JSON.parse(new TextDecoder('utf-8').decode(data));
      await this.configureYamlSchemas(schemaUri);
      this.diagnosticProvider?.updateSchema(schema);
    } catch (_error) {
      // ignore if no cached schema yet
    }
  }

  private async configureYamlSchemas(schemaUri: vscode.Uri) {
    const config = vscode.workspace.getConfiguration('yaml');
    const existingSchemas = config.get<Record<string, string[]>>('schemas') ?? {};
    const schemas = { ...existingSchemas };
    schemas[schemaUri.toString()] = [
      '**/*query*.yaml',
      '**/*query*.yml',
      '**/*source*.yaml',
      '**/*source*.yml',
      '**/*reaction*.yaml',
      '**/*reaction*.yml',
      '**/resources.yaml',
      '**/resources.yml',
      '**/*drasi*.yaml',
      '**/*drasi*.yml'
    ];
    await config.update('schemas', schemas, vscode.ConfigurationTarget.Workspace);
  }

  private async ensureStorage(storageUri: vscode.Uri) {
    try {
      await vscode.workspace.fs.createDirectory(storageUri);
    } catch (_error) {
      // ignore
    }
  }
}

function buildUnionSchema(openapi: any) {
  const definitions = openapi?.components?.schemas ?? {};
  const sourceName = findSchema(definitions, isSourceSchema);
  const reactionName = findSchema(definitions, isReactionSchema);
  const queryName = findSchema(definitions, isQuerySchema);

  const sourceRef = sourceName ? { $ref: `#/definitions/${sourceName}` } : minimalSourceSchema();
  const reactionRef = reactionName ? { $ref: `#/definitions/${reactionName}` } : minimalReactionSchema();
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
    oneOf: [resourceSchema, sourceRef, queryRef, reactionRef, configSchema],
    definitions,
  };
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

function isSourceSchema(schema: any) {
  return isObjectSchema(schema)
    && !!schema.properties?.kind
    && !!schema.properties?.id
    && !schema.properties?.queries
    && !schema.properties?.query;
}

function isReactionSchema(schema: any) {
  return isObjectSchema(schema)
    && !!schema.properties?.queries
    && !!schema.properties?.id;
}

function isQuerySchema(schema: any) {
  return isObjectSchema(schema)
    && !!schema.properties?.query
    && !!schema.properties?.id;
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
