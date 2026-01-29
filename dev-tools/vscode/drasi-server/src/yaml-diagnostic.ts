import * as vscode from 'vscode';
import * as yaml from 'yaml';
import Ajv from 'ajv';

export class DrasiYamlDiagnosticProvider {
  private diagnosticCollection: vscode.DiagnosticCollection;
  private ajv: Ajv;
  private validator: ((data: any) => boolean) | undefined;

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
    vscode.workspace.textDocuments.forEach((doc) => this.validateDocument(doc));
  }

  private validateDocument(document: vscode.TextDocument) {
    if (document.languageId !== 'yaml') {
      return;
    }

    if (!this.isDrasiFile(document.fileName.toLowerCase())) {
      return;
    }

    if (!this.validator) {
      return;
    }

    const diagnostics: vscode.Diagnostic[] = [];
    const content = document.getText();

    try {
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
          const valid = this.validator(item.payload);
          if (!valid && this.validator.errors) {
            for (const error of this.validator.errors) {
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

  private isDrasiFile(fileName: string): boolean {
    return fileName.includes('query') ||
      fileName.includes('source') ||
      fileName.includes('reaction') ||
      fileName.includes('drasi') ||
      fileName.includes('resources');
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
  const items: Array<{ payload: any; kindLabel: string }> = [];

  if (document.kind && document.id) {
    items.push({ payload: document, kindLabel: `${document.kind} ${document.id}` });
  }

  if (document.sources || document.queries || document.reactions) {
    for (const source of document.sources ?? []) {
      if (source?.id) {
        items.push({ payload: { kind: 'Source', ...source }, kindLabel: `Source ${source.id}` });
      }
    }
    for (const query of document.queries ?? []) {
      if (query?.id) {
        items.push({ payload: { kind: 'Query', ...query }, kindLabel: `Query ${query.id}` });
      }
    }
    for (const reaction of document.reactions ?? []) {
      if (reaction?.id) {
        items.push({ payload: { kind: 'Reaction', ...reaction }, kindLabel: `Reaction ${reaction.id}` });
      }
    }
  }

  return items;
}
