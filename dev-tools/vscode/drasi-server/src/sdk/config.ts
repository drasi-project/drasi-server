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

  /**
   * Move personal connection settings from workspace scope to user (global)
   * scope and clear any leftover workspace values so they stop shadowing.
   *
   * Safe to call repeatedly — no-ops when workspace has nothing set.
   */
  async migrateWorkspaceSettingsToGlobal(): Promise<boolean> {
    const config = vscode.workspace.getConfiguration(this.configurationSection);
    let migrated = false;

    migrated =
      (await this.migrateSettingToGlobal<ServerConnectionConfig[]>(config, 'connections', (value) =>
        Array.isArray(value) && value.length > 0
      )) || migrated;

    migrated =
      (await this.migrateSettingToGlobal<string>(config, 'currentConnectionId', (value) =>
        typeof value === 'string' && value.length > 0
      )) || migrated;

    return migrated;
  }

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
    await config.update('currentConnectionId', connectionId, vscode.ConfigurationTarget.Global);
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
    await config.update('connections', connections, vscode.ConfigurationTarget.Global);
  }

  private async migrateSettingToGlobal<T>(
    config: vscode.WorkspaceConfiguration,
    key: string,
    isPresent: (value: T | undefined) => boolean
  ): Promise<boolean> {
    const inspected = config.inspect<T>(key);
    if (!inspected) {
      return false;
    }

    const workspaceValue = inspected.workspaceValue;
    const workspaceFolderValue = inspected.workspaceFolderValue;
    const hasWorkspace = isPresent(workspaceValue);
    const hasWorkspaceFolder = isPresent(workspaceFolderValue);

    if (!hasWorkspace && !hasWorkspaceFolder) {
      return false;
    }

    // Prefer existing user settings; only promote workspace values when global is empty.
    if (!isPresent(inspected.globalValue)) {
      const valueToPromote = hasWorkspace ? workspaceValue : workspaceFolderValue;
      await config.update(key, valueToPromote, vscode.ConfigurationTarget.Global);
    }

    // Clearing may fail for application-scoped settings; ignore so activation still succeeds.
    if (hasWorkspace) {
      try {
        await config.update(key, undefined, vscode.ConfigurationTarget.Workspace);
      } catch {
        // ignore
      }
    }
    if (hasWorkspaceFolder) {
      try {
        await config.update(key, undefined, vscode.ConfigurationTarget.WorkspaceFolder);
      } catch {
        // ignore
      }
    }

    return true;
  }
}
