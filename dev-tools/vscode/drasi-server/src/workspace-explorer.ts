import * as vscode from 'vscode';
import * as yaml from 'js-yaml';
import { DrasiClient } from './drasi-client';
import { QueryDebugger } from './query-debugger';

export class WorkspaceExplorer implements vscode.TreeDataProvider<ExplorerNode> {
  private _onDidChangeTreeData: vscode.EventEmitter<ExplorerNode | undefined | void> = new vscode.EventEmitter<ExplorerNode | undefined | void>();
  readonly onDidChangeTreeData: vscode.Event<ExplorerNode | undefined | void> = this._onDidChangeTreeData.event;
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;

  constructor(extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.extensionUri = extensionUri;
    this.drasiClient = drasiClient;
    vscode.commands.registerCommand('workspace.refresh', this.refresh.bind(this));
    vscode.commands.registerCommand('workspace.query.run', this.runQuery.bind(this));
    vscode.commands.registerCommand('workspace.resource.apply', this.applyResource.bind(this));
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
      const docs: any[] = yaml.loadAll(content.toString());

      for (const qry of docs.filter((x) => !!x && x.kind === 'Query' && x.id)) {
        const queryUri = vscode.Uri.parse(element.resourceUri.toString() + '#' + qry.id);
        result.push(new QueryNode(qry, queryUri));
      }

      for (const resource of docs.filter((x) => !!x && x.kind === 'Source' && x.id)) {
        const resourceUri = vscode.Uri.parse(element.resourceUri.toString() + '#' + resource.id);
        result.push(new SourceNode(resource, resourceUri));
      }

      for (const resource of docs.filter((x) => !!x && x.kind === 'Reaction' && x.id)) {
        const resourceUri = vscode.Uri.parse(element.resourceUri.toString() + '#' + resource.id);
        result.push(new ReactionNode(resource, resourceUri));
      }

      for (const configDoc of docs.filter((x) => !!x && (x.sources || x.queries || x.reactions))) {
        for (const qry of (configDoc.queries ?? [])) {
          if (qry?.id) {
            const queryUri = vscode.Uri.parse(element.resourceUri.toString() + '#' + qry.id);
            result.push(new QueryNode(qry, queryUri));
          }
        }

        for (const resource of (configDoc.sources ?? [])) {
          if (resource?.id) {
            const resourceUri = vscode.Uri.parse(element.resourceUri.toString() + '#' + resource.id);
            result.push(new SourceNode(resource, resourceUri));
          }
        }

        for (const resource of (configDoc.reactions ?? [])) {
          if (resource?.id) {
            const resourceUri = vscode.Uri.parse(element.resourceUri.toString() + '#' + resource.id);
            result.push(new ReactionNode(resource, resourceUri));
          }
        }
      }
    } catch (_err) {
      // ignore parse errors
    }

    return result;
  }

  async runQuery(queryNode: QueryNode) {
    if (!queryNode?.resource?.id) {
      return;
    }

    const dbg = new QueryDebugger(queryNode.resource.id, this.extensionUri, this.drasiClient);
    dbg.start();
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

  constructor(resourceType: 'Source' | 'Query' | 'Reaction', resource: any, uri: vscode.Uri) {
    super(uri, vscode.TreeItemCollapsibleState.Expanded);
    this.resourceType = resourceType;
    this.resource = resource;
    this.resourceUri = uri;
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

  constructor(query: any, uri: vscode.Uri) {
    super('Query', query, uri);
    this.iconPath = new vscode.ThemeIcon('code');
    this.label = query.id;
    this.command = {
      command: 'vscode.open',
      title: 'Open',
      arguments: [uri]
    };
  }
}

class SourceNode extends ResourceNode {
  contextValue = 'workspace.sourceNode';

  constructor(resource: any, uri: vscode.Uri) {
    super('Source', resource, uri);
    this.iconPath = new vscode.ThemeIcon('database');
    this.label = resource.id;
    this.command = {
      command: 'vscode.open',
      title: 'Open',
      arguments: [uri]
    };
  }
}

class ReactionNode extends ResourceNode {
  contextValue = 'workspace.reactionNode';

  constructor(resource: any, uri: vscode.Uri) {
    super('Reaction', resource, uri);
    this.iconPath = new vscode.ThemeIcon('zap');
    this.label = resource.id;
    this.command = {
      command: 'vscode.open',
      title: 'Open',
      arguments: [uri]
    };
  }
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
