import * as vscode from 'vscode';
import * as yaml from 'js-yaml';
import { DrasiClient } from './drasi-client';

export class WorkspaceExplorer implements vscode.TreeDataProvider<ExplorerNode> {
  private _onDidChangeTreeData: vscode.EventEmitter<ExplorerNode | undefined | void> = new vscode.EventEmitter<ExplorerNode | undefined | void>();
  readonly onDidChangeTreeData: vscode.Event<ExplorerNode | undefined | void> = this._onDidChangeTreeData.event;
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;

  constructor(extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.extensionUri = extensionUri;
    this.drasiClient = drasiClient;
    vscode.commands.registerCommand('workspace.refresh', this.refresh.bind(this));
    vscode.commands.registerCommand('workspace.resource.apply', this.applyResource.bind(this));
    vscode.commands.registerCommand('workspace.resource.goto', this.gotoResource.bind(this));
    vscode.workspace.onDidSaveTextDocument((evt) => {
      if (evt.languageId === 'yaml') {
        this.refresh();
      }
    });
  }

  refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  getTreeItem(element: ExplorerNode): vscode.TreeItem | Thenable<vscode.TreeItem> {
    return element;
  }

  async getChildren(element?: ExplorerNode | undefined): Promise<ExplorerNode[]> {
    if (!vscode.workspace.workspaceFolders) {
      return [];
    }

    if (!element) {
      const result: ExplorerNode[] = [];
      const files = await vscode.workspace.findFiles('**/*.{yaml,yml}');

      for (const f of files.sort()) {
        try {
          const content = await vscode.workspace.fs.readFile(f);
          const docs: any[] = yaml.loadAll(content.toString());
          const hasQueries = docs.some((x) => !!x && x.kind === 'Query');
          const hasSources = docs.some((x) => !!x && x.kind === 'Source');
          const hasReactions = docs.some((x) => !!x && x.kind === 'Reaction');
          const hasConfig = docs.some((x) => !!x && (x.sources || x.queries || x.reactions));

          if (hasQueries || hasSources || hasReactions || hasConfig) {
            result.push(new FileNode(f));
          }
        } catch (_err) {
          // ignore parse errors
        }
      }

      return result;
    }

    if (!element.resourceUri) {
      return [];
    }

    if (element instanceof ResourceNode) {
      return [];
    }

    const result: ExplorerNode[] = [];

    try {
      const content = await vscode.workspace.fs.readFile(element.resourceUri);
      const text = content.toString();
      const docs: any[] = yaml.loadAll(text);

      for (const qry of docs.filter((x) => !!x && x.kind === 'Query' && x.id)) {
        const line = findResourceLine(text, 'Query', qry.id);
        result.push(new QueryNode(qry, element.resourceUri, line));
      }

      for (const resource of docs.filter((x) => !!x && x.kind === 'Source' && x.id)) {
        const line = findResourceLine(text, 'Source', resource.id);
        result.push(new SourceNode(resource, element.resourceUri, line));
      }

      for (const resource of docs.filter((x) => !!x && x.kind === 'Reaction' && x.id)) {
        const line = findResourceLine(text, 'Reaction', resource.id);
        result.push(new ReactionNode(resource, element.resourceUri, line));
      }

      for (const configDoc of docs.filter((x) => !!x && (x.sources || x.queries || x.reactions))) {
        for (const qry of (configDoc.queries ?? [])) {
          if (qry?.id) {
            const line = findNestedResourceLine(text, 'queries', qry.id);
            result.push(new QueryNode(qry, element.resourceUri, line));
          }
        }

        for (const resource of (configDoc.sources ?? [])) {
          if (resource?.id) {
            const line = findNestedResourceLine(text, 'sources', resource.id);
            result.push(new SourceNode(resource, element.resourceUri, line));
          }
        }

        for (const resource of (configDoc.reactions ?? [])) {
          if (resource?.id) {
            const line = findNestedResourceLine(text, 'reactions', resource.id);
            result.push(new ReactionNode(resource, element.resourceUri, line));
          }
        }
      }
    } catch (_err) {
      // ignore parse errors
    }

    return result;
  }

  async gotoResource(node: ResourceNode) {
    if (!node?.fileUri) {
      return;
    }
    const doc = await vscode.workspace.openTextDocument(node.fileUri);
    const line = node.line ?? 0;
    const position = new vscode.Position(line, 0);
    await vscode.window.showTextDocument(doc, {
      selection: new vscode.Range(position, position),
    });
  }

