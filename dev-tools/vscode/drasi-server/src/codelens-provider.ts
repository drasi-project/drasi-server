import * as vscode from 'vscode';
import * as yaml from 'yaml';
import { DrasiClient } from './drasi-client';
import { isDrasiYaml } from './drasi-yaml';
import { ServerLauncher, extractPort } from './server-launcher';

const CONFIG_KEYS = new Set([
  'sources', 'queries', 'reactions', 'instances',
  'host', 'port', 'logLevel', 'persistConfig', 'persistIndex',
  'defaultPriorityQueueCapacity', 'defaultDispatchBufferCapacity', 'stateStore',
]);

export class CodeLensProvider implements vscode.CodeLensProvider {
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;
  private serverLauncher: ServerLauncher;

  constructor(extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.extensionUri = extensionUri;
    this.drasiClient = drasiClient;
    this.serverLauncher = new ServerLauncher();

    vscode.commands.getCommands(true).then((commands) => {
      if (!commands.includes('editor.resource.apply')) {
        vscode.commands.registerCommand('editor.resource.apply', this.applyResource.bind(this));
      }
      if (!commands.includes('drasi.server.launch')) {
        vscode.commands.registerCommand('drasi.server.launch', this.launchServer.bind(this));
      }
    });
  }

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = [];
    const docStr = document.getText();

    if (!isDrasiYaml(docStr)) {
      return codeLenses;
    }

    const docs = yaml.parseAllDocuments(docStr);
    let addedLaunchLens = false;

    docs.forEach((doc) => {
      const obj = doc.toJS();
      if (!addedLaunchLens && obj && typeof obj === 'object' && isConfigDocument(obj)) {
        const contents = doc.contents;
        const start = contents ? getPosition(docStr, (contents.range ?? [0])[0]) : new vscode.Position(0, 0);
        const range = new vscode.Range(start, start);
        codeLenses.push(new vscode.CodeLens(range, {
          command: 'drasi.server.launch',
          title: '$(play) Launch Server',
          arguments: [document.uri.fsPath, extractPort(docStr)],
        }));
        addedLaunchLens = true;
      }

      const items = extractItems(doc);
      items.forEach((item) => {
        const range = new vscode.Range(getPosition(docStr, item.range.start), getPosition(docStr, item.range.end));
        if (item.kind === 'Query' || item.kind === 'Source' || item.kind === 'Reaction') {
          codeLenses.push(new vscode.CodeLens(range, {
            command: 'editor.resource.apply',
            title: 'Apply',
            arguments: [item.payload]
          }));
        }
      });
    });

    return codeLenses;
  }

  async launchServer(configPath: string, defaultPort?: number) {
    await this.serverLauncher.launch(configPath, defaultPort);
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

type ItemRange = { start: number; end: number };
type ExtractedItem = { kind: string; payload: any; range: ItemRange };

function extractItems(doc: yaml.Document): ExtractedItem[] {
  const items: ExtractedItem[] = [];
  const docContents = doc.contents;
  if (!docContents || !yaml.isMap(docContents)) {
    return items;
  }
  const map = docContents as yaml.YAMLMap;
  const kindNode = map.get('kind', true);
  if (kindNode) {
    const kind = doc.get('kind');
    const id = doc.get('id');
    if (kind && id) {
      items.push({
        kind,
        payload: doc.toJS(),
        range: rangeFromNode(docContents),
      });
    }
    return items;
  }

  addListItems(doc, map, 'sources', 'Source', items);
  addListItems(doc, map, 'queries', 'Query', items);
  addListItems(doc, map, 'reactions', 'Reaction', items);
  if (items.length === 0) {
    const kind = doc.get('kind');
    const id = doc.get('id');
    if (kind && id) {
      items.push({
        kind,
        payload: doc.toJS(),
        range: rangeFromNode(docContents),
      });
    }
  }
  return items;
}

function addListItems(
  doc: yaml.Document,
  map: yaml.YAMLMap,
  key: string,
  kind: string,
  items: ExtractedItem[]
) {
  const node = map.get(key, true);
  if (!node || !yaml.isSeq(node)) {
    return;
  }
  const seq = node as yaml.YAMLSeq;
  seq.items.forEach((entry) => {
    if (!entry || !yaml.isMap(entry)) {
      return;
    }
    const entryMap = entry as yaml.YAMLMap;
    const idNode = entryMap.get('id', true);
    if (!idNode) {
      return;
    }
    const spec = entry.toJS(doc);
    const payload = {
      kind,
      id: spec?.id,
      spec,
    };
    items.push({
      kind,
      payload,
      range: rangeFromNode(entry),
    });
  });
}

function rangeFromNode(node: yaml.Node): ItemRange {
  const range = node.range ?? [0, 0, 0];
  return { start: range[0], end: range[1] };
}

function getPosition(yamlString: string, index: number): vscode.Position {
  if (index === 0) {
    return new vscode.Position(0, 0);
  }
  const lines = yamlString.slice(0, index).split('\n');
  const lineNumber = lines.length - 1;
  const columnNumber = lines[lines.length - 1].length;
  return new vscode.Position(lineNumber, columnNumber);
}

function isConfigDocument(obj: Record<string, unknown>): boolean {
  return Object.keys(obj).some((key) => CONFIG_KEYS.has(key));
}
