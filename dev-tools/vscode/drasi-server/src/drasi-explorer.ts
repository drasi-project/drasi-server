import * as vscode from 'vscode';
import { DrasiClient } from './drasi-client';
import { ComponentEvent, ComponentListItem, ComponentStatus, InstanceListItem, LogMessage } from './models/common';
import { ConnectionRegistry, ServerConnectionConfig } from './sdk/config';
import { QueryWatcher } from './query-watcher';
import { ObservabilityViewer, LogTerminalViewer } from './observability-viewer';
import { ObservabilityStream } from './observability-stream';

export class DrasiExplorer implements vscode.TreeDataProvider<ExplorerNode> {
  private _onDidChangeTreeData: vscode.EventEmitter<ExplorerNode | undefined | void> = new vscode.EventEmitter<ExplorerNode | undefined | void>();
  readonly onDidChangeTreeData: vscode.Event<ExplorerNode | undefined | void> = this._onDidChangeTreeData.event;
  readonly drasiClient: DrasiClient;
  readonly registry: ConnectionRegistry;
  private extensionUri: vscode.Uri;

  constructor(extensionUri: vscode.Uri, drasiClient: DrasiClient, registry: ConnectionRegistry) {
    this.extensionUri = extensionUri;
    this.drasiClient = drasiClient;
    this.registry = registry;
    vscode.commands.registerCommand('drasi.refresh', this.refresh.bind(this));
    vscode.commands.registerCommand('drasi.query.watch', this.watchQuery.bind(this));
    vscode.commands.registerCommand('drasi.resource.delete', this.deleteResource.bind(this));
    vscode.commands.registerCommand('drasi.source.start', this.startSource.bind(this));
    vscode.commands.registerCommand('drasi.source.stop', this.stopSource.bind(this));
    vscode.commands.registerCommand('drasi.query.start', this.startQuery.bind(this));
    vscode.commands.registerCommand('drasi.query.stop', this.stopQuery.bind(this));
    vscode.commands.registerCommand('drasi.reaction.start', this.startReaction.bind(this));
    vscode.commands.registerCommand('drasi.reaction.stop', this.stopReaction.bind(this));
    vscode.commands.registerCommand('drasi.events.view', this.viewEvents.bind(this));
    vscode.commands.registerCommand('drasi.events.stream', this.streamEvents.bind(this));
    vscode.commands.registerCommand('drasi.logs.view', this.viewLogs.bind(this));
    vscode.commands.registerCommand('drasi.logs.stream', this.streamLogs.bind(this));
    vscode.commands.registerCommand('drasi.instance.use', this.useInstance.bind(this));
    vscode.commands.registerCommand('drasi.instance.add', this.addInstance.bind(this));
    vscode.commands.registerCommand('drasi.connection.configure', this.configureConnection.bind(this));
    vscode.commands.registerCommand('drasi.connection.add', this.addConnection.bind(this));
    vscode.commands.registerCommand('drasi.connection.use', this.useConnection.bind(this));
    vscode.commands.registerCommand('drasi.resource.viewConfig', this.viewConfig.bind(this));
  }

  refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  getTreeItem(element: ExplorerNode): vscode.TreeItem | Thenable<vscode.TreeItem> {
    return element;
  }

