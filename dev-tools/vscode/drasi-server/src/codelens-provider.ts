import * as vscode from 'vscode';
import * as yaml from 'yaml';
import { DrasiClient } from './drasi-client';

export class CodeLensProvider implements vscode.CodeLensProvider {
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;

  constructor(extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.extensionUri = extensionUri;
    this.drasiClient = drasiClient;

    vscode.commands.getCommands(true).then((commands) => {
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
      const items = extractItems(doc);
      if (items.length === 0) {
        return;
      }

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
  const lineNumber = lines.length;
  const columnNumber = lines[lines.length - 1].length + 1;
  return new vscode.Position(lineNumber, columnNumber);
}
