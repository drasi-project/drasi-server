import * as vscode from 'vscode';
import { randomUUID } from 'crypto';

export interface ServerConnectionConfig {
  id: string;
  name: string;
  url: string;
  instanceId?: string;
}

export class ConnectionRegistry {
  private readonly configurationSection = 'drasiServer';

  async ensureDefaultConnection(): Promise<ServerConnectionConfig> {
    const connections = this.getConnections();
    if (connections.length > 0) {
      return this.getCurrentConnection();
    }

    const config = vscode.workspace.getConfiguration(this.configurationSection);
    const url = config.get<string>('url') ?? 'http://localhost:8080';
    const instanceId = config.get<string>('instanceId') ?? '';
    const connection: ServerConnectionConfig = {
      id: randomUUID(),
      name: 'Local Drasi Server',
      url: url.replace(/\/$/, ''),
      instanceId: instanceId || undefined,
    };

    await this.setConnections([connection]);
    await this.setCurrentConnectionId(connection.id);
    return connection;
  }

  getConnections(): ServerConnectionConfig[] {
    const config = vscode.workspace.getConfiguration(this.configurationSection);
    return config.get<ServerConnectionConfig[]>('connections') ?? [];
  }

  getCurrentConnectionId(): string | undefined {
    const config = vscode.workspace.getConfiguration(this.configurationSection);
    return config.get<string>('currentConnectionId') ?? undefined;
  }

  getCurrentConnection(): ServerConnectionConfig {
    const connections = this.getConnections();
    const currentId = this.getCurrentConnectionId();
    const found = connections.find((connection) => connection.id === currentId);
    if (found) {
      return found;
    }
    if (connections.length > 0) {
      return connections[0];
    }

    return {
      id: 'default',
      name: 'Local Drasi Server',
      url: 'http://localhost:8080',
    };
  }

  async addConnection(name: string, url: string) {
    const connections = this.getConnections();
    const connection: ServerConnectionConfig = {
      id: randomUUID(),
      name,
      url: url.replace(/\/$/, ''),
    };
    connections.push(connection);
    await this.setConnections(connections);
    await this.setCurrentConnectionId(connection.id);
    return connection;
  }

  async updateCurrentConnectionUrl(url: string) {
    const connections = this.getConnections();
    const currentId = this.getCurrentConnectionId();
    const updated = connections.map((connection) => {
      if (connection.id !== currentId) {
        return connection;
      }
      return {
        ...connection,
        url: url.replace(/\/$/, ''),
      };
    });
    await this.setConnections(updated);
  }

  async setCurrentConnectionId(connectionId: string) {
    const config = vscode.workspace.getConfiguration(this.configurationSection);
    await config.update('currentConnectionId', connectionId, vscode.ConfigurationTarget.Workspace);
  }

  async setCurrentInstanceId(instanceId: string) {
    const connections = this.getConnections();
    const currentId = this.getCurrentConnectionId();
    const updated = connections.map((connection) => {
      if (connection.id !== currentId) {
        return connection;
      }
      return {
        ...connection,
        instanceId: instanceId || undefined,
      };
    });
    await this.setConnections(updated);
  }

  private async setConnections(connections: ServerConnectionConfig[]) {
    const config = vscode.workspace.getConfiguration(this.configurationSection);
    await config.update('connections', connections, vscode.ConfigurationTarget.Workspace);
  }
}