  async getChildren(element?: ExplorerNode | undefined): Promise<ExplorerNode[]> {
    if (!element) {
      await this.registry.ensureDefaultConnection();
      const connections = this.registry.getConnections();
      const currentId = this.registry.getCurrentConnectionId();
      
      // Check health for all connections in parallel
      const healthChecks = await Promise.all(
        connections.map(async (connection) => {
          const isConnected = await DrasiClient.checkHealthForUrl(connection.url);
          return { connection, isConnected };
        })
      );
      
      return healthChecks.map(({ connection, isConnected }) => {
        return new ConnectionNode(connection, connection.id === currentId, isConnected);
      });
    }

    if (element instanceof ConnectionNode) {
      if (!element.isCurrent || !element.isConnected) {
        return [];
      }
      const instances = await this.drasiClient.listInstances();
      if (instances.length === 1) {
        const instance = instances[0];
        const currentInstance = this.registry.getCurrentConnection().instanceId;
        if (currentInstance !== instance.id) {
          await this.registry.setCurrentInstanceId(instance.id);
        }
        return [
          new CategoryNode(Category.sources, instance),
          new CategoryNode(Category.queries, instance),
          new CategoryNode(Category.reactions, instance)
        ];
      }

      const currentInstance = this.registry.getCurrentConnection().instanceId;
      return instances.map((instance) => new InstanceNode(instance, instance.id === currentInstance));
    }

    if (element instanceof InstanceNode) {
      if (!element.isCurrent) {
        return [];
      }
      return [
        new CategoryNode(Category.sources, element.instance),
        new CategoryNode(Category.queries, element.instance),
        new CategoryNode(Category.reactions, element.instance)
      ];
    }

    if (element instanceof CategoryNode) {
      switch (element.category) {
        case Category.sources:
          return (await this.drasiClient.listSources()).map((x) => new SourceNode(x, element.instance));
        case Category.queries:
          return (await this.drasiClient.listQueries()).map((x) => new QueryNode(x, element.instance));
        case Category.reactions:
          return (await this.drasiClient.listReactions()).map((x) => new ReactionNode(x, element.instance));
      }
    }

    return [];
  }

  async watchQuery(queryNode: QueryNode) {
    if (!queryNode?.component?.id) {
      return;
    }
    const watcher = new QueryWatcher(queryNode.component.id, this.extensionUri, this.drasiClient);
    await watcher.start();
  }

  async deleteResource(resourceNode: ResourceNode) {
    if (!resourceNode) {
      return;
    }

    const confirm = await vscode.window.showWarningMessage(
      `Are you sure you want to delete ${resourceNode.component.id}?`,
      'Yes',
      'No'
    );

    if (confirm !== 'Yes') {
      return;
    }

    await vscode.window.withProgress({
      title: `Deleting ${resourceNode.component.id}`,
      location: vscode.ProgressLocation.Notification,
    }, async (progress) => {
      progress.report({ message: 'Deleting...' });

      try {
        await deleteByKind(this.drasiClient, resourceNode.kind, resourceNode.component.id);
        vscode.window.showInformationMessage(`Resource ${resourceNode.component.id} deleted`);
      } catch (err) {
        vscode.window.showErrorMessage(`Error deleting resource: ${err}`);
      }
    });
    this.refresh();
  }

  async startSource(resourceNode: SourceNode) {
    await this.startStopResource(resourceNode, 'start', () => this.drasiClient.startSource(resourceNode.component.id));
  }

  async stopSource(resourceNode: SourceNode) {
    await this.startStopResource(resourceNode, 'stop', () => this.drasiClient.stopSource(resourceNode.component.id));
  }

  async startQuery(resourceNode: QueryNode) {
    await this.startStopResource(resourceNode, 'start', () => this.drasiClient.startQuery(resourceNode.component.id));
  }

  async stopQuery(resourceNode: QueryNode) {
    await this.startStopResource(resourceNode, 'stop', () => this.drasiClient.stopQuery(resourceNode.component.id));
  }

  async startReaction(resourceNode: ReactionNode) {
    await this.startStopResource(resourceNode, 'start', () => this.drasiClient.startReaction(resourceNode.component.id));
  }

  async stopReaction(resourceNode: ReactionNode) {
    await this.startStopResource(resourceNode, 'stop', () => this.drasiClient.stopReaction(resourceNode.component.id));
  }

  async viewEvents(resourceNode: ResourceNode) {
    if (!resourceNode) {
      return;
    }
    await vscode.window.withProgress({
      title: `Fetching events for ${resourceNode.component.id}`,
      location: vscode.ProgressLocation.Notification,
    }, async () => {
      try {
        const events = await this.fetchEvents(resourceNode);
        const viewer = new LogTerminalViewer(`Events: ${resourceNode.component.id}`);
        viewer.show();
        viewer.appendHeader('Snapshot');
        viewer.appendEvents(events);
        if (events.length === 0) {
          viewer.appendRaw('No events available.');
        }
      } catch (error) {
        vscode.window.showErrorMessage(`Failed to fetch events: ${error}`);
      }
    });
  }

