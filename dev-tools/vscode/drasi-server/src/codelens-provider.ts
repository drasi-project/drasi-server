import * as vscode from 'vscode';
import * as yaml from 'yaml';
import { DrasiClient } from './drasi-client';
import { QueryDebugger } from './query-debugger';

export class CodeLensProvider implements vscode.CodeLensProvider {
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;

  constructor(extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.extensionUri = extensionUri;
    this.drasiClient = drasiClient;

    vscode.commands.getCommands(true).then((commands) => {
      if (!commands.includes('editor.query.run')) {
        vscode.commands.registerCommand('editor.query.run', this.runQuery.bind(this));
      }
      if (!commands.includes('editor.resource.apply')) {
        vscode.commands.registerCommand('editor.resource.apply', this.applyResource.bind(this));
      }
    });
  }

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = [];
    const docStr = document.getText();
    const docs = yaml.parseAllDocuments(docStr);

    docs.forEach((doc) => {
      const kind = doc.get('kind');
      const id = doc.get('id');
      if (!kind || !id) {
        return;
      }

      const range = new vscode.Range(getPosition(docStr, doc.range[0]), getPosition(docStr, doc.range[1]));

      if (kind === 'Query') {
        codeLenses.push(new vscode.CodeLens(range, {
          command: 'editor.query.run',
          title: 'Debug',
          arguments: [doc.toJS()]
        }));
      }

      if (kind === 'Query' || kind === 'Source' || kind === 'Reaction') {
        codeLenses.push(new vscode.CodeLens(range, {
          command: 'editor.resource.apply',
          title: 'Apply',
          arguments: [doc.toJS()]
        }));
      }
    });

    return codeLenses;
  }

  async runQuery(query: any) {
    if (!query?.id) {
      return;
    }
    const dbg = new QueryDebugger(query.id, this.extensionUri, this.drasiClient);
    dbg.start();
  }

  async applyResource(resource: any) {
    if (!resource) {
      return;
    }

    const confirm = await vscode.window.showWarningMessage(
      `Are you sure you want to apply ${resource.id ?? resource.name}?`,
      'Yes',
      'No'
    );

    if (confirm !== 'Yes') {
      return;
    }

    await vscode.window.withProgress({
      title: `Applying ${resource.id ?? resource.name}`,
      location: vscode.ProgressLocation.Notification,
    }, async (progress) => {
      progress.report({ message: 'Applying...' });

      try {
        await applyResourceByKind(this.drasiClient, resource);
        vscode.window.showInformationMessage(`Resource ${resource.id ?? resource.name} applied successfully`);
      } catch (err) {
        vscode.window.showErrorMessage(`Error applying resource: ${err}`);
      }
    });
    vscode.commands.executeCommand('drasi.refresh');
  }
}

async function applyResourceByKind(client: DrasiClient, resource: any) {
  switch (resource.kind) {
    case 'Source':
      await client.applySource(resource);
      break;
    case 'Query':
      await client.applyQuery(resource);
      break;
    case 'Reaction':
      await client.applyReaction(resource);
      break;
    default:
      throw new Error(`Unsupported resource kind: ${resource.kind}`);
  }
}

function getPosition(yamlString: string, index: number): vscode.Position {
  if (index === 0) {
    return new vscode.Position(0, 0);
  }
  const lines = yamlString.slice(0, index).split('\n');
  const lineNumber = lines.length;
  const columnNumber = lines[lines.length - 1].length + 1;
  return new vscode.Position(lineNumber, columnNumber);
}
