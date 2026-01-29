import * as vscode from 'vscode';
import { ConnectionRegistry } from './sdk/config';
import { DrasiClient } from './drasi-client';
import { WorkspaceExplorer } from './workspace-explorer';
import { DrasiExplorer } from './drasi-explorer';
import { CodeLensProvider } from './codelens-provider';
import { SchemaProvider } from './schema-provider';
import { DrasiYamlDiagnosticProvider } from './yaml-diagnostic';

let drasiClient: DrasiClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
  const registry = new ConnectionRegistry();
  await registry.ensureDefaultConnection();
  drasiClient = new DrasiClient(registry);

  const workspaceExplorer = new WorkspaceExplorer(context.extensionUri, drasiClient);
  vscode.window.registerTreeDataProvider('workspace', workspaceExplorer);

  const drasiExplorer = new DrasiExplorer(context.extensionUri, drasiClient, registry);
  vscode.window.registerTreeDataProvider('drasi', drasiExplorer);

  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider({ language: 'yaml' }, new CodeLensProvider(context.extensionUri, drasiClient))
  );

  const diagnosticProvider = new DrasiYamlDiagnosticProvider();
  diagnosticProvider.activate(context);

  const schemaProvider = new SchemaProvider(registry, diagnosticProvider);
  await schemaProvider.activate(context);

  context.subscriptions.push(
    vscode.commands.registerCommand('drasi.schema.refresh', async () => {
      await schemaProvider.refreshSchemas(context.globalStorageUri);
      vscode.window.showInformationMessage('Drasi schemas refreshed');
    })
  );

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration('drasiServer')) {
        await schemaProvider.refreshSchemas(context.globalStorageUri);
        drasiExplorer.refresh();
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('drasi.schema.useConnection', async () => {
      await vscode.commands.executeCommand('drasi.connection.use');
      await schemaProvider.refreshSchemas(context.globalStorageUri);
      drasiExplorer.refresh();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('drasi.schema.toggleFile', async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        return;
      }

      if (editor.document.languageId !== 'yaml') {
        vscode.window.showWarningMessage('Open a YAML file to mark as a Drasi config.');
        return;
      }

      const documentPath = vscode.workspace.asRelativePath(editor.document.uri, false);
      const config = vscode.workspace.getConfiguration('drasiServer');
      const existing = config.get<string[]>('schemaFiles') ?? [];
      const normalized = documentPath.replace(/\\/g, '/');
      const isMarked = existing.includes(normalized);
      const updated = isMarked
        ? existing.filter((value) => value !== normalized)
        : [...existing, normalized];

      await config.update('schemaFiles', updated, vscode.ConfigurationTarget.Workspace);
      await schemaProvider.refreshSchemas(context.globalStorageUri);
      vscode.window.showInformationMessage(
        isMarked ? 'Removed file from Drasi schema list.' : 'Marked file as Drasi config.'
      );
    })
  );
}

export function deactivate() {
  drasiClient = undefined;
}