  async streamEvents(resourceNode: ResourceNode) {
    if (!resourceNode) {
      return;
    }
    const viewer = new LogTerminalViewer(`Events Stream: ${resourceNode.component.id}`);
    const stream = new ObservabilityStream();
    
    // Clean up stream when terminal is closed
    viewer.onDispose(() => stream.stop());
    
    viewer.show();
    viewer.appendHeader('Streaming');
    try {
      const url = this.getEventsStreamUrl(resourceNode);
      await stream.streamEvents(url, viewer);
    } catch (error) {
      viewer.appendError(String(error));
    }
  }

  async viewLogs(resourceNode: ResourceNode) {
    if (!resourceNode) {
      return;
    }
    await vscode.window.withProgress({
      title: `Fetching logs for ${resourceNode.component.id}`,
      location: vscode.ProgressLocation.Notification,
    }, async () => {
      try {
        const logs = await this.fetchLogs(resourceNode);
        const viewer = new LogTerminalViewer(`Logs: ${resourceNode.component.id}`);
        viewer.show();
        viewer.appendHeader('Snapshot');
        viewer.appendLogMessages(logs);
        if (logs.length === 0) {
          viewer.appendRaw('No logs available.');
        }
      } catch (error) {
        vscode.window.showErrorMessage(`Failed to fetch logs: ${error}`);
      }
    });
  }

  async streamLogs(resourceNode: ResourceNode) {
    if (!resourceNode) {
      return;
    }
    const viewer = new LogTerminalViewer(`Logs Stream: ${resourceNode.component.id}`);
    const stream = new ObservabilityStream();
    
    // Clean up stream when terminal is closed
    viewer.onDispose(() => stream.stop());
    
    viewer.show();
    viewer.appendHeader('Streaming');
    try {
      const url = this.getLogsStreamUrl(resourceNode);
      await stream.streamLogs(url, viewer);
    } catch (error) {
      viewer.appendError(String(error));
    }
  }

  async viewConfig(resourceNode: ResourceNode) {
    if (!resourceNode) {
      return;
    }
    await vscode.window.withProgress({
      title: `Fetching config for ${resourceNode.component.id}`,
      location: vscode.ProgressLocation.Notification,
    }, async () => {
      try {
        const config = await this.fetchConfig(resourceNode);
        const yaml = this.toYaml(config);
        const doc = await vscode.workspace.openTextDocument({
          content: yaml,
          language: 'yaml',
        });
        await vscode.window.showTextDocument(doc, { preview: true });
      } catch (error) {
        vscode.window.showErrorMessage(`Failed to fetch config: ${error}`);
      }
    });
  }

  private async fetchConfig(resourceNode: ResourceNode): Promise<Record<string, unknown>> {
    switch (resourceNode.kind) {
      case 'source':
        return this.drasiClient.getSourceConfig(resourceNode.component.id);
      case 'query':
        return this.drasiClient.getQueryConfig(resourceNode.component.id);
      case 'reaction':
        return this.drasiClient.getReactionConfig(resourceNode.component.id);
      default:
        throw new Error(`Unknown resource kind: ${resourceNode.kind}`);
    }
  }

  private toYaml(obj: Record<string, unknown>, indent = 0): string {
    const spaces = '  '.repeat(indent);
    let result = '';
    for (const [key, value] of Object.entries(obj)) {
      if (value === null || value === undefined) {
        continue;
      }
      if (Array.isArray(value)) {
        if (value.length === 0) {
          result += `${spaces}${key}: []\n`;
        } else {
          result += `${spaces}${key}:\n`;
          for (const item of value) {
            if (typeof item === 'object' && item !== null) {
              result += `${spaces}  -\n`;
              result += this.toYaml(item as Record<string, unknown>, indent + 2);
            } else {
              result += `${spaces}  - ${this.formatValue(item)}\n`;
            }
          }
        }
      } else if (typeof value === 'object') {
        result += `${spaces}${key}:\n`;
        result += this.toYaml(value as Record<string, unknown>, indent + 1);
      } else {
        result += `${spaces}${key}: ${this.formatValue(value)}\n`;
      }
    }
    return result;
  }

