import * as vscode from 'vscode';
import * as yaml from 'yaml';
import Ajv from 'ajv';
import { parseDrasiApiVersion } from './drasi-yaml';

export class DrasiYamlDiagnosticProvider {
  private diagnosticCollection: vscode.DiagnosticCollection;
  private ajv: Ajv;
  private validator: ((data: any) => boolean) | undefined;
  private configValidator: ((data: any) => boolean) | undefined;

  constructor() {
    this.diagnosticCollection = vscode.languages.createDiagnosticCollection('drasi-yaml');
    this.ajv = new Ajv({ strict: false, allErrors: true });
  }

  activate(context: vscode.ExtensionContext) {
    context.subscriptions.push(this.diagnosticCollection);

    context.subscriptions.push(
      vscode.workspace.onDidOpenTextDocument((doc) => this.validateDocument(doc)),
      vscode.workspace.onDidChangeTextDocument((e) => this.validateDocument(e.document)),
      vscode.workspace.onDidCloseTextDocument((doc) => this.diagnosticCollection.delete(doc.uri))
    );

    vscode.workspace.textDocuments.forEach((doc) => this.validateDocument(doc));
  }

  updateSchema(schema: any) {
    this.validator = this.ajv.compile(schema);
    const configSchema = findConfigSchema(schema);
    this.configValidator = configSchema ? this.ajv.compile(configSchema) : undefined;
    vscode.workspace.textDocuments.forEach((doc) => this.validateDocument(doc));
  }

  private validateDocument(document: vscode.TextDocument) {
    if (document.languageId !== 'yaml') {
      return;
    }

    if (!this.isDrasiFile(document)) {
      return;
    }

    if (!this.validator) {
      return;
    }

    const diagnostics: vscode.Diagnostic[] = [];
    const content = document.getText();

    try {
      this.diagnosticCollection.delete(document.uri);
      const docs = yaml.parseAllDocuments(content);
      let currentLine = 0;

      for (const doc of docs) {
        const obj = doc.toJS();
        if (!obj || typeof obj !== 'object') {
          currentLine += doc.toString().split('\n').length;
          continue;
        }

        const validatedItems = extractDrasiItems(obj);
        for (const item of validatedItems) {
          const activeValidator = item.kindLabel === 'config' && this.configValidator
            ? this.configValidator
            : this.validator;
          if (!activeValidator) {
            continue;
          }
          const valid = activeValidator(item.payload);
          if (!valid && activeValidator.errors) {
            for (const error of activeValidator.errors) {
              if (error.keyword === 'additionalProperties') {
                continue;
              }
              if (item.kindLabel === 'config' && error.keyword === 'oneOf') {
                continue;
              }
              // Skip "must be string" errors when the value is a number (YAML coercion)
              if (error.keyword === 'type' && error.params?.type === 'string') {
                continue;
              }
              const diagnostic = this.createDiagnostic(document, doc, error, currentLine, item.kindLabel);
              if (diagnostic) {
                diagnostics.push(diagnostic);
              }
            }
          }
        }

        currentLine += doc.toString().split('\n').length;
      }
    } catch (_error) {
      // ignore parse errors - YAML extension handles them
    }

    this.diagnosticCollection.set(document.uri, diagnostics);
  }

  private isDrasiFile(document: vscode.TextDocument): boolean {
    if (document.languageId !== 'yaml') {
      return false;
    }
    return parseDrasiApiVersion(document.getText()) !== undefined;
  }

  private createDiagnostic(
    document: vscode.TextDocument,
    doc: yaml.Document,
    error: any,
    baseLineOffset: number,
    kindLabel: string
  ): vscode.Diagnostic | null {
    try {
      const errorPath = error.instancePath.split('/').filter((p: string) => p);
      let range = new vscode.Range(baseLineOffset, 0, baseLineOffset, 0);

      if (errorPath.length > 0) {
        const docText = doc.toString();
        const lines = docText.split('\n');

        for (let i = 0; i < lines.length; i++) {
          const line = lines[i];
          const key = errorPath[errorPath.length - 1];
          if (line.includes(`${key}:`)) {
            range = new vscode.Range(
              baseLineOffset + i, 0,
              baseLineOffset + i, line.length
            );
            break;
          }
        }
      }

      let message = error.message;
      if (error.params) {
        if (error.params.allowedValues) {
          message += ` (allowed: ${error.params.allowedValues.join(', ')})`;
        }
        if (error.params.missingProperty) {
          message = `Missing required property: ${error.params.missingProperty}`;
        }
      }

      if (kindLabel) {
        message = `${kindLabel}: ${message}`;
      }

      return new vscode.Diagnostic(
        range,
        message,
        vscode.DiagnosticSeverity.Error
      );
    } catch (_error) {
      return null;
    }
  }
}

function extractDrasiItems(document: any) {
  if (document && isConfigDocument(document)) {
    return [{ payload: document, kindLabel: 'config' }];
  }
  return [{ payload: document, kindLabel: '' }];
}

function isConfigDocument(document: any) {
  const keys = [
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

function findConfigSchema(schema: any) {
  if (!schema) {
    return undefined;
  }
  if (schema.definitions?.DrasiServerConfig) {
    return {
      $schema: schema.$schema,
      definitions: schema.definitions,
      $ref: '#/definitions/DrasiServerConfig',
    };
  }
  if (!Array.isArray(schema.oneOf)) {
    return undefined;
  }
  const configSchema = schema.oneOf.find((item: any) => item?.properties?.sources && item?.properties?.queries);
  if (!configSchema) {
    return undefined;
  }
  return {
    ...schema,
    oneOf: [configSchema],
  };
}