  async applyResource(resourceNode: ResourceNode) {
    if (!resourceNode?.resource) {
      return;
    }

    const confirm = await vscode.window.showWarningMessage(
      `Are you sure you want to apply ${resourceNode.resource.id}?`,
      'Yes',
      'No'
    );

    if (confirm !== 'Yes') {
      return;
    }

    await vscode.window.withProgress({
      title: `Applying ${resourceNode.resource.id}`,
      location: vscode.ProgressLocation.Notification,
    }, async (progress) => {
      progress.report({ message: 'Applying...' });

      try {
        await applyResourceByType(this.drasiClient, resourceNode);
        vscode.window.showInformationMessage(`Resource ${resourceNode.resource.id} applied successfully`);
      } catch (err) {
        vscode.window.showErrorMessage(`Error applying resource: ${err}`);
      }
    });
    vscode.commands.executeCommand('drasi.refresh');
  }
}

abstract class ExplorerNode extends vscode.TreeItem {
  resourceUri?: vscode.Uri;
}

abstract class ResourceNode extends ExplorerNode {
  resourceType: 'Source' | 'Query' | 'Reaction';
  resource: any;
  fileUri: vscode.Uri;
  line?: number;

  constructor(resourceType: 'Source' | 'Query' | 'Reaction', resource: any, fileUri: vscode.Uri, line?: number) {
    super(resource.id, vscode.TreeItemCollapsibleState.None);
    this.resourceType = resourceType;
    this.resource = resource;
    this.fileUri = fileUri;
    this.line = line;
  }
}

class FileNode extends ExplorerNode {
  contextValue = 'fileNode';

  constructor(uri: vscode.Uri) {
    super(uri, vscode.TreeItemCollapsibleState.Expanded);
    this.resourceUri = uri;
  }
}

class QueryNode extends ResourceNode {
  contextValue = 'workspace.queryNode';

  constructor(query: any, fileUri: vscode.Uri, line?: number) {
    super('Query', query, fileUri, line);
    this.iconPath = new vscode.ThemeIcon('code');
    this.label = query.id;
    this.command = {
      command: 'workspace.resource.goto',
      title: 'Go to resource',
      arguments: [this]
    };
  }
}

class SourceNode extends ResourceNode {
  contextValue = 'workspace.sourceNode';

  constructor(resource: any, fileUri: vscode.Uri, line?: number) {
    super('Source', resource, fileUri, line);
    this.iconPath = new vscode.ThemeIcon('database');
    this.label = resource.id;
    this.command = {
      command: 'workspace.resource.goto',
      title: 'Go to resource',
      arguments: [this]
    };
  }
}

class ReactionNode extends ResourceNode {
  contextValue = 'workspace.reactionNode';

  constructor(resource: any, fileUri: vscode.Uri, line?: number) {
    super('Reaction', resource, fileUri, line);
    this.iconPath = new vscode.ThemeIcon('zap');
    this.label = resource.id;
    this.command = {
      command: 'workspace.resource.goto',
      title: 'Go to resource',
      arguments: [this]
    };
  }
}

/**
 * Find the line number of a top-level resource (kind: Type with id: resourceId)
 */
function findResourceLine(text: string, kind: string, resourceId: string): number {
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    // Look for "id: resourceId" and verify it's the right kind by checking nearby lines
    if (line.match(new RegExp(`^\\s*id:\\s*['"]?${escapeRegex(resourceId)}['"]?\\s*$`))) {
      // Check if this is preceded by "kind: Type" within a few lines
      for (let j = Math.max(0, i - 5); j < i; j++) {
        if (lines[j].match(new RegExp(`^\\s*kind:\\s*['"]?${kind}['"]?\\s*$`))) {
          return j; // Return the line with "kind:"
        }
      }
    }
  }
  return 0;
}

/**
 * Find the line number of a nested resource (under sources:, queries:, or reactions:)
 */
function findNestedResourceLine(text: string, section: string, resourceId: string): number {
  const lines = text.split('\n');
  let inSection = false;
  
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    
    // Check if we're entering the section (e.g., "sources:", "queries:", "reactions:")
    if (line.match(new RegExp(`^${section}:\\s*$`))) {
      inSection = true;
      continue;
    }
    
    // Check if we're leaving the section (another top-level key)
    if (inSection && line.match(/^[a-zA-Z]/)) {
      inSection = false;
    }
    
    if (inSection) {
      // Look for "- id: resourceId" or "id: resourceId" in the section
      if (line.match(new RegExp(`^\\s*-?\\s*id:\\s*['"]?${escapeRegex(resourceId)}['"]?\\s*$`))) {
        // Find the start of this list item (the line with "- ")
        for (let j = i; j >= 0; j--) {
          if (lines[j].match(/^\s*-\s/)) {
            return j;
          }
        }
        return i;
      }
    }
  }
  return 0;
}

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function applyResourceByType(client: DrasiClient, resourceNode: ResourceNode) {
  switch (resourceNode.resourceType) {
    case 'Source':
      await client.applySource(resourceNode.resource);
      break;
    case 'Query':
      await client.applyQuery(resourceNode.resource);
      break;
    case 'Reaction':
      await client.applyReaction(resourceNode.resource);
      break;
  }
}