  private formatValue(value: unknown): string {
    if (typeof value === 'string') {
      // Quote strings that contain special characters or look like other types
      if (value.includes(':') || value.includes('#') || value.includes('\n') || 
          value === '' || value === 'true' || value === 'false' ||
          /^\d+$/.test(value) || /^\d+\.\d+$/.test(value)) {
        return `"${value.replace(/"/g, '\\"')}"`;
      }
      return value;
    }
    return String(value);
  }

  async useInstance(instanceNode: InstanceNode) {
    if (!instanceNode) {
      return;
    }
    await this.registry.setCurrentInstanceId(instanceNode.instance.id);
    this.refresh();
  }

  async addInstance() {
    const id = await vscode.window.showInputBox({
      prompt: 'Enter a unique ID for the new instance',
      placeHolder: 'my-instance',
    });

    if (!id) {
      return;
    }

    // Ask about persistent indexing
    const persistIndexChoice = await vscode.window.showQuickPick(
      ['No (in-memory, faster)', 'Yes (RocksDB, persistent)'],
      { placeHolder: 'Use persistent indexing?' }
    );
    
    if (persistIndexChoice === undefined) {
      return;
    }
    const persistIndex = persistIndexChoice.startsWith('Yes');

    // Ask about priority queue capacity (optional)
    const priorityQueueCapacityStr = await vscode.window.showInputBox({
      prompt: 'Default priority queue capacity (leave empty for default)',
      placeHolder: '10000',
      validateInput: (value) => {
        if (value && isNaN(parseInt(value, 10))) {
          return 'Must be a number';
        }
        return null;
      }
    });

    if (priorityQueueCapacityStr === undefined) {
      return;
    }

    // Ask about dispatch buffer capacity (optional)
    const dispatchBufferCapacityStr = await vscode.window.showInputBox({
      prompt: 'Default dispatch buffer capacity (leave empty for default)',
      placeHolder: '1000',
      validateInput: (value) => {
        if (value && isNaN(parseInt(value, 10))) {
          return 'Must be a number';
        }
        return null;
      }
    });

    if (dispatchBufferCapacityStr === undefined) {
      return;
    }

    const request = {
      id,
      persistIndex: persistIndex || undefined,
      defaultPriorityQueueCapacity: priorityQueueCapacityStr ? parseInt(priorityQueueCapacityStr, 10) : undefined,
      defaultDispatchBufferCapacity: dispatchBufferCapacityStr ? parseInt(dispatchBufferCapacityStr, 10) : undefined,
    };

    await vscode.window.withProgress({
      title: `Creating instance ${id}`,
      location: vscode.ProgressLocation.Notification,
    }, async () => {
      try {
        await this.drasiClient.createInstance(request);
        vscode.window.showInformationMessage(`Instance '${id}' created successfully`);
        this.refresh();
      } catch (err) {
        vscode.window.showErrorMessage(`Failed to create instance: ${err}`);
      }
    });
  }

  async configureConnection() {
    const current = this.registry.getCurrentConnection();
    const url = await vscode.window.showInputBox({
      prompt: 'Enter Drasi Server URL',
      value: current.url,
    });

    if (!url) {
      return;
    }

    await this.registry.updateCurrentConnectionUrl(url);
    this.refresh();
  }

  async addConnection() {
    const url = await vscode.window.showInputBox({
      prompt: 'Enter Drasi Server URL',
      value: 'http://localhost:8080',
    });

    if (!url) {
      return;
    }

    const name = await vscode.window.showInputBox({
      prompt: 'Enter a name for this server',
      value: url,
    });

    if (!name) {
      return;
    }

    await this.registry.addConnection(name, url);
    this.refresh();
  }

