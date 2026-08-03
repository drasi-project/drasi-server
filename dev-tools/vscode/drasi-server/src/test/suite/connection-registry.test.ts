import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { ConnectionRegistry, ServerConnectionConfig } from '../../sdk/config';

const SECTION = 'drasiServer';

function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function clearConnectionSettings() {
  const config = vscode.workspace.getConfiguration(SECTION);
  await config.update('connections', undefined, vscode.ConfigurationTarget.Global);
  await config.update('currentConnectionId', undefined, vscode.ConfigurationTarget.Global);

  // Best-effort workspace cleanup (may no-op when settings are application-scoped).
  try {
    await config.update('connections', undefined, vscode.ConfigurationTarget.Workspace);
    await config.update('currentConnectionId', undefined, vscode.ConfigurationTarget.Workspace);
  } catch {
    // ignore
  }

  const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (workspaceRoot) {
    const settingsPath = path.join(workspaceRoot, '.vscode', 'settings.json');
    if (fs.existsSync(settingsPath)) {
      try {
        const raw = JSON.parse(fs.readFileSync(settingsPath, 'utf8')) as Record<string, unknown>;
        delete raw['drasiServer.connections'];
        delete raw['drasiServer.currentConnectionId'];
        fs.writeFileSync(settingsPath, `${JSON.stringify(raw, null, 2)}\n`);
      } catch {
        // ignore corrupt/missing settings during cleanup
      }
    }
  }
}

suite('ConnectionRegistry settings scope', () => {
  setup(async () => {
    await clearConnectionSettings();
  });

  teardown(async () => {
    await clearConnectionSettings();
  });

  test('writes connections and currentConnectionId to Global, not Workspace', async () => {
    assert.ok(
      vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders.length > 0,
      'Test host must open a workspace folder'
    );

    const registry = new ConnectionRegistry();
    const connection = await registry.addConnection('Test Server', 'http://example.test:9090');

    const config = vscode.workspace.getConfiguration(SECTION);
    const connectionsInspect = config.inspect<ServerConnectionConfig[]>('connections');
    const currentIdInspect = config.inspect<string>('currentConnectionId');

    assert.ok(connectionsInspect?.globalValue, 'connections should be stored in global/user settings');
    assert.strictEqual(
      connectionsInspect?.workspaceValue,
      undefined,
      'connections must not be workspace-scoped'
    );
    assert.strictEqual(connectionsInspect?.globalValue?.[0]?.url, 'http://example.test:9090');

    assert.strictEqual(currentIdInspect?.globalValue, connection.id);
    assert.strictEqual(
      currentIdInspect?.workspaceValue,
      undefined,
      'currentConnectionId must not be workspace-scoped'
    );
  });

  test('ensureDefaultConnection does not write workspace settings', async () => {
    const registry = new ConnectionRegistry();
    await registry.ensureDefaultConnection();

    const config = vscode.workspace.getConfiguration(SECTION);
    const connectionsInspect = config.inspect<ServerConnectionConfig[]>('connections');
    const currentIdInspect = config.inspect<string>('currentConnectionId');

    assert.ok(connectionsInspect?.globalValue && connectionsInspect.globalValue.length > 0);
    assert.strictEqual(connectionsInspect?.workspaceValue, undefined);
    assert.ok(currentIdInspect?.globalValue);
    assert.strictEqual(currentIdInspect?.workspaceValue, undefined);
  });

  test('migrateWorkspaceSettingsToGlobal is a no-op when workspace is clean', async () => {
    const registry = new ConnectionRegistry();
    const migrated = await registry.migrateWorkspaceSettingsToGlobal();
    assert.strictEqual(migrated, false);
  });

  test('migrates workspace connections seeded via settings.json when visible to inspect', async function () {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const workspaceConnections: ServerConnectionConfig[] = [
      {
        id: 'ws-conn-1',
        name: 'Workspace Server',
        url: 'http://workspace.test:8080',
      },
    ];

    const vscodeDir = path.join(workspaceRoot, '.vscode');
    fs.mkdirSync(vscodeDir, { recursive: true });
    const settingsPath = path.join(vscodeDir, 'settings.json');
    const existing = fs.existsSync(settingsPath)
      ? (JSON.parse(fs.readFileSync(settingsPath, 'utf8')) as Record<string, unknown>)
      : {};
    existing['drasiServer.connections'] = workspaceConnections;
    existing['drasiServer.currentConnectionId'] = 'ws-conn-1';
    fs.writeFileSync(settingsPath, `${JSON.stringify(existing, null, 2)}\n`);

    // Give VS Code a moment to notice the settings file change.
    await delay(500);

    const config = vscode.workspace.getConfiguration(SECTION);
    const before = config.inspect<ServerConnectionConfig[]>('connections');
    if (!before?.workspaceValue) {
      // application-scoped settings may hide workspace values from inspect/get.
      // In that case migration is unnecessary for correctness; skip.
      this.skip();
      return;
    }

    const registry = new ConnectionRegistry();
    const migrated = await registry.migrateWorkspaceSettingsToGlobal();
    assert.strictEqual(migrated, true);

    const connectionsInspect = config.inspect<ServerConnectionConfig[]>('connections');
    const currentIdInspect = config.inspect<string>('currentConnectionId');

    assert.deepStrictEqual(connectionsInspect?.globalValue, workspaceConnections);
    assert.strictEqual(connectionsInspect?.workspaceValue, undefined);
    assert.strictEqual(currentIdInspect?.globalValue, 'ws-conn-1');
    assert.strictEqual(currentIdInspect?.workspaceValue, undefined);
  });
});