  async useConnection(connectionNode?: ConnectionNode) {
    let target = connectionNode?.connection;
    if (!target) {
      await this.registry.ensureDefaultConnection();
      const connections = this.registry.getConnections();
      const currentId = this.registry.getCurrentConnectionId();
      const options = connections.map((connection) => ({
        label: connection.name,
        description: connection.url,
        detail: connection.id === currentId ? 'Current' : undefined,
        connection,
      }));
      const picked = await vscode.window.showQuickPick(options, {
        placeHolder: 'Select Drasi server',
        matchOnDescription: true,
      });
      if (!picked) {
        return;
      }
      target = picked.connection;
    }

    await this.registry.setCurrentConnectionId(target.id);
    this.refresh();
  }

  private async startStopResource(resourceNode: ResourceNode, action: string, fn: () => Promise<void>) {
    if (!resourceNode) {
      return;
    }

    await vscode.window.withProgress({
      title: `${action === 'start' ? 'Starting' : 'Stopping'} ${resourceNode.component.id}`,
      location: vscode.ProgressLocation.Notification,
    }, async (progress) => {
      progress.report({ message: `${action === 'start' ? 'Starting' : 'Stopping'}...` });

      try {
        await fn();
      } catch (err) {
        vscode.window.showErrorMessage(`Error ${action}ing resource: ${err}`);
      }
    });

    this.refresh();
  }

  private async fetchEvents(resourceNode: ResourceNode): Promise<ComponentEvent[]> {
    switch (resourceNode.kind) {
      case 'source':
        return this.drasiClient.getSourceEvents(resourceNode.component.id);
      case 'query':
        return this.drasiClient.getQueryEvents(resourceNode.component.id);
      case 'reaction':
        return this.drasiClient.getReactionEvents(resourceNode.component.id);
      default:
        return [];
    }
  }

  private async fetchLogs(resourceNode: ResourceNode): Promise<LogMessage[]> {
    switch (resourceNode.kind) {
      case 'source':
        return this.drasiClient.getSourceLogs(resourceNode.component.id);
      case 'query':
        return this.drasiClient.getQueryLogs(resourceNode.component.id);
      case 'reaction':
        return this.drasiClient.getReactionLogs(resourceNode.component.id);
      default:
        return [];
    }
  }

  private getEventsStreamUrl(resourceNode: ResourceNode): string {
    switch (resourceNode.kind) {
      case 'source':
        return this.drasiClient.getSourceEventsStreamUrl(resourceNode.component.id);
      case 'query':
        return this.drasiClient.getQueryEventsStreamUrl(resourceNode.component.id);
      case 'reaction':
        return this.drasiClient.getReactionEventsStreamUrl(resourceNode.component.id);
      default:
        throw new Error(`Unsupported resource kind: ${resourceNode.kind}`);
    }
  }

  private getLogsStreamUrl(resourceNode: ResourceNode): string {
    switch (resourceNode.kind) {
      case 'source':
        return this.drasiClient.getSourceLogsStreamUrl(resourceNode.component.id);
      case 'query':
        return this.drasiClient.getQueryLogsStreamUrl(resourceNode.component.id);
      case 'reaction':
        return this.drasiClient.getReactionLogsStreamUrl(resourceNode.component.id);
      default:
        throw new Error(`Unsupported resource kind: ${resourceNode.kind}`);
    }
  }
}

class ExplorerNode extends vscode.TreeItem {}

class ConnectionNode extends ExplorerNode {
  contextValue = 'drasi.connectionNode';
  connection: ServerConnectionConfig;
  private current: boolean;
  private connected: boolean;

  constructor(connection: ServerConnectionConfig, current: boolean, connected: boolean) {
    super(connection.name, vscode.TreeItemCollapsibleState.Expanded);
    this.connection = connection;
    this.current = current;
    this.connected = connected;
    this.description = connection.url;
    if (current) {
      this.contextValue = 'drasi.connectionCurrentNode';
    }
    this.iconPath = this.getStatusIcon();
    this.tooltip = connected ? 'Connected' : 'Disconnected';
  }

  private getStatusIcon(): vscode.ThemeIcon {
    if (this.connected) {
      return new vscode.ThemeIcon('cloud', new vscode.ThemeColor('testing.iconPassed'));
    }
    return new vscode.ThemeIcon('cloud', new vscode.ThemeColor('testing.iconFailed'));
  }

  public get isCurrent() {
    return this.current;
  }

  public get isConnected() {
    return this.connected;
  }
}

class InstanceNode extends ExplorerNode {
  contextValue = 'drasi.instanceNode';
  instance: InstanceListItem;
  private current: boolean;

  constructor(instance: InstanceListItem, current: boolean) {
    super(instance.id, current ? vscode.TreeItemCollapsibleState.Expanded : vscode.TreeItemCollapsibleState.None);
    this.instance = instance;
    this.current = current;
  }

  public get isCurrent() {
    return this.current;
  }
}

enum Category {
  queries,
  sources,
  reactions
}

class CategoryNode extends ExplorerNode {
  contextValue = 'drasi.categoryNode';
  category: Category;
  instance: InstanceListItem;

  constructor(category: Category, instance: InstanceListItem) {
    let label = '';
    switch (category) {
      case Category.sources:
        label = 'Sources';
        break;
      case Category.queries:
        label = 'Queries';
        break;
      case Category.reactions:
        label = 'Reactions';
        break;
    }
    super(label, vscode.TreeItemCollapsibleState.Expanded);
    this.category = category;
    this.instance = instance;
  }
}

class ResourceNode extends ExplorerNode {
  kind: 'source' | 'query' | 'reaction';
  component: ComponentListItem;
  instance: InstanceListItem;

  constructor(kind: 'source' | 'query' | 'reaction', component: ComponentListItem, instance: InstanceListItem) {
    super(component.id, vscode.TreeItemCollapsibleState.None);
    this.kind = kind;
    this.component = component;
    this.instance = instance;
    this.description = component.status;
    if (component.error_message) {
      this.tooltip = component.error_message;
    }
  }
}

class QueryNode extends ResourceNode {
  contextValue = 'drasi.queryNode';

  constructor(query: ComponentListItem, instance: InstanceListItem) {
    super('query', query, instance);
    this.iconPath = statusIcon(query.status, 'code');
  }
}

class SourceNode extends ResourceNode {
  contextValue = 'drasi.sourceNode';

  constructor(source: ComponentListItem, instance: InstanceListItem) {
    super('source', source, instance);
    this.iconPath = statusIcon(source.status, 'database');
  }
}

class ReactionNode extends ResourceNode {
  contextValue = 'drasi.reactionNode';

  constructor(reaction: ComponentListItem, instance: InstanceListItem) {
    super('reaction', reaction, instance);
    this.iconPath = statusIcon(reaction.status, 'symbol-event');
  }
}

function statusIcon(status: ComponentStatus, icon: string) {
  const normalized = normalizeStatus(status);
  switch (normalized) {
    case 'Running':
      return new vscode.ThemeIcon(icon, new vscode.ThemeColor('testing.iconPassed'));
    case 'Error':
    case 'Failed':
    case 'TerminalError':
      return new vscode.ThemeIcon(icon, new vscode.ThemeColor('testing.iconFailed'));
    default:
      return new vscode.ThemeIcon(icon, new vscode.ThemeColor('testing.iconQueued'));
  }
}

function normalizeStatus(status: ComponentStatus): ComponentStatus {
  return status ?? 'Unknown';
}

async function deleteByKind(client: DrasiClient, kind: string, id: string) {
  switch (kind) {
    case 'source':
      await client.deleteSource(id);
      break;
    case 'query':
      await client.deleteQuery(id);
      break;
    case 'reaction':
      await client.deleteReaction(id);
      break;
    default:
      throw new Error(`Unsupported resource kind: ${kind}`);
  }
}
